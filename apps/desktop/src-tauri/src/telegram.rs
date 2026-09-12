//! Telegram как второй канал управления (ТЗ §28, `docs/REMOTE-CONTROL.md` §5).
//!
//! # Почему long polling, а не webhook
//!
//! Webhook требует открытого порта или туннеля наружу. `docs/REMOTE-CONTROL.md`
//! §1 запрещает «просто HTTP на локальный порт» прямым текстом: открытый порт в
//! домашней сети доступен всему, что в этой сети есть. Long polling — исходящее
//! соединение к Telegram, и с точки зрения сети Yuki остаётся клиентом.
//!
//! # Что этот канал не защищает
//!
//! Сообщения проходят через серверы Telegram. Сквозного шифрования между
//! телефоном и машиной здесь нет и быть не может: посредник по построению
//! видит текст. Это сказано пользователю прямо в настройках, а не спрятано в
//! документации, — и по той же причине канал несовместим с режимом Local Only
//! (ТЗ §29): «работать частично» здесь означало бы отправлять наружу то, что
//! человек запретил отправлять.
//!
//! # Кто выполняет просьбы
//!
//! Не этот модуль. Он принимает сообщение, проверяет, что чат сопряжён, и
//! отдаёт текст наверх событием. Отвечает агентный цикл — тот самый, что
//! работает в окне, со теми же инструментами и тем же Permission Gate. Иначе
//! рядом с локальным путём вырос бы второй, и разрешения в нём пришлось бы
//! проверять заново.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::privacy;
use crate::secrets;
use crate::state::AppState;

/// Ключ секрета с токеном бота. Токен — это полный доступ к боту, и в базе
/// его быть не может (ТЗ §29).
const TOKEN_REF: &str = "telegram:bot";

/// Ключи настроек.
const KEY_ENABLED: &str = "telegram.enabled";
/// Смещение `getUpdates`: без него после перезапуска прилетели бы заново все
/// сообщения за сутки, и Yuki выполнила бы их повторно.
const KEY_OFFSET: &str = "telegram.offset";

/// Канал в таблице устройств.
const CHANNEL: &str = "telegram";

/// Событие «пришло сообщение из сопряжённого чата».
pub const MESSAGE_EVENT: &str = "yuki://telegram-message";

/// Событие «незнакомый чат назвал код сопряжения».
pub const PAIRING_EVENT: &str = "yuki://telegram-pairing";

/// Сколько живёт код сопряжения, секунды.
///
/// Пять минут, а не сутки: `docs/REMOTE-CONTROL.md` §2 требует минут, потому
/// что код, действующий сутки, успевает попасть в чужой скриншот.
const PAIRING_TTL: u64 = 300;

/// Сколько Telegram держит соединение, секунды.
const POLL_TIMEOUT: u64 = 30;

/// Работает ли опрос прямо сейчас.
static RUNNING: AtomicBool = AtomicBool::new(false);

/// Выданный код сопряжения. Одноразовый: использован — стёрт.
static PAIRING: Mutex<Option<Pairing>> = Mutex::new(None);

#[derive(Debug, Clone)]
struct Pairing {
    code: String,
    expires_at: u64,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// Сопряжённое устройство.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteDevice {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    pub last_seen: Option<i64>,
}

/// Состояние канала.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    /// Включён ли канал в настройках.
    pub enabled: bool,
    /// Задан ли токен бота.
    pub has_token: bool,
    /// Идёт ли опрос прямо сейчас.
    pub running: bool,
    /// Включён ли Local Only — при нём канал работать не может.
    pub local_only: bool,
    pub devices: Vec<RemoteDevice>,
    /// Действующий код сопряжения, если он выдан.
    pub pairing_code: Option<String>,
    /// Сколько секунд коду осталось жить.
    pub pairing_seconds_left: Option<u64>,
}

// ── Настройки ───────────────────────────────────────────────────────────────────

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .ok()
        .flatten()
}

fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![key, value],
            )
        })
        .map(|_| ())
        .map_err(err)
}

