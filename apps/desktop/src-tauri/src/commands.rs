//! Tauri-команды: единственный мост между Agent Core на TypeScript и системным слоем.
//!
//! Каждая команда возвращает `Result<T, String>` — Tauri сериализует `Err` в reject
//! промиса, и на стороне TS это превращается в `ToolResult.error`. Инвариант ТЗ §5
//! («не рапортовать об успехе без подтверждения инструмента») держится тем, что
//! успех здесь — это всегда полезные данные от ОС, а не факт отсутствия исключения.

use serde::Serialize;
use tauri::State;
use yuki_system::{
    AccessibilityNode, AppInfo, FileEntry, FileQuery, Modifier, MouseButton, ScreenCapture,
    SystemInfo, WindowInfo,
};

use crate::state::AppState;

/// Приводит ошибку любого слоя к сообщению для TS.
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Приложения и окна (ТЗ §6, §30) ──────────────────────────────────────────────

#[tauri::command]
pub fn open_app(state: State<'_, AppState>, app: String) -> Result<AppInfo, String> {
    state.adapters.system.open_app(&app).map_err(err)
}

#[tauri::command]
pub fn close_app(state: State<'_, AppState>, app: String, force: bool) -> Result<(), String> {
    state.adapters.system.close_app(&app, force).map_err(err)
}

#[tauri::command]
pub fn list_apps(state: State<'_, AppState>) -> Result<Vec<AppInfo>, String> {
    state.adapters.system.list_apps().map_err(err)
}

#[tauri::command]
pub fn list_windows(state: State<'_, AppState>) -> Result<Vec<WindowInfo>, String> {
    state.adapters.system.list_windows().map_err(err)
}

#[tauri::command]
pub fn focus_window(state: State<'_, AppState>, window_id: u64) -> Result<(), String> {
    state.adapters.system.focus_window(window_id).map_err(err)
}

#[tauri::command]
pub fn active_window(state: State<'_, AppState>) -> Result<Option<WindowInfo>, String> {
    state.adapters.system.active_window().map_err(err)
}

#[tauri::command]
pub fn system_info(state: State<'_, AppState>) -> Result<SystemInfo, String> {
    state.adapters.system.system_info().map_err(err)
}

#[tauri::command]
pub fn get_volume(state: State<'_, AppState>) -> Result<f32, String> {
    state.adapters.system.volume().map_err(err)
}

#[tauri::command]
pub fn set_volume(state: State<'_, AppState>, level: f32) -> Result<(), String> {
    state.adapters.system.set_volume(level).map_err(err)
}

// ── Файлы (ТЗ §8, §30) ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn file_search(state: State<'_, AppState>, query: FileQuery) -> Result<Vec<FileEntry>, String> {
    state.adapters.files.search(&query).map_err(err)
}

#[tauri::command]
pub fn file_read_text(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let bytes = state.adapters.files.read(&path).map_err(err)?;
    String::from_utf8(bytes).map_err(|_| format!("{path} не является текстом в UTF-8"))
}

#[tauri::command]
pub fn file_write_text(
    state: State<'_, AppState>,
    path: String,
    contents: String,
) -> Result<(), String> {
    state
        .adapters
        .files
        .write(&path, contents.as_bytes())
        .map_err(err)
}

#[tauri::command]
pub fn file_move(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    state.adapters.files.move_to(&from, &to).map_err(err)
}

#[tauri::command]
pub fn file_copy(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    state.adapters.files.copy(&from, &to).map_err(err)
}

/// Удаление файла. ТЗ §22 относит его к HIGH risk, поэтому вызов сюда обязан
/// приходить уже после подтверждения пользователя, а `to_trash` по умолчанию — `true`.
#[tauri::command]
pub fn file_delete(
    state: State<'_, AppState>,
    path: String,
    to_trash: Option<bool>,
) -> Result<(), String> {
    state
        .adapters
        .files
        .delete(&path, to_trash.unwrap_or(true))
        .map_err(err)
}

#[tauri::command]
pub fn file_stat(state: State<'_, AppState>, path: String) -> Result<FileEntry, String> {
    state.adapters.files.stat(&path).map_err(err)
}

#[tauri::command]
pub fn file_open(state: State<'_, AppState>, path: String) -> Result<(), String> {
    state.adapters.files.open(&path).map_err(err)
}

/// Открывает ссылку в браузере по умолчанию (ТЗ §7).
///
/// Схема проверяется явно: `opener` умеет открывать не только веб-адреса, и без
/// проверки инструмент «открой ссылку» стал бы способом запустить что угодно
/// через `file://` или зарегистрированный в системе протокол.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    open_external(&url)
}

