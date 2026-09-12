//! Точка сборки десктопного приложения Yuki.
//!
//! Здесь соединяются три вещи и больше ничего: платформенные адаптеры (ТЗ §30),
//! локальное хранилище (ТЗ §31) и список команд, доступных Agent Core на TypeScript.

pub mod ai;
pub mod automation;
pub mod avatar;
pub mod profile;
pub mod telegram;
pub mod backup;
pub mod calendar;
pub mod capabilities;
pub mod diagnostics;
pub mod catalog;
pub mod commands;
pub mod everyday;
pub mod hotkeys;
pub mod memory;
pub mod notes;
pub mod onboarding;
pub mod permissions;
pub mod persona;
pub mod plugins;
pub mod privacy;
pub mod reminders;
pub mod requirements;
pub mod secrets;
pub mod state;
pub mod storage;
pub mod system;
pub mod tray;
pub mod updater;
pub mod voice;
pub mod window;

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
/// Включает журнал диагностики и возвращает буфер с его хвостом.
///
/// Без подписчика каждый `tracing::warn!` в коде — это строка, которую никто
/// никогда не прочтёт. Именно так незамеченным осталось неудавшееся
/// сочетание вызова: код честно предупреждал, но предупреждение некуда было
/// вывести.
///
/// Уровень берётся из `YUKI_LOG` (формат `RUST_LOG`), по умолчанию `info`.
fn init_logging() -> diagnostics::LogBuffer {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("YUKI_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    let buffer = diagnostics::LogBuffer::new();

    // Два приёмника: stderr для того, кто смотрит сейчас, и кольцевой
    // буфер для отчёта о поломке, который соберут после сбоя.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(buffer.clone()),
        )
        .try_init();

    buffer
}

pub fn run() {
    let logs = init_logging();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Автозапуск без аргументов: при старте вместе с системой окно
        // не распахивается — Yuki просто появляется в трее (docs/GAPS.md §3).
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let storage = Storage::open(data_dir.join(DB_FILE))?;
            // Разрешение могли выдать или отозвать в системных настройках между
            // запусками, поэтому статус ОС пересчитывается на каждом старте.
            permissions::sync_os(&storage)?;
            let adapters = system::build()?;
            let http = yuki_ai::http_client()?;

            // Протухшая краткосрочная память не должна пережить перезапуск.
            let _ = memory::prune_expired(&storage);

            app.manage(AppState::new(adapters, storage, http, logs.clone()));

            hotkeys::init(app.handle());
            // Трей строится после состояния: его меню читает настройки.
            if let Err(error) = tray::init(app.handle()) {
                // Система без области уведомлений — не повод не запуститься.
                tracing::warn!(%error, "не удалось создать иконку в трее");
            }
            tray::restore(app.handle());
            reminders::spawn_scheduler(app.handle().clone());
            capabilities::connect_enabled(app.handle().clone());
            // Окно аватара возвращается туда же, где его оставили (ТЗ §12).
            avatar::restore(app.handle().clone());
            // Канал управления с телефона поднимается сам, если был включён
            // (ТЗ §28). Local Only и отсутствие токена он проверяет внутри.
            telegram::restore(app.handle().clone());
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
            commands::screen_read_text,
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
            voice::voice_speaking_level,
            voice::voice_start,
            voice::voice_stop,
            voice::voice_finish_utterance,
            voice::voice_speak,
            voice::voice_stop_speaking,
            voice::voice_set_voice,
            voice::voice_configure_stt,
            // слово пробуждения (ТЗ §37)
            voice::wake_status,
            voice::wake_enroll_record,
            voice::wake_enroll_finish,
            voice::wake_forget,
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
            // роль и тон (docs/GAPS.md §7)
            persona::persona_get,
            persona::persona_set,
            // диагностика (docs/GAPS.md §14)
            diagnostics::diagnostics_report,
            diagnostics::diagnostics_save,
            // экспорт и импорт (docs/GAPS.md §11)
            backup::backup_export,
            backup::backup_preview,
            backup::backup_import,
            // заметки (docs/GAPS.md §5)
            notes::note_list,
            notes::note_save,
            notes::note_delete,
            // погода и курсы (docs/GAPS.md §6)
            everyday::weather_get,
            everyday::rates_get,
            // обновления (docs/GAPS.md §2)
            updater::update_status,
            updater::update_check,
            updater::update_install,
            // трей, фон и автозапуск (docs/GAPS.md §3)
            tray::background_status,
            tray::background_set_close_to_tray,
            tray::background_set_always_on_top,
            tray::background_set_autostart,
            tray::tray_set_state,
            // мастер первого запуска (docs/GAPS.md §1)
            onboarding::onboarding_status,
            onboarding::onboarding_completed,
            onboarding::onboarding_finish,
            onboarding::onboarding_reset,
            onboarding::permission_open_settings,
            onboarding::permission_request_os,
            // системные требования (docs/GAPS.md §4)
            requirements::system_requirements,
            // аватар (ТЗ §12)
            avatar::avatar_status,
            avatar::avatar_open,
            avatar::avatar_close,
            avatar::avatar_set_click_through,
            avatar::avatar_set_always_on_top,
            telegram::telegram_status,
            telegram::telegram_set_token,
            telegram::telegram_clear_token,
            telegram::telegram_pair,
            telegram::telegram_approve,
            telegram::telegram_revoke,
            telegram::telegram_revoke_all,
            telegram::telegram_send,
            telegram::telegram_start,
            telegram::telegram_stop,
            profile::persona_name,
            profile::persona_set_name,
            profile::profile_list,
            profile::profile_save,
            profile::profile_apply,
            profile::profile_delete,
            avatar::avatar_set_pose,
            avatar::avatar_set_animations,
            avatar::avatar_animations,
            avatar::avatar_animation_bytes,
            avatar::avatar_play,
            avatar::avatar_set_anchor,
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
            // Крестик прячет главное окно в трей, а не выключает ассистента:
            // вместе с окном иначе умирают напоминания, сочетания и голос
            // (docs/GAPS.md §3). Совсем выйти можно из меню трея.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    let app = window.app_handle();
                    if tray::close_to_tray(app, &app.state::<AppState>()) {
                        api.prevent_close();
                        crate::window::hide_main(app);
                        return;
                    }
                }
            }

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
