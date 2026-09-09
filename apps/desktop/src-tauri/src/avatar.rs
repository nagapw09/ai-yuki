//! Аватар: отдельное прозрачное окно поверх всех (ТЗ §12).
//!
//! # Почему отдельное окно
//!
//! ТЗ требует always-on-top, прозрачность и click-through. Всё это — свойства
//! окна, а не элемента внутри страницы: сделать «поверх всех» частью главного
//! окна нельзя, а прозрачность главного окна сломала бы весь остальной
//! интерфейс. Поэтому аватар живёт своим окном без рамки, а состояние получает
//! событиями из главного.
//!
//! # Модель приносит пользователь
//!
//! Готового VRM в поставке нет и не будет: у моделей свои лицензии, и класть
//! чужую в дистрибутив нельзя. Yuki принимает путь к `.vrm`, читает файл сама и
//! отдаёт байты в окно — так модель не проходит через файловый протокол
//! WebView и не требует ослаблять CSP.

use serde::{Deserialize, Serialize};
use tauri::{Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::state::AppState;

/// Метка окна аватара. По ней его находят и команды, и события.
pub const WINDOW_LABEL: &str = "avatar";

/// Ключи настроек.
const KEY_ENABLED: &str = "avatar.enabled";
const KEY_MODEL: &str = "avatar.model";
const KEY_CLICK_THROUGH: &str = "avatar.click_through";
const KEY_ON_TOP: &str = "avatar.always_on_top";
const KEY_PLACEMENT: &str = "avatar.placement";

/// Расширение единственного поддерживаемого формата.
const MODEL_EXTENSION: &str = "vrm";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Placement {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarStatus {
    pub enabled: bool,
    /// Путь к модели; пустая строка — модель не выбрана.
    pub model: String,
    /// Существует ли файл модели прямо сейчас.
    pub model_present: bool,
    pub click_through: bool,
    pub always_on_top: bool,
    /// Открыто ли окно в данный момент.
    pub open: bool,
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

fn flag(state: &AppState, key: &str, default: bool) -> bool {
    match setting(state, key).as_deref() {
        Some("on") => true,
        Some("off") => false,
        _ => default,
    }
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn avatar_status(app: tauri::AppHandle, state: State<'_, AppState>) -> AvatarStatus {
    let model = setting(&state, KEY_MODEL).unwrap_or_default();

    AvatarStatus {
        enabled: flag(&state, KEY_ENABLED, false),
        // Файл могли переместить между запусками; «модель выбрана» и «модель
        // есть» — разные утверждения, и путать их значит показать пустое окно
        // без объяснения.
        model_present: !model.is_empty() && std::path::Path::new(&model).is_file(),
        model,
        click_through: flag(&state, KEY_CLICK_THROUGH, false),
        always_on_top: flag(&state, KEY_ON_TOP, true),
        open: app.get_webview_window(WINDOW_LABEL).is_some(),
    }
}

/// Открывает окно аватара, восстанавливая прежние размер и место.
#[tauri::command]
pub async fn avatar_open(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AvatarStatus, String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.show().map_err(err)?;
        window.set_focus().map_err(err)?;
        return Ok(avatar_status(app, state));
    }

    let placement: Placement = setting(&state, KEY_PLACEMENT)
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or(Placement {
            x: 80,
            y: 80,
            width: 320,
            height: 480,
        });

    let mut builder = WebviewWindowBuilder::new(
        &app,
        WINDOW_LABEL,
        WebviewUrl::App("index.html".into()),
    )
    .title("Yuki")
    .inner_size(placement.width as f64, placement.height as f64)
    .position(placement.x as f64, placement.y as f64)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(true)
    // В панели задач ему делать нечего: это компаньон на экране, а не второе
    // приложение.
    .skip_taskbar(true);

    if flag(&state, KEY_ON_TOP, true) {
        builder = builder.always_on_top(true);
    }

    let window = builder.build().map_err(err)?;

    if flag(&state, KEY_CLICK_THROUGH, false) {
        window.set_ignore_cursor_events(true).map_err(err)?;
    }

    set_setting(&state, KEY_ENABLED, "on")?;
    Ok(avatar_status(app, state))
}

#[tauri::command]
pub fn avatar_close(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        // Место запоминаем до закрытия: после него окна уже нет, и спросить
        // его координаты будет не у кого.
        let _ = remember_placement(&window, &state);
        window.close().map_err(err)?;
    }
    set_setting(&state, KEY_ENABLED, "off")
}

/// Пропускать ли клики сквозь окно (ТЗ §12: click-through).
#[tauri::command]
pub fn avatar_set_click_through(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.set_ignore_cursor_events(enabled).map_err(err)?;
    }
    set_setting(&state, KEY_CLICK_THROUGH, if enabled { "on" } else { "off" })
}

#[tauri::command]
pub fn avatar_set_always_on_top(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(WINDOW_LABEL) {
        window.set_always_on_top(enabled).map_err(err)?;
    }
    set_setting(&state, KEY_ON_TOP, if enabled { "on" } else { "off" })
}

/// Запоминает путь к модели.
#[tauri::command]
pub fn avatar_set_model(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<AvatarStatus, String> {
    let trimmed = path.trim();

    if !trimmed.is_empty() {
        let file = std::path::Path::new(trimmed);
        if !file.is_file() {
            return Err(format!("файла нет: {trimmed}"));
        }
        // Проверяем расширение здесь, а не при загрузке: сообщение «выберите
        // .vrm» полезнее, чем ошибка разбора glTF в консоли окна аватара.
        let extension = file
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension != MODEL_EXTENSION {
            return Err("нужна модель в формате .vrm".into());
        }
    }

    set_setting(&state, KEY_MODEL, trimmed)?;
    Ok(avatar_status(app, state))
}

/// Отдаёт файл модели окну аватара.
///
/// Байтами через IPC, а не ссылкой на файл: так не нужен файловый протокол в
/// WebView и не нужно ослаблять CSP ради одной картинки. Модель читается один
/// раз при открытии окна.
#[tauri::command]
pub fn avatar_model_bytes(state: State<'_, AppState>) -> Result<tauri::ipc::Response, String> {
    let path = setting(&state, KEY_MODEL).unwrap_or_default();
    if path.trim().is_empty() {
        return Err("модель не выбрана".into());
    }

    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Сохраняет текущее положение окна.
#[tauri::command]
pub fn avatar_remember_placement(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let window = app
        .get_webview_window(WINDOW_LABEL)
        .ok_or("окно аватара закрыто")?;
    remember_placement(&window, &state)
}

fn remember_placement(window: &tauri::WebviewWindow, state: &AppState) -> Result<(), String> {
    let position = window.outer_position().map_err(err)?;
    let size = window.inner_size().map_err(err)?;

    let placement = Placement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };

    set_setting(
        state,
        KEY_PLACEMENT,
        &serde_json::to_string(&placement).map_err(err)?,
    )
}

/// Открывает окно на старте, если в прошлый раз оно было открыто.
pub fn restore(app: tauri::AppHandle) {
    let state = app.state::<AppState>();
    if !flag(&state, KEY_ENABLED, false) {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = handle.state::<AppState>();
        if let Err(error) = avatar_open(handle.clone(), state).await {
            tracing::warn!(%error, "не удалось восстановить окно аватара");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_survives_a_round_trip_through_settings() {
        let placement = Placement {
            x: -1200,
            y: 40,
            width: 320,
            height: 480,
        };
        let text = serde_json::to_string(&placement).expect("должно сериализоваться");
        let back: Placement = serde_json::from_str(&text).expect("и разобраться обратно");

        // Отрицательная координата — это второй монитор слева, а не ошибка.
        assert_eq!(back.x, -1200);
        assert_eq!(back.width, 320);
    }

    #[test]
    fn an_unreadable_placement_falls_back_instead_of_failing() {
        assert!(serde_json::from_str::<Placement>("не json").is_err());
        // Вызов в avatar_open использует ok(), поэтому испорченная настройка
        // означает «открыть на месте по умолчанию», а не отказ открыть окно.
    }
}