fn devices(state: &AppState) -> Vec<RemoteDevice> {
    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, created_at, last_seen FROM remote_devices
                 WHERE channel = ?1 ORDER BY created_at DESC",
            )?;

            let rows = stmt.query_map([CHANNEL], |row| {
                Ok(RemoteDevice {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    last_seen: row.get(3)?,
                })
            })?;

            rows.collect()
        })
        .unwrap_or_default()
}

fn is_paired(state: &AppState, chat_id: &str) -> Option<String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT name FROM remote_devices WHERE channel = ?1 AND id = ?2",
                [CHANNEL, chat_id],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .ok()
        .flatten()
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn telegram_status(state: State<'_, AppState>) -> TelegramStatus {
    let pairing = PAIRING.lock().ok().and_then(|guard| guard.clone());
    let alive = pairing.filter(|p| p.expires_at > now());

    TelegramStatus {
        enabled: setting(&state, KEY_ENABLED).as_deref() == Some("on"),
        has_token: secrets::exists(TOKEN_REF),
        running: RUNNING.load(Ordering::Relaxed),
        local_only: privacy::is_local_only(&state.storage),
        devices: devices(&state),
        pairing_seconds_left: alive.as_ref().map(|p| p.expires_at.saturating_sub(now())),
        pairing_code: alive.map(|p| p.code),
    }
}

/// Сохраняет токен бота, проверив его у Telegram.
///
/// Проверка обязательна: токен с опечаткой иначе обнаружился бы только тем, что
/// канал молча не работает, а искать причину человек пошёл бы в настройки сети.
#[tauri::command]
pub async fn telegram_set_token(app: AppHandle, token: String) -> Result<String, String> {
    let token = token.trim().to_string();
    if token.is_empty() {
        return Err("токен пустой".into());
    }

    let client = {
        let state = app.state::<AppState>();
        if privacy::is_local_only(&state.storage) {
            return Err(
                "включён режим Local Only: сообщения Telegram идут через чужие серверы, \
                 и канал с ним несовместим"
                    .into(),
            );
        }
        state.http.clone()
    };

    let response = client
        .get(format!("https://api.telegram.org/bot{token}/getMe"))
        .send()
        .await
        .map_err(|e| format!("не удалось связаться с Telegram: {e}"))?;

    let body: serde_json::Value = response.json().await.map_err(err)?;

    if body["ok"].as_bool() != Some(true) {
        let reason = body["description"].as_str().unwrap_or("токен не принят");
        return Err(format!("Telegram отказал: {reason}"));
    }

    let name = body["result"]["username"]
        .as_str()
        .unwrap_or("бот")
        .to_string();

    secrets::set(TOKEN_REF, &token).map_err(err)?;
    Ok(name)
}

/// Убирает токен и останавливает канал.
#[tauri::command]
pub fn telegram_clear_token(state: State<'_, AppState>) -> Result<(), String> {
    RUNNING.store(false, Ordering::Relaxed);
    set_setting(&state, KEY_ENABLED, "off")?;
    secrets::delete(TOKEN_REF).map_err(err)
}

/// Выдаёт одноразовый код сопряжения.
#[tauri::command]
pub fn telegram_pair(state: State<'_, AppState>) -> Result<String, String> {
    if !secrets::exists(TOKEN_REF) {
        return Err("сначала нужен токен бота".into());
    }

    let code = pairing_code();

    let mut guard = PAIRING.lock().map_err(|_| "состояние сопряжения повреждено")?;
    *guard = Some(Pairing {
        code: code.clone(),
        expires_at: now() + PAIRING_TTL,
    });

    // Канал должен работать, иначе код некому услышать.
    let _ = setting(&state, KEY_ENABLED);
    Ok(code)
}

/// Подтверждает сопряжение — на рабочей машине, а не на телефоне.
///
/// `docs/REMOTE-CONTROL.md` §2: иначе первый, кто увидел код, становится
/// владельцем.
#[tauri::command]
pub fn telegram_approve(
    state: State<'_, AppState>,
    chat_id: String,
    name: String,
) -> Result<Vec<RemoteDevice>, String> {
    let name = if name.trim().is_empty() {
        format!("чат {chat_id}")
    } else {
        name.trim().to_string()
    };

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO remote_devices (id, channel, name) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name",
                rusqlite::params![chat_id, CHANNEL, name],
            )
        })
        .map_err(err)?;

    Ok(devices(&state))
}

