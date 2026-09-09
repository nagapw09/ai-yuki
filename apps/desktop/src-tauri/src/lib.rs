//! Точка сборки десктопного приложения Yuki.
//!
//! Здесь соединяются три вещи и больше ничего: платформенные адаптеры (ТЗ §30),
//! локальное хранилище (ТЗ §31) и список команд, доступных Agent Core на TypeScript.

pub mod ai;
pub mod automation;
pub mod avatar;
pub mod calendar;
pub mod capabilities;
pub mod catalog;
pub mod commands;
pub mod hotkeys;
pub mod memory;
pub mod permissions;
pub mod plugins;
pub mod privacy;
pub mod reminders;
pub mod secrets;
pub mod state;
pub mod storage;
pub mod system;
pub mod voice;

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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let storage = Storage::open(data_dir.join(DB_FILE))?;
            // Разрешение могли выдать или отозвать в системных настройках между
            // запусками, поэтому статус ОС пересчитывается на каждом старте.
            permissions::sync_os(&storage)?;
            let adapters = system::build()?;
            let http = yuki_ai::http_client()?;

            // Протухшая краткосрочная память не должна пережить перезапуск.
            let _ = memory::prune_expired(&storage);

            app.manage(AppState::new(adapters, storage, http));

            hotkeys::init(app.handle());
            reminders::spawn_scheduler(app.handle().clone());
            capabilities::connect_enabled(app.handle().clone());
            // Окно аватара возвращается туда же, где его оставили (ТЗ §12).
            avatar::restore(app.handle().clone());
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
            commands::open_url,
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
            commands::accessibility_text,
            commands::display_count,
            // секреты (ТЗ §29)
            commands::secret_set,
            commands::secret_delete,
            commands::secret_exists,
            // разрешения (ТЗ §21)
            commands::permissions_list,
            commands::permission_set,
            commands::permission_hint,
            // настройки и журнал (ТЗ §23, §31)
            commands::setting_get,
            commands::setting_set,
            commands::activity_log,
            commands::activity_record,
            // AI-провайдеры (ТЗ §4)
            ai::provider_list,
            ai::provider_save,
            ai::provider_set_key,
            ai::provider_clear_key,
            ai::provider_set_default,
            ai::provider_test,
            ai::chat_send,
            // приватность (ТЗ §29)
            privacy::privacy_status,
            privacy::privacy_set_local_only,
            // память (ТЗ §9)
            memory::memory_list,
            memory::memory_save,
            memory::memory_search,
            memory::memory_delete,
            memory::memory_clear,
            memory::memory_context,
            // напоминания и уведомления (ТЗ §25)
            reminders::reminder_create,
            reminders::reminder_list,
            reminders::reminder_complete,
            reminders::reminder_delete,
            reminders::notify,
            // хоткеи (ТЗ §16, §38)
            hotkeys::hotkey_get,
            hotkeys::hotkey_set,
            // голос (ТЗ §10)
            voice::voice_status,
            voice::voice_start,
            voice::voice_stop,
            voice::voice_finish_utterance,
            voice::voice_speak,
            voice::voice_stop_speaking,
            voice::voice_set_voice,
            voice::voice_configure_stt,
            // возможности и MCP (ТЗ §17, §18, §19)
            capabilities::integrations_list,
            capabilities::integration_install,
            capabilities::capability_list,
            capabilities::capability_set_enabled,
            capabilities::capability_remove,
            capabilities::mcp_add,
            capabilities::mcp_test,
            capabilities::mcp_tools,
            capabilities::mcp_call,
            // аватар (ТЗ §12)
            avatar::avatar_status,
            avatar::avatar_open,
            avatar::avatar_close,
            avatar::avatar_set_click_through,
            avatar::avatar_set_always_on_top,
            avatar::avatar_set_model,
            avatar::avatar_model_bytes,
            avatar::avatar_remember_placement,
            // календари (ТЗ §25)
            calendar::calendar_accounts,
            calendar::calendar_set_client,
            calendar::calendar_connect,
            calendar::calendar_disconnect,
            calendar::calendar_events,
            calendar::calendar_create_event,
            calendar::calendar_delete_event,
            // плагины (ТЗ §18, §20)
            plugins::plugin_list,
            plugins::plugin_review,
            plugins::plugin_install,
            plugins::plugin_remove,
            plugins::plugin_scaffold,
            // автоматизации (ТЗ §16)
            automation::command_list,
            automation::command_save,
            automation::command_delete,
        ])
        .on_window_event(|window, event| {
            // Микрофон и синтезатор держат устройства ОС: закрыть их надо явно,
            // иначе процесс уходит, а индикатор записи у пользователя остаётся.
            if matches!(event, tauri::WindowEvent::Destroyed)
                && window.label() != avatar::WINDOW_LABEL
            {
                voice::shutdown(window.app_handle());
            }
        })
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно Yuki");
}