/// То же самое для внутренних вызовов — например для страницы согласия
/// календаря (ТЗ §25). Проверка схемы одна на всех: второй путь открытия
/// ссылки быстро стал бы путём в обход проверки.
pub(crate) fn open_external(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    let allowed = trimmed.starts_with("https://") || trimmed.starts_with("http://");

    if !allowed {
        return Err(format!(
            "открывать можно только http и https, получено «{trimmed}»"
        ));
    }

    opener::open(trimmed).map_err(err)
}

// ── Ввод (ТЗ §6, §30) ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn type_text(state: State<'_, AppState>, text: String) -> Result<(), String> {
    state.adapters.input.type_text(&text).map_err(err)
}

#[tauri::command]
pub fn press_key(
    state: State<'_, AppState>,
    key: String,
    modifiers: Vec<Modifier>,
) -> Result<(), String> {
    state.adapters.input.press_key(&key, &modifiers).map_err(err)
}

#[tauri::command]
pub fn mouse_move(state: State<'_, AppState>, x: i32, y: i32) -> Result<(), String> {
    state.adapters.input.mouse_move(x, y).map_err(err)
}

#[tauri::command]
pub fn mouse_click(state: State<'_, AppState>, button: MouseButton) -> Result<(), String> {
    state.adapters.input.mouse_click(button).map_err(err)
}

#[tauri::command]
pub fn mouse_scroll(state: State<'_, AppState>, dx: i32, dy: i32) -> Result<(), String> {
    state.adapters.input.mouse_scroll(dx, dy).map_err(err)
}

#[tauri::command]
pub fn cursor_position(state: State<'_, AppState>) -> Result<(i32, i32), String> {
    state.adapters.input.cursor_position().map_err(err)
}

// ── Буфер обмена (ТЗ §16, §24) ──────────────────────────────────────────────────

#[tauri::command]
pub fn clipboard_read(state: State<'_, AppState>) -> Result<Option<String>, String> {
    state.adapters.clipboard.read_text().map_err(err)
}

#[tauri::command]
pub fn clipboard_write(state: State<'_, AppState>, text: String) -> Result<(), String> {
    state.adapters.clipboard.write_text(&text).map_err(err)
}

// ── Экран (ТЗ §6, §30) ──────────────────────────────────────────────────────────

/// Сколько пикселей в ширину уходит модели по умолчанию.
///
/// Снимок 2560×1440 в PNG весит около мегабайта, и это мегабайт на каждом
/// шаге агента. 1280 пикселей — это всё ещё читаемый интерфейс и вчетверо
/// меньше данных. Когда нужен мелкий текст, есть область — она даёт
/// полное разрешение без лишнего экрана вокруг.
const DEFAULT_MAX_WIDTH: u32 = 1280;

#[tauri::command]
pub fn screen_capture(
    state: State<'_, AppState>,
    display_index: Option<usize>,
    region: Option<yuki_system::Rect>,
    max_width: Option<u32>,
) -> Result<ScreenCapture, String> {
    state
        .adapters
        .screen
        .capture_with(&yuki_system::CaptureOptions {
            display_index,
            region,
            // Ноль от вызывающего — это явное «без ограничения»: бывает нужно
            // прочитать мелкий текст целиком.
            max_width: Some(max_width.unwrap_or(DEFAULT_MAX_WIDTH)).filter(|w| *w > 0),
        })
        .map_err(err)
}

/// Распознаёт текст на экране (ТЗ §6).
///
/// Область не уменьшается по умолчанию, в отличие от снимка: уменьшенный мелкий
/// текст перестаёт распознаваться, а ради мелкого текста область и берут.
#[tauri::command]
pub fn screen_read_text(
    state: State<'_, AppState>,
    display_index: Option<usize>,
    region: Option<yuki_system::Rect>,
) -> Result<Vec<yuki_system::TextLine>, String> {
    state
        .adapters
        .screen
        .recognize_text(&yuki_system::CaptureOptions {
            display_index,
            region,
            max_width: None,
        })
        .map_err(err)
}

#[tauri::command]
pub fn screen_capture_window(
    state: State<'_, AppState>,
    window_id: u64,
) -> Result<ScreenCapture, String> {
    state.adapters.screen.capture_window(window_id).map_err(err)
}

#[tauri::command]
pub fn accessibility_tree(
    state: State<'_, AppState>,
    window_id: Option<u64>,
) -> Result<AccessibilityNode, String> {
    state
        .adapters
        .screen
        .accessibility_tree(window_id)
        .map_err(err)
}