/// Отзывает одно устройство. Немедленно и без его согласия.
#[tauri::command]
pub fn telegram_revoke(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<RemoteDevice>, String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "DELETE FROM remote_devices WHERE channel = ?1 AND id = ?2",
                [CHANNEL, &id],
            )
        })
        .map_err(err)?;

    Ok(devices(&state))
}

/// Отзывает все устройства — на случай украденного телефона.
#[tauri::command]
pub fn telegram_revoke_all(state: State<'_, AppState>) -> Result<Vec<RemoteDevice>, String> {
    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM remote_devices WHERE channel = ?1", [CHANNEL]))
        .map_err(err)?;

    Ok(devices(&state))
}

/// Отправляет текст в чат.
///
/// Вызывается из окна: ответ сочиняет агентный цикл, а не этот модуль.
#[tauri::command]
pub async fn telegram_send(app: AppHandle, chat_id: String, text: String) -> Result<(), String> {
    let (client, token) = {
        let state = app.state::<AppState>();

        if is_paired(&state, &chat_id).is_none() {
            // Отправка в несопряжённый чат — это либо ошибка в коде, либо
            // попытка использовать Yuki как рассыльщика. Ни того, ни другого
            // быть не должно.
            return Err("чат не сопряжён".into());
        }

        (
            state.http.clone(),
            secrets::get(TOKEN_REF).map_err(err)?.ok_or("токен не задан")?,
        )
    };

    send(&client, &token, &chat_id, &text).await
}

async fn send(
    client: &reqwest::Client,
    token: &str,
    chat_id: &str,
    text: &str,
) -> Result<(), String> {
    let response = client
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&serde_json::json!({ "chat_id": chat_id, "text": text }))
        .send()
        .await
        .map_err(|e| format!("не удалось отправить сообщение: {e}"))?;

    let body: serde_json::Value = response.json().await.map_err(err)?;

    if body["ok"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err(body["description"]
            .as_str()
            .unwrap_or("Telegram отказал")
            .to_string())
    }
}

/// Включает канал и запускает опрос.
#[tauri::command]
pub fn telegram_start(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if privacy::is_local_only(&state.storage) {
        return Err(
            "включён режим Local Only: сообщения Telegram идут через чужие серверы, \
             и канал с ним несовместим"
                .into(),
        );
    }

    if !secrets::exists(TOKEN_REF) {
        return Err("сначала нужен токен бота".into());
    }

    set_setting(&state, KEY_ENABLED, "on")?;

    // Второй опрос того же бота получал бы половину сообщений: Telegram отдаёт
    // обновление одному запросу.
    if RUNNING.swap(true, Ordering::Relaxed) {
        return Ok(());
    }

    tauri::async_runtime::spawn(poll(app));
    Ok(())
}

#[tauri::command]
pub fn telegram_stop(state: State<'_, AppState>) -> Result<(), String> {
    RUNNING.store(false, Ordering::Relaxed);
    set_setting(&state, KEY_ENABLED, "off")
}

/// Поднимает канал на старте, если в прошлый раз он был включён.
pub fn restore(app: AppHandle) {
    let state = app.state::<AppState>();

    if setting(&state, KEY_ENABLED).as_deref() != Some("on") {
        return;
    }

    // Local Only мог включиться между запусками — тогда канал не поднимается,
    // и это не молчаливый отказ: в настройках видно, что он выключен режимом.
    if privacy::is_local_only(&state.storage) || !secrets::exists(TOKEN_REF) {
        return;
    }

    if !RUNNING.swap(true, Ordering::Relaxed) {
        tauri::async_runtime::spawn(poll(app.clone()));
    }
}

// ── Опрос ───────────────────────────────────────────────────────────────────────

