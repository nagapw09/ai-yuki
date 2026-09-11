//! Трей, фоновый режим и автозапуск (`docs/GAPS.md` §3).
//!
//! # Почему это не украшение
//!
//! Голосовой ассистент со словом пробуждения обязан жить в фоне. Приложение,
//! которое закрывается по крестику, слышит пользователя ровно тогда, когда он
//! на него смотрит, — то есть тогда, когда проще нажать кнопку. ТЗ §16
//! упоминает триггер `startup`, но нигде не требует, чтобы Yuki вообще
//! оставалась запущенной; этот пропуск и закрывается здесь.
//!
//! # Состояние видно из трея
//!
//! Иконка перекрашивается под состояние из ТЗ §13 — тот же автомат, что у Orb
//! и аватара. Рисуется она в коде, а не берётся из восьми файлов: восемь
//! почти одинаковых картинок пришлось бы держать в двух размерах на две
//! платформы и следить, чтобы они не разошлись с палитрой дизайн-системы.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

/// Идентификатор иконки в трее.
const TRAY_ID: &str = "yuki-tray";

/// Событие перехода на экран — его слушает главное окно.
const EVENT_NAVIGATE: &str = "yuki://navigate";

/// Ключи настроек.
const KEY_CLOSE_TO_TRAY: &str = "tray.close_to_tray";
const KEY_ALWAYS_ON_TOP: &str = "window.always_on_top";

/// Размер иконки.
///
/// 32 пикселя: Windows берёт 16 или 32 в зависимости от масштаба, macOS
/// уменьшает до 22 и любит чётные размеры. Меньший размер система увеличивать
/// не станет, а больший уменьшит без потерь.
const ICON_SIZE: u32 = 32;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Иконка состояния ────────────────────────────────────────────────────────────

/// Цвет состояния — те же значения, что в токенах дизайн-системы (ТЗ §14).
///
/// Продублированы здесь, потому что иконку трея рисует Rust, а токены живут в
/// CSS. Тест ниже следит, чтобы значения не разошлись.
fn state_color(state: &str) -> [u8; 3] {
    match state {
        "listening" => [0x7f, 0xd1, 0xff],
        "thinking" => [0x6c, 0x5c, 0xe7],
        "working" => [0xff, 0x9f, 0x6e],
        "speaking" => [0xe8, 0xf7, 0xff],
        "success" => [0x4a, 0xde, 0x9b],
        "error" => [0xff, 0x6b, 0x7a],
        "sleeping" => [0x6b, 0x77, 0x86],
        // idle и всё незнакомое — спокойный основной цвет.
        _ => [0xa8, 0xb2, 0xc0],
    }
}

/// Рисует круг заданного цвета в RGBA.
///
/// Со сглаженным краем: ступенчатый круг в трее выглядит как артефакт, а не
/// как индикатор. Сглаживание считается по расстоянию до центра, поэтому не
/// зависит от размера.
pub fn state_icon(state: &str, size: u32) -> Vec<u8> {
    let [r, g, b] = state_color(state);
    let center = (size as f32 - 1.0) / 2.0;
    // Небольшой отступ от края: система рисует иконку впритык к границам, и
    // круг во всю ширину сливается с соседями на панели.
    let radius = size as f32 / 2.0 - 1.5;

    let mut pixels = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let distance = (dx * dx + dy * dy).sqrt();

            // Полоса в один пиксель по краю — переход от непрозрачного к пустому.
            let alpha = ((radius - distance) + 0.5).clamp(0.0, 1.0);

            pixels.extend_from_slice(&[r, g, b, (alpha * 255.0) as u8]);
        }
    }

    pixels
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

fn set_setting(state: &AppState, key: &str, value: bool) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![key, if value { "on" } else { "off" }],
            )
        })
        .map(|_| ())
        .map_err(err)
}

pub fn flag(state: &AppState, key: &str, default: bool) -> bool {
    match setting(state, key).as_deref() {
        Some("on") => true,
        Some("off") => false,
        _ => default,
    }
}

/// Закрывать окно в трей, а не выходить.
///
/// По умолчанию да: иначе крестик выключает и слово пробуждения, и напоминания,
/// и команды по сочетанию — а человек этого не заказывал.
///
/// Но только пока трей действительно есть. Спрятать окно туда, где его нечем
/// достать, — это не фоновый режим, а пропавшее приложение.
pub fn close_to_tray(app: &AppHandle, state: &AppState) -> bool {
    app.tray_by_id(TRAY_ID).is_some() && flag(state, KEY_CLOSE_TO_TRAY, true)
}

// ── Построение трея ─────────────────────────────────────────────────────────────

