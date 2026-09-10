//! Глобальные горячие клавиши (ТЗ §16 triggers, §38).
//!
//! Два вида сочетаний: одно вызывает саму Yuki, остальные запускают команды
//! пользователя. Регистрируются они вместе и всегда целиком: снять одно
//! сочетание, не трогая другие, платформенный слой не умеет, а частичная
//! перерегистрация оставляет висеть старые.

use std::collections::HashMap;

use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

/// Ключ настройки с сочетанием вызова.
const SETTING_KEY: &str = "hotkey.summon";

/// Событие запуска команды по сочетанию.
const EVENT_COMMAND: &str = "yuki://command-trigger";

/// Сочетание по умолчанию.
///
/// На macOS модификатор Command, на Windows — Control: одинаковое сочетание на
/// обеих системах ощущалось бы чужим ровно на одной из них.
#[cfg(target_os = "macos")]
const DEFAULT_SUMMON: &str = "Cmd+Shift+Y";
#[cfg(not(target_os = "macos"))]
const DEFAULT_SUMMON: &str = "Ctrl+Shift+Y";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Показывает и фокусирует главное окно.
///
/// Если окно уже видно и активно — прячет его: тот же хоткей должен убирать
/// ассистента с глаз, иначе для этого нужна мышь.
fn toggle_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);

    if visible && focused {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn parse(shortcut: &str) -> Result<Shortcut, String> {
    shortcut
        .parse::<Shortcut>()
        .map_err(|e| format!("не удалось разобрать сочетание «{shortcut}»: {e}"))
}

/// Регистрирует вызов Yuki и все командные сочетания.
///
/// Ошибка одного сочетания не отменяет остальные: занятый другим приложением
/// хоткей — обычное дело, и терять из-за него весь набор нельзя.
fn apply_all(app: &AppHandle, summon: &str, commands: &[(String, String)]) -> Result<(), String> {
    let manager = app.global_shortcut();
    let _ = manager.unregister_all();

    // Сопоставление сочетания с командой держим рядом с обработчиком: сам
    // обработчик получает только объект сочетания, без наших идентификаторов.
    let mut by_shortcut: HashMap<String, String> = HashMap::new();
    for (id, shortcut) in commands {
        match parse(shortcut) {
            Ok(parsed) => {
                by_shortcut.insert(parsed.to_string(), id.clone());
            }
            Err(error) => tracing::warn!(command = %id, %error, "сочетание команды не разобрано"),
        }
    }

    let summon_shortcut = parse(summon)?;
    let summon_key = summon_shortcut.to_string();
    let summon_label = summon_key.clone();

    let handle = app.clone();
    let registered = by_shortcut.len();
    let mut all: Vec<Shortcut> = vec![summon_shortcut];
    for shortcut in by_shortcut.keys() {
        if let Ok(parsed) = parse(shortcut) {
            all.push(parsed);
        }
    }

    manager
        .on_shortcuts(all, move |_app, shortcut, event| {
            // Реагируем только на нажатие: без этого одно нажатие даёт два
            // срабатывания и окно тут же прячется обратно.
            if event.state() != ShortcutState::Pressed {
                return;
            }

            let key = shortcut.to_string();
            if key == summon_key {
                toggle_window(&handle);
                return;
            }

            if let Some(id) = by_shortcut.get(&key) {
                let _ = handle.emit(EVENT_COMMAND, serde_json::json!({ "commandId": id }));
            }
        })
        .map_err(err)?;

    // Сочетание, которое не сработало, неотличимо от незарегистрированного,
    // пока не сказано, что именно было зарегистрировано.
    tracing::info!(summon = %summon_label, commands = registered, "сочетания зарегистрированы");
    Ok(())
}

/// Читает сохранённое сочетание вызова.
fn saved_summon(state: &AppState) -> Option<String> {
    state
        .storage
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
        .filter(|s| !s.trim().is_empty())
}

/// Перерегистрирует все сочетания по текущему состоянию базы.
///
/// Вызывается после любого изменения команд: сочетание могло появиться,
/// поменяться или исчезнуть вместе с командой.
pub fn resync(app: &AppHandle) {
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };

    let summon = saved_summon(&state).unwrap_or_else(|| DEFAULT_SUMMON.to_string());
    let commands = crate::automation::hotkeys(&state);

    if let Err(error) = apply_all(app, &summon, &commands) {
        tracing::warn!(%error, "сочетания не зарегистрированы");
    }
}

/// Ставит сочетания при старте.
///
/// Ошибка регистрации не фатальна: сочетание может быть занято другим
/// приложением, и это не повод не запускать ассистента.
pub fn init(app: &AppHandle) {
    resync(app);
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn hotkey_get(state: State<'_, AppState>) -> Result<String, String> {
    Ok(saved_summon(&state).unwrap_or_else(|| DEFAULT_SUMMON.to_string()))
}

/// Меняет сочетание вызова и сохраняет его.
///
/// Сначала проверка разбора, потом запись, потом регистрация: иначе в настройках
/// осталось бы сочетание, которое не работает.
#[tauri::command]
pub fn hotkey_set(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<(), String> {
    let shortcut = shortcut.trim().to_string();
    parse(&shortcut)?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![SETTING_KEY, shortcut],
            )
        })
        .map_err(err)?;

    resync(&app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shortcut_parses_on_this_platform() {
        // Сочетание по умолчанию обязано быть разбираемым: иначе ассистента
        // нельзя вызвать вообще, и узнаётся это только в рантайме.
        assert!(parse(DEFAULT_SUMMON).is_ok(), "{DEFAULT_SUMMON} не разбирается");
    }

    #[test]
    fn rejects_nonsense_shortcuts() {
        assert!(parse("не сочетание").is_err());
    }

    #[test]
    fn parsed_shortcuts_render_back_to_a_stable_key() {
        // На сопоставлении по строковому виду держится доставка события нужной
        // команде: если бы вид зависел от написания, «Ctrl+Shift+K» и
        // «ctrl+shift+k» разъехались бы в разные ключи.
        let a = parse("Ctrl+Shift+K").expect("разбирается");
        let b = parse("ctrl+shift+K").expect("разбирается");
        assert_eq!(a.to_string(), b.to_string());
    }
}