/// Тело цикла опроса.
///
/// Ошибки сети не останавливают канал: интернет пропадает, Telegram бывает
/// недоступен, и канал, умирающий от одного неудачного запроса, пришлось бы
/// включать руками после каждого обрыва связи.
async fn poll(app: AppHandle) {
    let mut backoff = 1u64;

    while RUNNING.load(Ordering::Relaxed) {
        let (client, token, offset, local_only) = {
            let state = app.state::<AppState>();
            let token = match secrets::get(TOKEN_REF) {
                Ok(Some(token)) => token,
                _ => break,
            };

            (
                state.http.clone(),
                token,
                setting(&state, KEY_OFFSET)
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .unwrap_or(0),
                privacy::is_local_only(&state.storage),
            )
        };

        // Режим могли включить, пока канал работал. Останавливаемся сами:
        // «работать частично» здесь означало бы отправлять наружу то, что
        // человек запретил отправлять.
        if local_only {
            RUNNING.store(false, Ordering::Relaxed);
            let state = app.state::<AppState>();
            let _ = set_setting(&state, KEY_ENABLED, "off");
            break;
        }

        match updates(&client, &token, offset).await {
            Ok(list) => {
                backoff = 1;
                for update in list {
                    handle(&app, &client, &token, update).await;
                }
            }
            Err(reason) => {
                tracing::warn!("Telegram: {reason}");
                tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
                // До минуты: чаще стучаться в недоступный сервис бессмысленно,
                // реже — значит не заметить, что связь вернулась.
                backoff = (backoff * 2).min(60);
            }
        }
    }

    RUNNING.store(false, Ordering::Relaxed);
}

/// Одно обновление в том виде, в каком он нам нужен.
struct Update {
    id: i64,
    chat_id: String,
    /// Имя для списка устройств: `@username`, иначе имя человека.
    name: String,
    text: String,
}

async fn updates(
    client: &reqwest::Client,
    token: &str,
    offset: i64,
) -> Result<Vec<Update>, String> {
    let response = client
        .get(format!("https://api.telegram.org/bot{token}/getUpdates"))
        .query(&[
            ("offset", offset.to_string()),
            ("timeout", POLL_TIMEOUT.to_string()),
            // Только сообщения: нажатия кнопок и правки нам не нужны, а
            // просить всё значит разбирать то, на что мы всё равно не ответим.
            ("allowed_updates", "[\"message\"]".into()),
        ])
        // Собственный таймаут длиннее серверного: иначе клиент разорвёт
        // соединение раньше, чем Telegram успеет ответить пустым списком.
        .timeout(std::time::Duration::from_secs(POLL_TIMEOUT + 15))
        .send()
        .await
        .map_err(|e| e.to_string())?;

    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;

    if body["ok"].as_bool() != Some(true) {
        return Err(body["description"]
            .as_str()
            .unwrap_or("getUpdates отказал")
            .to_string());
    }

    Ok(body["result"]
        .as_array()
        .map(|list| list.iter().filter_map(parse).collect())
        .unwrap_or_default())
}

fn parse(raw: &serde_json::Value) -> Option<Update> {
    let id = raw["update_id"].as_i64()?;
    let message = &raw["message"];
    let chat = &message["chat"];
    let chat_id = chat["id"].as_i64()?.to_string();
    let text = message["text"].as_str()?.trim().to_string();

    if text.is_empty() {
        return None;
    }

    let name = chat["username"]
        .as_str()
        .map(|user| format!("@{user}"))
        .or_else(|| chat["first_name"].as_str().map(str::to_string))
        .unwrap_or_else(|| format!("чат {chat_id}"));

    Some(Update {
        id,
        chat_id,
        name,
        text,
    })
}

