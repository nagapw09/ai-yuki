//! Подключение календарей (ТЗ §25).
//!
//! # Как устроен вход
//!
//! Системный браузер + возврат на loopback (RFC 8252). Показывать форму входа
//! Google или Microsoft внутри окна Yuki нельзя: в чужом WebView она неотличима
//! от подделки, и оба сервиса это прямо запрещают. Поэтому Yuki открывает
//! настоящий браузер, поднимает на 127.0.0.1 одноразовый сервер и ждёт на нём
//! один ответ.
//!
//! # Что где лежит
//!
//! `client_id` — обычная настройка, он не секрет. `client_secret` (у Google
//! настольных клиентов он есть, но конфиденциальным не считается) и
//! refresh-токен — только в хранилище ОС (ТЗ §29). Access-токен не сохраняется
//! никуда: он живёт час, лежит в памяти процесса и после перезапуска
//! запрашивается заново по refresh-токену.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::State;
use yuki_calendar::{
    oauth, CalendarClient, CalendarEvent, CalendarProvider, EventDraft, Tokens,
};

use crate::state::AppState;

/// Сколько ждём, пока человек закончит вход в браузере.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn refresh_ref(provider: CalendarProvider) -> String {
    format!("calendar:{}:refresh", provider.as_str())
}

fn secret_ref(provider: CalendarProvider) -> String {
    format!("calendar:{}:client_secret", provider.as_str())
}

/// Access-токены, полученные в этом запуске.
///
/// Отдельно от базы намеренно: access-токен — это ключ доступа со сроком жизни
/// в час, и записывать его на диск ради экономии одного обновления не стоит.
#[derive(Default)]
pub struct TokenCache {
    tokens: Mutex<HashMap<String, Tokens>>,
}

impl TokenCache {
    fn get(&self, provider: CalendarProvider) -> Option<Tokens> {
        self.tokens
            .lock()
            .ok()?
            .get(provider.as_str())
            .cloned()
    }

    fn put(&self, provider: CalendarProvider, tokens: Tokens) {
        if let Ok(mut guard) = self.tokens.lock() {
            guard.insert(provider.as_str().to_string(), tokens);
        }
    }

    fn forget(&self, provider: CalendarProvider) {
        if let Ok(mut guard) = self.tokens.lock() {
            guard.remove(provider.as_str());
        }
    }
}

/// Состояние подключения для интерфейса.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarAccount {
    pub provider: String,
    pub label: String,
    /// Где завести приложение и взять client_id.
    pub console_url: String,
    pub client_id: String,
    pub connected: bool,
    pub connected_at: Option<i64>,
}

fn parse_provider(value: &str) -> Result<CalendarProvider, String> {
    CalendarProvider::from_str(value).ok_or_else(|| format!("неизвестный календарь: {value}"))
}

// ── Настройка и подключение ─────────────────────────────────────────────────────

#[tauri::command]
pub fn calendar_accounts(state: State<'_, AppState>) -> Result<Vec<CalendarAccount>, String> {
    let rows: HashMap<String, (String, Option<i64>)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT provider, client_id, connected_at FROM calendar_accounts")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    (r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?),
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    Ok([CalendarProvider::Google, CalendarProvider::Microsoft]
        .into_iter()
        .map(|provider| {
            let row = rows.get(provider.as_str());
            CalendarAccount {
                provider: provider.as_str().to_string(),
                label: provider.label().to_string(),
                console_url: provider.console_url().to_string(),
                client_id: row.map(|(id, _)| id.clone()).unwrap_or_default(),
                // «Подключён» — это наличие refresh-токена, а не строки в базе:
                // токен могли отозвать, и тогда подключения нет, что бы ни
                // хранилось локально.
                connected: crate::secrets::exists(&refresh_ref(provider)),
                connected_at: row.and_then(|(_, at)| *at),
            }
        })
        .collect())
}

