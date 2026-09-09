//! Кроссплатформенные интерфейсы системного слоя Yuki.
//!
//! Крейт содержит **только** трейты и типы (ТЗ §30). Платформенные реализации живут
//! в `yuki-windows` и `yuki-macos` и зависят от этого крейта, а не наоборот —
//! так исключается цикл зависимостей, а выбор реализации остаётся за приложением.

pub mod adapter;
pub mod error;
pub mod types;

pub use adapter::{
    ClipboardAdapter, FileAdapter, InputAdapter, PlatformAdapters, ScreenAdapter, SystemAdapter,
};
pub use error::{Platform, SystemError, SystemResult};
pub use types::{
    AccessibilityNode, AppInfo, FileEntry, FileQuery, FileSort, Modifier, MouseButton, Rect,
    CaptureOptions, ScreenCapture, SystemInfo, WindowInfo,
};
