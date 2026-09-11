//! Показ и скрытие главного окна.
//!
//! # Зачем отдельный модуль на две функции
//!
//! Окно прячется и показывается из четырёх мест: крестик, меню трея, щелчок по
//! иконке и глобальное сочетание. Каждое из них должно сообщить странице, что
//! её больше не видно, — иначе интерфейс продолжает жить в трее.
//!
//! Причина вполне измеримая: анимации Orb (ТЗ §13) — это CSS, и WebView не
//! останавливает их у спрятанного окна сам. В покое это стоило 21 % одного
//! ядра **в трее**, то есть впустую: смотреть на анимацию в этот момент
//! некому. Пауза убирает эту трату целиком.

use tauri::{AppHandle, Emitter, Manager};

/// Событие видимости окна. Страница по нему ставит анимации на паузу.
pub const EVENT_VISIBLE: &str = "yuki://window-visible";

/// Показывает главное окно и возвращает ему фокус.
pub fn show_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
    let _ = app.emit(EVENT_VISIBLE, true);
}

/// Прячет главное окно в трей.
pub fn hide_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // Сначала сообщаем странице, потом прячем: событие, отправленное после
    // скрытия, тоже дойдёт, но пауза включится на кадр позже.
    let _ = app.emit(EVENT_VISIBLE, false);
    let _ = window.hide();
}

/// Переключает видимость: то же сочетание убирает окно с глаз.
///
/// Прячем только когда окно и видно, и активно. Видимое, но не активное окно по
/// сочетанию должно выйти на передний план, а не исчезнуть — иначе вызов
/// ассистента из другого приложения работал бы через раз.
pub fn toggle_main(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let visible = window.is_visible().unwrap_or(false);
    let focused = window.is_focused().unwrap_or(false);

    if visible && focused {
        hide_main(app);
    } else {
        show_main(app);
    }
}
