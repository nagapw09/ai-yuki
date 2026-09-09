//! Режим Local Only (ТЗ §29).
//!
//! ТЗ требует режим, в котором Yuki работает без облачного AI: локальная модель,
//! локальное распознавание, локальный синтез и локальная память. Синтез и память
//! локальны всегда — речь синтезирует ОС, а база лежит рядом с приложением.
//! Ограничивать нужно то, что умеет уйти в сеть: провайдера модели и сервис
//! распознавания.
//!
//! # Что значит «локально»
//!
//! Только петлевой адрес. Сервер в той же локальной сети — это уже другая машина,
//! и данные до неё идут по проводу, который Yuki не контролирует. Формулировка
//! «локально» в ТЗ §29 стоит рядом с «без cloud AI» и приватностью, поэтому
//! трактуется строго: 127.0.0.0/8, ::1 и `localhost`.
//!
//! # Чего режим не обещает
//!
//! Он не отключает сеть целиком. Инструменты, которые пользователь сам включил —
//! удалённый MCP-сервер, открытие ссылки в браузере — продолжают работать: это
//! его осознанные действия, а не отправка разговора в чужую модель. Обещание
//! режима ровно одно и оно выполняется буквально: **ни одна реплика не уходит
//! в облачный AI**.

use serde::Serialize;

use crate::state::AppState;
use crate::storage::Storage;

/// Ключ настройки.
pub const SETTING_KEY: &str = "privacy.local_only";

/// Состояние приватности для интерфейса.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyStatus {
    pub local_only: bool,
    /// Провайдер по умолчанию и локален ли он.
    pub provider_label: Option<String>,
    pub provider_local: bool,
    /// Адрес сервиса распознавания и локален ли он.
    pub stt_url: Option<String>,
    pub stt_local: bool,
    /// Что мешает включить режим прямо сейчас.
    pub blockers: Vec<String>,
}

/// Локален ли адрес.
///
/// Разбор ручной, без парсера URL: нужен только хост, а тянуть зависимость ради
/// одной проверки — плохая сделка, особенно когда от этой проверки зависит
/// обещание приватности и её хочется читать целиком в одном месте.
pub fn is_local_url(url: &str) -> bool {
    let rest = url
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://");

    // Отрезаем путь, параметры и учётные данные.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);

    // IPv6 в квадратных скобках: [::1]:8080.
    let host = if let Some(inner) = authority.strip_prefix('[') {
        inner.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };

    let host = host.trim().to_lowercase();

    if host == "localhost" || host == "::1" || host == "0:0:0:0:0:0:0:1" {
        return true;
    }

    // Вся сеть 127.0.0.0/8, а не только 127.0.0.1.
    host.strip_prefix("127.")
        .map(|tail| {
            let parts: Vec<&str> = tail.split('.').collect();
            parts.len() == 3 && parts.iter().all(|p| p.parse::<u8>().is_ok())
        })
        .unwrap_or(false)
}

