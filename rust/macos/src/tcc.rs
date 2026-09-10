//! Живой опрос статуса разрешений macOS (TCC).
//!
//! # Зачем это вообще нужно
//!
//! На macOS Accessibility и Screen Recording до выдачи ведут себя одинаково
//! плохо: API не возвращает ошибку, он возвращает **пустоту**. Дерево
//! интерфейса выглядит как «на экране ничего нет», снимок экрана — как «обои
//! без окон». Приложение, которое не спрашивает систему о статусе, в этот
//! момент честно рапортует ерунду.
//!
//! Поэтому статус спрашивается у системы напрямую, а не выводится из того,
//! получилось ли что-то прочитать.
//!
//! # Почему без крейтов
//!
//! Нужны ровно две функции из двух системных фреймворков. Тянуть ради них
//! `objc2` со всей его иерархией — несоразмерно: обе имеют C-совместимую
//! сигнатуру и объявляются напрямую.

/// Выдано ли разрешение Accessibility (управление интерфейсом).
///
/// `AXIsProcessTrusted` не показывает диалог и не меняет состояние — это
/// именно вопрос, а не запрос. Спрашивать её можно на каждом старте.
#[cfg(target_os = "macos")]
pub fn accessibility_granted() -> bool {
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    // Вызов не имеет побочных эффектов и не требует главного потока.
    unsafe { AXIsProcessTrusted() }
}

/// Выдано ли разрешение Screen Recording.
///
/// `CGPreflightScreenCaptureAccess` появилась в macOS 10.15 вместе с самим
/// разрешением; на 13+, которую требует ТЗ §2, она есть всегда.
///
/// Именно preflight, а не request: request показывает системный диалог, и
/// делать это на каждом старте — значит дёргать пользователя без повода.
/// Диалог уместен ровно один раз, из мастера первого запуска.
#[cfg(target_os = "macos")]
pub fn screen_recording_granted() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    unsafe { CGPreflightScreenCaptureAccess() }
}

/// Просит систему показать диалог выдачи Screen Recording.
///
/// Возвращает результат немедленно: если разрешение уже выдано — `true`, иначе
/// система показывает диалог и возвращает `false`, а фактический ответ
/// пользователя станет виден при следующем опросе. Так устроен сам API, и
/// притворяться, что он синхронный, нельзя.
#[cfg(target_os = "macos")]
pub fn request_screen_recording() -> bool {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    unsafe { CGRequestScreenCaptureAccess() }
}

// ── Заглушки для остальных платформ ─────────────────────────────────────────────
//
// Крейт собирается только под macOS, но модуль читают и правят с Windows —
// пусть он остаётся компилируемым и там, чтобы ошибка находилась сразу.

#[cfg(not(target_os = "macos"))]
pub fn accessibility_granted() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn screen_recording_granted() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_recording() -> bool {
    true
}