/// Сохраняет учётные данные приложения, заведённого пользователем.
#[tauri::command]
pub fn calendar_set_client(
    state: State<'_, AppState>,
    provider: String,
    client_id: String,
    client_secret: Option<String>,
) -> Result<(), String> {
    let provider = parse_provider(&provider)?;

    if client_id.trim().is_empty() {
        return Err("client_id не может быть пустым".into());
    }

    match client_secret.as_deref().map(str::trim) {
        Some(secret) if !secret.is_empty() => {
            crate::secrets::set(&secret_ref(provider), secret).map_err(err)?
        }
        // Пустое поле — это «секрета нет», а не «оставить прежний»: у публичного
        // клиента Microsoft секрета не бывает вовсе, и хранить чужой остаток нельзя.
        _ => {
            let _ = crate::secrets::delete(&secret_ref(provider));
        }
    }

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO calendar_accounts (provider, client_id) VALUES (?1, ?2)
                 ON CONFLICT(provider) DO UPDATE SET client_id = excluded.client_id",
                rusqlite::params![provider.as_str(), client_id.trim()],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Проводит вход через браузер и сохраняет refresh-токен (ТЗ §25).
#[tauri::command]
pub async fn calendar_connect(
    state: State<'_, AppState>,
    provider: String,
) -> Result<CalendarAccount, String> {
    let provider = parse_provider(&provider)?;
    let (client_id, client_secret) = client_credentials(&state, provider)?;

    // Порт выбирает система: занятый фиксированный порт сделал бы вход
    // невозможным, а угадывать свободный — гадание.
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| {
        format!("не удалось открыть локальный порт для ответа браузера: {e}")
    })?;
    let port = listener.local_addr().map_err(err)?.port();

    // Google разрешает настольным клиентам любой порт на 127.0.0.1; Azure —
    // только на имени localhost. Разница не косметическая: неверная форма
    // адреса отвергается на стороне сервиса.
    let redirect_uri = match provider {
        CalendarProvider::Google => format!("http://127.0.0.1:{port}"),
        CalendarProvider::Microsoft => format!("http://localhost:{port}"),
    };

    let request = oauth::authorize(provider, &client_id, &redirect_uri);

    crate::commands::open_external(&request.url)?;

    let expected_state = request.state.clone();
    let code = tauri::async_runtime::spawn_blocking(move || {
        wait_for_code(listener, &expected_state)
    })
    .await
    .map_err(err)??;

    let tokens = oauth::exchange(
        provider,
        &state.http,
        &client_id,
        client_secret.as_deref(),
        &code,
        &request.pkce.verifier,
        &redirect_uri,
    )
    .await
    .map_err(err)?;

    let refresh = tokens.refresh_token.clone().ok_or(
        "сервис не выдал refresh-токен: подключение прожило бы час. \
         Проверьте, что приложение зарегистрировано как настольное (Desktop / Mobile).",
    )?;

    crate::secrets::set(&refresh_ref(provider), &refresh).map_err(err)?;
    state.calendar.put(provider, tokens);

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE calendar_accounts SET connected_at = unixepoch() WHERE provider = ?1",
                [provider.as_str()],
            )
        })
        .map_err(err)?;

    calendar_accounts(state)?
        .into_iter()
        .find(|a| a.provider == provider.as_str())
        .ok_or_else(|| "подключение не сохранилось".to_string())
}

#[tauri::command]
pub fn calendar_disconnect(state: State<'_, AppState>, provider: String) -> Result<(), String> {
    let provider = parse_provider(&provider)?;

    // Refresh-токен уходит из хранилища ОС вместе с подключением: оставленный,
    // он продолжал бы давать доступ к чужому календарю.
    let _ = crate::secrets::delete(&refresh_ref(provider));
    state.calendar.forget(provider);

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE calendar_accounts SET connected_at = NULL WHERE provider = ?1",
                [provider.as_str()],
            )
        })
        .map(|_| ())
        .map_err(err)
}

// ── События ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn calendar_events(
    state: State<'_, AppState>,
    provider: String,
    from: String,
    to: String,
) -> Result<Vec<CalendarEvent>, String> {
    let provider = parse_provider(&provider)?;
    let client = connected_client(&state, provider).await?;
    client.events(&from, &to).await.map_err(err)
}

#[tauri::command]
pub async fn calendar_create_event(
    state: State<'_, AppState>,
    provider: String,
    draft: EventDraft,
) -> Result<CalendarEvent, String> {
    let provider = parse_provider(&provider)?;
    let client = connected_client(&state, provider).await?;
    client.create(&draft).await.map_err(err)
}

