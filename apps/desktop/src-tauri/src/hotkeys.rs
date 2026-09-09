//! Глобальные горячие клавиши (ТЗ §16 triggers, §38).
//!
//! Ассистент должен вызываться поверх любой работы: пользователь не станет искать
//! окно Yuki в панели задач ради одной фразы. Поэтому хоткей регистрируется на
//! уровне ОС и работает, даже когда фокус в другом приложении.

use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::state::AppState;

/// Ключ настройки с текущим сочетанием.
const SETTING_KEY: &str = "hotkey.summon";

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

/// Регистрирует сочетание, снимая предыдущее.
fn apply(app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let manager = app.global_shortcut();

    // Снимаем всё прежнее: иначе старое сочетание продолжит работать и
    // пользователь получит два хоткея вместо одного.
    let _ = manager.unregister_all();

    let parsed = parse(shortcut)?;
    let handle = app.clone();
    manager
        .on_shortcut(parsed, move |_app, _shortcut, event| {
            // Реагируем только на нажатие: без этого одно нажатие даёт два
            // срабатывания и окно тут же прячется обратно.
            if event.state() == ShortcutState::Pressed {
                toggle_window(&handle);
            }
        })
        .map_err(err)
}

/// Ставит хоткей при старте: сохранённый пользователем или значение по умолчанию.
///
/// Ошибка регистрации не фатальна: сочетание может быть занято другим
/// приложением, и это не повод не запускать ассистента.
pub fn init(app: &AppHandle, saved: Option<String>) {
    let shortcut = saved
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SUMMON.to_string());

    if let Err(error) = apply(app, &shortcut) {
        tracing::warn!(%error, "глобальный хоткей не зарегистрирован");
    }
}

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Текущее сочетание.
#[tauri::command]
pub fn hotkey_get(state: State<'_, AppState>) -> Result<String, String> {
    let saved: Option<String> = state
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
        .map_err(err)?;

    Ok(saved.unwrap_or_else(|| DEFAULT_SUMMON.to_string()))
}

/// Меняет сочетание и сохраняет его.
///
/// Сначала регистрация, потом запись: иначе в настройках осталось бы сочетание,
/// которое не работает.
#[tauri::command]
pub fn hotkey_set(
    app: AppHandle,
    state: State<'_, AppState>,
    shortcut: String,
) -> Result<(), String> {
    apply(&app, shortcut.trim())?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![SETTING_KEY, shortcut.trim()],
            )
        })
        .map(|_| ())
        .map_err(err)
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
}