async fn handle(app: &AppHandle, client: &reqwest::Client, token: &str, update: Update) {
    // Смещение двигается до обработки, а не после: сообщение, на котором
    // обработка падает, иначе приходило бы снова и снова в бесконечном круге.
    {
        let state = app.state::<AppState>();
        let _ = set_setting(&state, KEY_OFFSET, &(update.id + 1).to_string());
    }

    let paired = {
        let state = app.state::<AppState>();
        is_paired(&state, &update.chat_id)
    };

    if let Some(name) = paired {
        {
            let state = app.state::<AppState>();
            let _ = state.storage.with_conn(|conn| {
                conn.execute(
                    "UPDATE remote_devices SET last_seen = unixepoch()
                     WHERE channel = ?1 AND id = ?2",
                    [CHANNEL, &update.chat_id],
                )
            });
        }

        // Выполняет окно: там агентный цикл, инструменты и Permission Gate.
        let _ = app.emit(
            MESSAGE_EVENT,
            serde_json::json!({
                "chatId": update.chat_id,
                "name": name,
                "text": update.text,
            }),
        );

        return;
    }

    // Незнакомый чат. Единственное, что он может сделать, — назвать код.
    if consume_code(&update.text) {
        let _ = app.emit(
            PAIRING_EVENT,
            serde_json::json!({
                "chatId": update.chat_id,
                "name": update.name,
            }),
        );

        let _ = send(
            client,
            token,
            &update.chat_id,
            "Код принят. Подтвердите подключение на компьютере — \n             без подтверждения доступ не откроется.",
        )
        .await;

        return;
    }

    let _ = send(
        client,
        token,
        &update.chat_id,
        "Этот чат не подключён. Откройте настройки Yuki, получите код \
         сопряжения и пришлите его сюда.",
    )
    .await;
}

/// Сверяет присланный текст с выданным кодом и сжигает код при совпадении.
///
/// Сжигается именно при совпадении, а не при подтверждении на машине:
/// `docs/REMOTE-CONTROL.md` §2 требует одноразовости, а «код сработал, но
/// хозяин отказал» — это уже использованный код.
fn consume_code(text: &str) -> bool {
    let mut guard = match PAIRING.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };

    let Some(pairing) = guard.as_ref() else {
        return false;
    };

    if pairing.expires_at <= now() {
        *guard = None;
        return false;
    }

    if !matches(&pairing.code, text) {
        return false;
    }

    *guard = None;
    true
}

/// Сравнение кода за постоянное время.
///
/// Обычное `==` выходит на первом несовпавшем знаке, и по времени ответа код
/// подбирается знак за знаком. Шесть цифр — это миллион вариантов, то есть
/// перебор и без утечки возможен, но пять минут жизни кода делают его дорогим,
/// а утечка по времени вернула бы стоимость к шести попыткам.
fn matches(code: &str, given: &str) -> bool {
    let code = code.as_bytes();
    let given = given.trim().as_bytes();

    if code.len() != given.len() {
        return false;
    }

    let mut diff = 0u8;
    for (a, b) in code.iter().zip(given) {
        diff |= a ^ b;
    }

    diff == 0
}

/// Шестизначный код из системного источника случайности.
///
/// Время и счётчик здесь не годятся: код, выводимый из часов, предсказуем для
/// того, кто знает, когда его запросили.
fn pairing_code() -> String {
    let mut bytes = [0u8; 4];
    getrandom(&mut bytes);
    let value = u32::from_le_bytes(bytes) % 1_000_000;
    format!("{value:06}")
}