#[tauri::command]
pub async fn calendar_delete_event(
    state: State<'_, AppState>,
    provider: String,
    id: String,
) -> Result<(), String> {
    let provider = parse_provider(&provider)?;
    let client = connected_client(&state, provider).await?;
    client.delete(&id).await.map_err(err)
}

// ── Внутреннее ──────────────────────────────────────────────────────────────────

fn client_credentials(
    state: &AppState,
    provider: CalendarProvider,
) -> Result<(String, Option<String>), String> {
    let client_id: String = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT client_id FROM calendar_accounts WHERE provider = ?1",
                [provider.as_str()],
                |r| r.get(0),
            )
        })
        .map_err(|_| {
            format!(
                "{} не настроен: заведите приложение на {} и укажите client_id в настройках",
                provider.label(),
                provider.console_url()
            )
        })?;

    Ok((
        client_id,
        crate::secrets::get(&secret_ref(provider)).ok().flatten(),
    ))
}

/// Возвращает клиента с действующим токеном, обновив его при необходимости.
async fn connected_client(
    state: &AppState,
    provider: CalendarProvider,
) -> Result<CalendarClient, String> {
    let now = oauth::now();

    if let Some(tokens) = state.calendar.get(provider) {
        if !oauth::needs_refresh(tokens.expires_at, now) {
            return Ok(CalendarClient::new(
                provider,
                tokens.access_token,
                state.http.clone(),
            ));
        }
    }

    let refresh_token = crate::secrets::get(&refresh_ref(provider))
        .map_err(err)?
        .ok_or_else(|| {
            format!(
                "{} не подключён: откройте настройки и войдите в аккаунт",
                provider.label()
            )
        })?;

    let (client_id, client_secret) = client_credentials(state, provider)?;

    let mut tokens = oauth::refresh(
        provider,
        &state.http,
        &client_id,
        client_secret.as_deref(),
        &refresh_token,
    )
    .await
    .map_err(err)?;

    // При обновлении новый refresh-токен приходит не всегда; прежний остаётся
    // в силе, и терять его нельзя.
    if let Some(fresh) = &tokens.refresh_token {
        crate::secrets::set(&refresh_ref(provider), fresh).map_err(err)?;
    } else {
        tokens.refresh_token = Some(refresh_token);
    }

    let access_token = tokens.access_token.clone();
    state.calendar.put(provider, tokens);

    Ok(CalendarClient::new(provider, access_token, state.http.clone()))
}

/// Ждёт возврата браузера и достаёт код авторизации.
///
/// Одноразовый сервер: приняли ответ — закрылись. Слушать дольше нужного значит
/// держать открытым порт, на который может прийти что угодно.
fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    listener
        .set_nonblocking(false)
        .map_err(|e| format!("локальный сервер: {e}"))?;

    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;

    loop {
        if std::time::Instant::now() > deadline {
            return Err("вход не был завершён за три минуты".into());
        }

        let (mut stream, _) = listener
            .accept()
            .map_err(|e| format!("локальный сервер: {e}"))?;

        // Достаточно первой строки: в ней и метод, и путь с параметрами.
        let mut first_line = String::new();
        BufReader::new(
            stream
                .try_clone()
                .map_err(|e| format!("локальный сервер: {e}"))?,
        )
        .read_line(&mut first_line)
        .map_err(|e| format!("локальный сервер: {e}"))?;

        let target = first_line.split_whitespace().nth(1).unwrap_or("");
        let params = query_params(target);

        // Браузер попутно просит favicon — это не ответ авторизации.
        if !params.contains_key("code") && !params.contains_key("error") {
            let _ = stream.write_all(http_response("Жду ответа авторизации…").as_bytes());
            continue;
        }

        let outcome = if let Some(error) = params.get("error") {
            Err(format!(
                "сервис отказал: {error}{}",
                params
                    .get("error_description")
                    .map(|d| format!(" — {d}"))
                    .unwrap_or_default()
            ))
        } else if params.get("state").map(String::as_str) != Some(expected_state) {
            // Чужой state означает, что ответ пришёл не на наш запрос. Принять
            // его — значит дать подсунуть себе чужой код авторизации.
            Err("ответ авторизации не совпал с запросом — вход отклонён".to_string())
        } else {
            Ok(params["code"].clone())
        };

        let page = match &outcome {
            Ok(_) => "Готово. Можно вернуться в Yuki и закрыть эту вкладку.",
            Err(reason) => reason.as_str(),
        };
        let _ = stream.write_all(http_response(page).as_bytes());
        let _ = stream.flush();

        return outcome;
    }
}