/// Создаёт иконку в трее с меню.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Показать Yuki", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;

    let autostart_on = autostart_enabled(app);
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "Запускать при входе в систему",
        true,
        autostart_on,
        None::<&str>,
    )?;

    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Выйти", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show, &settings, &separator, &autostart, &separator, &quit])?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::new_owned(
            state_icon("idle", ICON_SIZE),
            ICON_SIZE,
            ICON_SIZE,
        ))
        .tooltip("Yuki")
        .menu(&menu)
        // Меню по левой кнопке мешает главному действию — открыть окно.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "settings" => {
                show_main(app);
                let _ = app.emit(EVENT_NAVIGATE, "settings");
            }
            "autostart" => toggle_autostart(app),
            "quit" => {
                // Явный выход из меню — единственный способ закрыть Yuki
                // полностью, когда крестик прячет её в трей.
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

/// Показ окна идёт общим путём: он же сообщает странице, что её снова видно.
fn show_main(app: &AppHandle) {
    crate::window::show_main(app);
}

// ── Автозапуск ──────────────────────────────────────────────────────────────────

fn autostart_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

fn toggle_autostart(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    let enabled = manager.is_enabled().unwrap_or(false);

    // Результат намеренно не игнорируется молча: запись в автозапуск может не
    // пройти из-за политики домена, и человек должен видеть это в логе.
    let outcome = if enabled {
        manager.disable()
    } else {
        manager.enable()
    };

    if let Err(error) = outcome {
        tracing::warn!(%error, "не удалось изменить автозапуск");
    }
}

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Состояние фонового режима для настроек.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundStatus {
    pub close_to_tray: bool,
    pub always_on_top: bool,
    pub autostart: bool,
}

#[tauri::command]
pub fn background_status(app: AppHandle, state: State<'_, AppState>) -> BackgroundStatus {
    BackgroundStatus {
        close_to_tray: close_to_tray(&app, &state),
        always_on_top: flag(&state, KEY_ALWAYS_ON_TOP, false),
        // Спрашиваем у системы, а не у своей настройки: запись в автозапуск
        // могли снять снаружи, и своя копия про это не знает.
        autostart: autostart_enabled(&app),
    }
}

#[tauri::command]
pub fn background_set_close_to_tray(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    set_setting(&state, KEY_CLOSE_TO_TRAY, enabled)
}

/// Держать ли главное окно поверх остальных (`docs/GAPS.md` §3: оверлей).
#[tauri::command]
pub fn background_set_always_on_top(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.set_always_on_top(enabled).map_err(err)?;
    }
    set_setting(&state, KEY_ALWAYS_ON_TOP, enabled)
}

#[tauri::command]
pub fn background_set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(err)
    } else {
        manager.disable().map_err(err)
    }
}

/// Показывает состояние Yuki в трее.
///
/// Вызывается тем же кодом, который рассылает состояние аватару: источник
/// истины один, и расходиться этим двум индикаторам не с чего.
#[tauri::command]
pub fn tray_set_state(app: AppHandle, state: String) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        // Трея может не быть: система без области уведомлений — это не ошибка
        // приложения, и падать из-за неё нельзя.
        return Ok(());
    };

    tray.set_icon(Some(Image::new_owned(
        state_icon(&state, ICON_SIZE),
        ICON_SIZE,
        ICON_SIZE,
    )))
    .map_err(err)?;

    tray.set_tooltip(Some(tooltip(&state))).map_err(err)
}

fn tooltip(state: &str) -> &'static str {
    match state {
        "listening" => "Yuki — слушает",
        "thinking" => "Yuki — думает",
        "working" => "Yuki — работает",
        "speaking" => "Yuki — отвечает",
        "success" => "Yuki — готово",
        "error" => "Yuki — не получилось",
        "sleeping" => "Yuki — спит",
        _ => "Yuki",
    }
}

/// Применяет сохранённые настройки окна при старте.
pub fn restore(app: &AppHandle) {
    let state = app.state::<AppState>();
    if !flag(&state, KEY_ALWAYS_ON_TOP, false) {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_always_on_top(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(icon: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * size + x) * 4) as usize;
        [
            icon[offset],
            icon[offset + 1],
            icon[offset + 2],
            icon[offset + 3],
        ]
    }

    #[test]
    fn the_icon_has_the_size_the_system_expects() {
        let icon = state_icon("idle", ICON_SIZE);
        assert_eq!(icon.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn the_centre_carries_the_colour_of_the_state() {
        let icon = state_icon("error", ICON_SIZE);
        let [r, g, b, a] = pixel(&icon, ICON_SIZE, ICON_SIZE / 2, ICON_SIZE / 2);
        assert_eq!([r, g, b], [0xff, 0x6b, 0x7a]);
        assert_eq!(a, 255, "центр должен быть непрозрачным");
    }

    #[test]
    fn the_corners_stay_transparent() {
        // Непрозрачный угол превращает индикатор в квадрат на панели.
        let icon = state_icon("idle", ICON_SIZE);
        assert_eq!(pixel(&icon, ICON_SIZE, 0, 0)[3], 0);
        assert_eq!(pixel(&icon, ICON_SIZE, ICON_SIZE - 1, ICON_SIZE - 1)[3], 0);
    }

    #[test]
    fn every_orb_state_has_its_own_colour() {
        // Два состояния одного цвета неотличимы в трее — индикатор бесполезен.
        let states = [
            "idle",
            "listening",
            "thinking",
            "working",
            "speaking",
            "success",
            "error",
            "sleeping",
        ];

        let mut seen = Vec::new();
        for state in states {
            let color = state_color(state);
            assert!(!seen.contains(&color), "цвет «{state}» уже занят");
            seen.push(color);
        }
    }

    #[test]
    fn colours_match_the_design_tokens() {
        // Палитра живёт в CSS, а иконку рисует Rust — разойтись им нельзя.
        let tokens = include_str!("../../src/design-system/tokens.css");
        for (state, token) in [
            ("listening", "#7fd1ff"),
            ("thinking", "#6c5ce7"),
            ("working", "#ff9f6e"),
            ("success", "#4ade9b"),
            ("error", "#ff6b7a"),
        ] {
            assert!(tokens.contains(token), "токена {token} больше нет в палитре");

            let [r, g, b] = state_color(state);
            assert_eq!(
                format!("#{r:02x}{g:02x}{b:02x}"),
                token,
                "цвет состояния «{state}» разошёлся с токеном"
            );
        }
    }

    #[test]
    fn an_unknown_state_still_produces_a_valid_icon() {
        let icon = state_icon("что-то новое", ICON_SIZE);
        assert_eq!(icon.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        assert_eq!(tooltip("что-то новое"), "Yuki");
    }
}