/// Случайные байты от ОС.
fn getrandom(buffer: &mut [u8]) {
    // `ring`/`rand` тянуть ради четырёх байт не стоит: у обеих систем есть
    // собственный источник, и он и есть источник по умолчанию.
    #[cfg(windows)]
    {
        use std::os::raw::c_void;
        #[link(name = "bcrypt")]
        extern "system" {
            fn BCryptGenRandom(
                algorithm: *mut c_void,
                buffer: *mut u8,
                length: u32,
                flags: u32,
            ) -> i32;
        }
        // BCRYPT_USE_SYSTEM_PREFERRED_RNG: системный генератор без открытия
        // провайдера.
        let status = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0x0000_0002,
            )
        };
        assert_eq!(status, 0, "системный генератор случайных чисел отказал");
    }

    #[cfg(not(windows))]
    {
        use std::io::Read;
        std::fs::File::open("/dev/urandom")
            .and_then(|mut file| file.read_exact(buffer))
            .expect("системный генератор случайных чисел отказал");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Код — шесть цифр и ничего больше.
    #[test]
    fn the_pairing_code_is_six_digits() {
        for _ in 0..200 {
            let code = pairing_code();
            assert_eq!(code.len(), 6, "код {code} не шестизначный");
            assert!(code.chars().all(|c| c.is_ascii_digit()), "в коде {code} не цифра");
        }
    }

    /// Коды не повторяются подряд — то есть генератор действительно случайный.
    ///
    /// Тест на выводимость из часов: код, собранный из времени, на двух
    /// соседних вызовах давал бы соседние числа.
    #[test]
    fn pairing_codes_do_not_repeat() {
        let made: std::collections::BTreeSet<_> = (0..64).map(|_| pairing_code()).collect();
        assert!(made.len() > 50, "слишком много повторов: {}", made.len());
    }

    /// Сравнение кода не зависит от того, где он разошёлся.
    #[test]
    fn code_comparison_accepts_only_the_exact_code() {
        assert!(matches("123456", "123456"));
        assert!(matches("123456", "  123456  "), "пробелы по краям не считаются");

        assert!(!matches("123456", "123457"));
        assert!(!matches("123456", "023456"));
        assert!(!matches("123456", "12345"));
        assert!(!matches("123456", "1234567"));
        assert!(!matches("123456", ""));
    }

    /// Код одноразовый.
    #[test]
    fn a_code_works_once() {
        let code = "424242".to_string();
        *PAIRING.lock().unwrap() = Some(Pairing {
            code: code.clone(),
            expires_at: now() + 60,
        });

        assert!(consume_code(&code), "первый раз код должен сработать");
        assert!(!consume_code(&code), "второй раз — уже нет");
    }

    /// Просроченный код не срабатывает.
    #[test]
    fn an_expired_code_does_not_work() {
        let code = "515151".to_string();
        *PAIRING.lock().unwrap() = Some(Pairing {
            code: code.clone(),
            expires_at: now() - 1,
        });

        assert!(!consume_code(&code));
    }

    /// Разбор обновления берёт чат, текст и имя.
    #[test]
    fn an_update_is_read_into_what_we_need() {
        let raw = serde_json::json!({
            "update_id": 17,
            "message": {
                "text": "  привет  ",
                "chat": { "id": -100200, "username": "alex" }
            }
        });

        let update = parse(&raw).expect("должно разобраться");
        assert_eq!(update.id, 17);
        assert_eq!(update.chat_id, "-100200");
        assert_eq!(update.text, "привет", "пробелы по краям снимаются");
        assert_eq!(update.name, "@alex");
    }

    /// Без имени пользователя берётся имя человека, иначе номер чата.
    #[test]
    fn a_chat_without_a_username_still_gets_a_name() {
        let by_first_name = serde_json::json!({
            "update_id": 1,
            "message": { "text": "да", "chat": { "id": 5, "first_name": "Алекс" } }
        });
        assert_eq!(parse(&by_first_name).unwrap().name, "Алекс");

        let nameless = serde_json::json!({
            "update_id": 2,
            "message": { "text": "да", "chat": { "id": 7 } }
        });
        assert_eq!(parse(&nameless).unwrap().name, "чат 7");
    }

    /// Не сообщения пропускаются молча.
    ///
    /// Картинка, наклейка и пустой текст — это не просьба; пытаться понять их
    /// как команду значит выполнять неизвестно что.
    #[test]
    fn updates_without_text_are_skipped() {
        for raw in [
            serde_json::json!({ "update_id": 1, "message": { "chat": { "id": 5 } } }),
            serde_json::json!({ "update_id": 2, "message": { "text": "   ", "chat": { "id": 5 } } }),
            serde_json::json!({ "update_id": 3, "edited_message": { "text": "да", "chat": { "id": 5 } } }),
            serde_json::json!({ "message": { "text": "да", "chat": { "id": 5 } } }),
        ] {
            assert!(parse(&raw).is_none(), "не должно разбираться: {raw}");
        }
    }
}