fn http_response(message: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Yuki</title>\
         <body style=\"font:16px system-ui;padding:3rem;color:#eee;background:#111\">{message}</body>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Разбирает параметры запроса из строки вида `/?code=...&state=...`.
fn query_params(target: &str) -> HashMap<String, String> {
    let Some((_, query)) = target.split_once('?') else {
        return HashMap::new();
    };

    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_string(), percent_decode(value)))
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                match u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_code_out_of_a_callback_request_line() {
        let params = query_params("/?code=4/0AX4&state=abc123&scope=calendar");
        assert_eq!(params["code"], "4/0AX4");
        assert_eq!(params["state"], "abc123");
    }

    #[test]
    fn decodes_escaped_values() {
        let params = query_params("/?error=access_denied&error_description=User%20said%20no");
        assert_eq!(params["error_description"], "User said no");
        // Плюс в значении — это пробел, а не знак.
        assert_eq!(query_params("/?a=one+two")["a"], "one two");
    }

    #[test]
    fn a_request_without_a_query_yields_nothing() {
        assert!(query_params("/favicon.ico").is_empty());
    }

    #[test]
    fn secret_references_are_namespaced_per_provider() {
        assert_eq!(refresh_ref(CalendarProvider::Google), "calendar:google:refresh");
        assert_eq!(
            secret_ref(CalendarProvider::Microsoft),
            "calendar:microsoft:client_secret"
        );
    }

    /// Стучится в локальный сервер так же, как это сделал бы браузер.
    fn knock(port: u16, target: &str) {
        use std::io::Read;
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("сервер должен слушать");
        stream
            .write_all(format!("GET {target} HTTP/1.1
Host: localhost

").as_bytes())
            .expect("запрос должен уйти");
        // Ответ вычитываем целиком: иначе сервер получит разорванное соединение.
        let mut sink = Vec::new();
        let _ = stream.read_to_end(&mut sink);
    }

    #[test]
    fn takes_the_code_from_a_callback_that_carries_the_expected_state() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("порт должен открыться");
        let port = listener.local_addr().expect("адрес").port();

        let caller = std::thread::spawn(move || {
            // Браузер сначала просит favicon — сервер обязан это пережить.
            knock(port, "/favicon.ico");
            knock(port, "/?code=auth-code-42&state=expected");
        });

        let code = wait_for_code(listener, "expected").expect("код должен вернуться");
        caller.join().expect("клиент должен завершиться");

        assert_eq!(code, "auth-code-42");
    }

    #[test]
    fn refuses_a_callback_whose_state_does_not_match() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("порт должен открыться");
        let port = listener.local_addr().expect("адрес").port();

        let caller = std::thread::spawn(move || knock(port, "/?code=stolen&state=someone-else"));

        // Чужой state — это ответ не на наш запрос: принять его значит дать
        // подсунуть себе чужой код авторизации.
        let outcome = wait_for_code(listener, "expected");
        caller.join().expect("клиент должен завершиться");

        assert!(outcome.is_err(), "чужой state должен быть отвергнут");
    }

    #[test]
    fn reports_the_reason_when_the_service_refuses() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("порт должен открыться");
        let port = listener.local_addr().expect("адрес").port();

        let caller = std::thread::spawn(move || {
            knock(port, "/?error=access_denied&error_description=User%20denied")
        });

        let outcome = wait_for_code(listener, "expected");
        caller.join().expect("клиент должен завершиться");

        let message = outcome.expect_err("отказ должен стать ошибкой");
        assert!(message.contains("access_denied"), "{message}");
        assert!(message.contains("User denied"), "{message}");
    }

    #[test]
    fn the_response_page_declares_its_length_in_bytes_not_characters() {
        // Кириллица в теле занимает больше байт, чем символов, и неверный
        // Content-Length оставил бы вкладку висеть в ожидании остатка.
        let response = http_response("Готово");
        let (head, body) = response.split_once("\r\n\r\n").expect("заголовки и тело");
        let declared: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .expect("заголовок длины")
            .parse()
            .expect("число");
        assert_eq!(declared, body.as_bytes().len());
    }
}
