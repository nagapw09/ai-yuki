//! Выбор платформенной реализации системного слоя (ТЗ §30).
//!
//! Единственное место во всём проекте, где известно, на какой ОС мы работаем.
//! Всё, что выше по стеку, видит только трейты из `yuki-system`.

use yuki_system::{PlatformAdapters, SystemAdapter, SystemResult};

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("Yuki собирается только под Windows 10/11 и macOS 13+ (ТЗ §2)");

/// Собирает набор адаптеров для текущей платформы.
///
/// Вызывается один раз при старте: `enigo` и COM-объекты дорого создавать заново
/// на каждый вызов команды.
pub fn build() -> SystemResult<PlatformAdapters> {
    #[cfg(windows)]
    let system: Box<dyn SystemAdapter> = Box::new(yuki_windows::WindowsAdapter::new());

    #[cfg(target_os = "macos")]
    let system: Box<dyn SystemAdapter> = Box::new(yuki_macos::MacOsAdapter::new());

    Ok(PlatformAdapters {
        system,
        files: Box::new(yuki_filesystem::CrossPlatformFileAdapter::new()),
        input: Box::new(yuki_input::DesktopInputAdapter::new()?),
        screen: Box::new(yuki_screen::DesktopScreenAdapter::new()),
        clipboard: Box::new(yuki_input::DesktopClipboardAdapter::new()),
    })
}