/// Включён ли режим.
pub fn is_local_only(storage: &Storage) -> bool {
    storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [SETTING_KEY], |r| {
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
        .as_deref()
        == Some("on")
}

/// Проверяет, можно ли обращаться к этому адресу в текущем режиме.
///
/// Вызывается перед каждым запросом к модели и к распознаванию — не при
/// включении режима. Настройки могли поменяться после, и проверка «на входе»
/// оставила бы дыру ровно там, где обещание должно держаться.
pub fn ensure_allowed(storage: &Storage, base_url: &str, what: &str) -> Result<(), String> {
    if !is_local_only(storage) || is_local_url(base_url) {
        return Ok(());
    }

    Err(format!(
        "включён режим Local Only: {what} по адресу {base_url} не локален. \
         Выберите локального провайдера или выключите режим в настройках."
    ))
}

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

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Текущее состояние приватности и то, что мешает включить режим.
#[tauri::command]
pub fn privacy_status(state: tauri::State<'_, AppState>) -> Result<PrivacyStatus, String> {
    let provider: Option<(String, String)> = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT label, base_url FROM providers WHERE is_default = 1 AND enabled = 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(|e| e.to_string())?;

    let stt_provider = setting(&state, "voice.stt.provider");
    let stt_url: Option<String> = state
        .storage
        .with_conn(|conn| {
            let result = match &stt_provider {
                Some(id) => conn.query_row(
                    "SELECT base_url FROM providers WHERE id = ?1",
                    [id],
                    |r| r.get::<_, String>(0),
                ),
                None => conn.query_row(
                    "SELECT base_url FROM providers
                     WHERE enabled = 1 AND kind IN ('openai', 'openrouter', 'ollama', 'lmstudio', 'custom')
                     ORDER BY is_default DESC LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0),
                ),
            };
            result.map(Some).or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(|e| e.to_string())?;

    let provider_local = provider.as_ref().is_some_and(|(_, url)| is_local_url(url));
    let stt_local = stt_url.as_deref().is_some_and(is_local_url);

    let mut blockers = Vec::new();
    match &provider {
        Some((label, url)) if !is_local_url(url) => {
            blockers.push(format!("провайдер «{label}» работает через {url}"))
        }
        None => blockers.push("провайдер по умолчанию не выбран".into()),
        _ => {}
    }
    if let Some(url) = &stt_url {
        if !is_local_url(url) {
            blockers.push(format!("распознавание речи идёт через {url}"));
        }
    }

    Ok(PrivacyStatus {
        local_only: is_local_only(&state.storage),
        provider_label: provider.as_ref().map(|(label, _)| label.clone()),
        provider_local,
        stt_url,
        stt_local,
        blockers,
    })
}

/// Включает или выключает режим.
///
/// Включение не проверяет, всё ли уже локально: пользователь вправе включить
/// режим заранее и настраивать провайдера после. Несоответствие он увидит
/// в списке помех, а запрос всё равно будет остановлен на месте.
#[tauri::command]
pub fn privacy_set_local_only(
    state: tauri::State<'_, AppState>,
    enabled: bool,
) -> Result<PrivacyStatus, String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![SETTING_KEY, if enabled { "on" } else { "off" }],
            )
        })
        .map_err(|e| e.to_string())?;

    privacy_status(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_loopback_in_all_its_spellings() {
        for url in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:1234/v1",
            "http://127.5.0.9:8080",
            "https://LOCALHOST/v1",
            "http://[::1]:8080/v1",
        ] {
            assert!(is_local_url(url), "{url} должен считаться локальным");
        }
    }

    #[test]
    fn treats_the_rest_of_the_network_as_remote() {
        for url in [
            "https://api.openai.com/v1",
            "https://api.anthropic.com",
            // Машина в той же сети — это уже другая машина.
            "http://192.168.1.50:11434/v1",
            "http://10.0.0.5:1234",
        ] {
            assert!(!is_local_url(url), "{url} не должен считаться локальным");
        }
    }

    #[test]
    fn is_not_fooled_by_a_hostname_that_merely_contains_localhost() {
        // Классическая подмена: хост чужой, а слово знакомое.
        assert!(!is_local_url("https://localhost.evil.com/v1"));
        assert!(!is_local_url("https://notlocalhost/v1"));
    }

    #[test]
    fn ignores_credentials_and_path_when_looking_at_the_host() {
        assert!(is_local_url("http://user:pass@127.0.0.1:8080/v1/chat"));
        assert!(!is_local_url("http://127.0.0.1@evil.com/v1"));
    }

    #[test]
    fn allows_everything_when_the_mode_is_off() {
        let storage = Storage::in_memory().expect("база должна открыться");
        assert!(ensure_allowed(&storage, "https://api.openai.com/v1", "провайдер").is_ok());
    }

    #[test]
    fn blocks_remote_endpoints_when_the_mode_is_on() {
        let storage = Storage::in_memory().expect("база должна открыться");
        storage
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, 'on')",
                    [SETTING_KEY],
                )
            })
            .expect("настройка должна записаться");

        assert!(is_local_only(&storage));
        assert!(ensure_allowed(&storage, "https://api.openai.com/v1", "провайдер").is_err());
        assert!(ensure_allowed(&storage, "http://localhost:11434/v1", "провайдер").is_ok());
    }
}
