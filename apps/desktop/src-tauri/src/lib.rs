//! Точка сборки десктопного приложения Yuki.
//!
//! Здесь соединяются три вещи и больше ничего: платформенные адаптеры (ТЗ §30),
//! локальное хранилище (ТЗ §31) и список команд, доступных Agent Core на TypeScript.

pub mod commands;
pub mod secrets;
pub mod state;
pub mod storage;
pub mod system;

use tauri::Manager;

use crate::state::AppState;
use crate::storage::Storage;

/// Имя файла базы в каталоге данных приложения.
const DB_FILE: &str = "yuki.db";

/// Запускает приложение.
///
/// Падение на этапе инициализации адаптеров или базы — фатально и осознанно:
/// Yuki без системного слоя и хранилища не ассистент, а пустое окно, и молча
/// продолжать в таком состоянии хуже, чем честно сообщить об ошибке.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let storage = Storage::open(data_dir.join(DB_FILE))?;
            let adapters = system::build()?;

            app.manage(AppState::new(adapters, storage));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // приложения и окна (ТЗ §6, §30)
            commands::open_app,
            commands::close_app,
            commands::list_apps,
            commands::list_windows,
            commands::focus_window,
            commands::active_window,
            commands::system_info,
            commands::get_volume,
            commands::set_volume,
            // файлы (ТЗ §8)
            commands::file_search,
            commands::file_read_text,
            commands::file_write_text,
            commands::file_move,
            commands::file_copy,
            commands::file_delete,
            commands::file_stat,
            commands::file_open,
            // ввод (ТЗ §6)
            commands::type_text,
            commands::press_key,
            commands::mouse_move,
            commands::mouse_click,
            commands::mouse_scroll,
            commands::cursor_position,
            // буфер обмена (ТЗ §16, §24)
            commands::clipboard_read,
            commands::clipboard_write,
            // экран (ТЗ §6)
            commands::screen_capture,
            commands::screen_capture_window,
            commands::accessibility_tree,
            commands::display_count,
            // секреты (ТЗ §29)
            commands::secret_set,
            commands::secret_delete,
            commands::secret_exists,
            // разрешения (ТЗ §21)
            commands::permissions_list,
            commands::permission_set,
            // настройки и журнал (ТЗ §23, §31)
            commands::setting_get,
            commands::setting_set,
            commands::activity_log,
            commands::activity_record,
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно Yuki");
}