/// Дерево интерфейса в компактном текстовом виде (ТЗ §6).
///
/// Отдельно от [`accessibility_tree`], который возвращает структуру: модели нужен
/// текст, и собирать его лучше здесь, чем гонять через мост дерево объектов,
/// чтобы тут же склеить его в строку.
#[tauri::command]
pub fn accessibility_text(
    state: State<'_, AppState>,
    window_id: Option<u64>,
) -> Result<String, String> {
    let tree = state
        .adapters
        .screen
        .accessibility_tree(window_id)
        .map_err(err)?;
    Ok(yuki_accessibility::render(&tree))
}

#[tauri::command]
pub fn display_count(state: State<'_, AppState>) -> Result<usize, String> {
    state.adapters.screen.display_count().map_err(err)
}

// ── Секреты (ТЗ §29, §31) ───────────────────────────────────────────────────────

/// Записывает секрет в системное хранилище.
///
/// Обратной команды «прочитать секрет» намеренно нет: значение ключа не должно
/// попадать во фронтенд вообще. Запросы к провайдерам собираются в Rust-слое,
/// а UI работает только с фактом наличия ключа.
#[tauri::command]
pub fn secret_set(secret_ref: String, value: String) -> Result<(), String> {
    crate::secrets::set(&secret_ref, &value).map_err(err)
}

#[tauri::command]
pub fn secret_delete(secret_ref: String) -> Result<(), String> {
    crate::secrets::delete(&secret_ref).map_err(err)
}

#[tauri::command]
pub fn secret_exists(secret_ref: String) -> bool {
    crate::secrets::exists(&secret_ref)
}

// ── Разрешения (ТЗ §21) ─────────────────────────────────────────────────────────

/// Статус одной категории разрешений.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub category: String,
    /// Решение пользователя внутри Yuki.
    pub granted: bool,
    /// Разрешение на уровне ОС. На Windows для большинства категорий совпадает
    /// с `granted`; на macOS выдаётся системой отдельно и требует перезапуска.
    pub os_granted: bool,
}

#[tauri::command]
pub fn permissions_list(state: State<'_, AppState>) -> Result<Vec<PermissionStatus>, String> {
    state
        .storage
        .with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT category, granted, os_granted FROM permissions ORDER BY category")?;
            let rows = stmt.query_map([], |row| {
                Ok(PermissionStatus {
                    category: row.get(0)?,
                    granted: row.get::<_, i64>(1)? != 0,
                    os_granted: row.get::<_, i64>(2)? != 0,
                })
            })?;
            rows.collect()
        })
        .map_err(err)
}

#[tauri::command]
pub fn permission_set(
    state: State<'_, AppState>,
    category: String,
    granted: bool,
) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE permissions SET granted = ?2, updated_at = unixepoch() WHERE category = ?1",
                rusqlite::params![category, granted as i64],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Где пользователю выдать разрешение вручную (ТЗ §21).
///
/// `None` означает, что категория не требует отдельной выдачи на этой ОС —
/// UI в этом случае не должен звать человека в системные настройки впустую.
#[tauri::command]
pub fn permission_hint(category: String) -> Option<String> {
    crate::permissions::how_to_grant(&category).map(str::to_string)
}

// ── Настройки (ТЗ §31) ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn setting_get(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [&key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(err)
}

#[tauri::command]
pub fn setting_set(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
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

// ── Журнал активности (ТЗ §23) ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: String,
    pub ts: i64,
    pub tool: String,
    pub target: Option<String>,
    pub status: String,
    pub result: Option<String>,
    pub duration_ms: Option<i64>,
}

#[tauri::command]
pub fn activity_log(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<ActivityEntry>, String> {
    let limit = limit.unwrap_or(100).min(1000);
    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, ts, tool, target, status, result, duration_ms
                 FROM activity_logs ORDER BY ts DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit], |row| {
                Ok(ActivityEntry {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    tool: row.get(2)?,
                    target: row.get(3)?,
                    status: row.get(4)?,
                    result: row.get(5)?,
                    duration_ms: row.get(6)?,
                })
            })?;
            rows.collect()
        })
        .map_err(err)
}

#[tauri::command]
pub fn activity_record(
    state: State<'_, AppState>,
    tool: String,
    target: Option<String>,
    status: String,
    result: Option<String>,
    duration_ms: Option<i64>,
) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO activity_logs (id, tool, target, status, result, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    uuid_v4(),
                    tool,
                    target,
                    status,
                    result,
                    duration_ms
                ],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Идентификатор записи. Полноценный UUID здесь избыточен: нужна лишь
/// монотонность и отсутствие коллизий внутри одной локальной базы.
fn uuid_v4() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:032x}-{seq:x}")
}
