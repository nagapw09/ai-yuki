//! Состояние приложения, разделяемое всеми Tauri-командами.

use yuki_system::PlatformAdapters;

use crate::storage::Storage;

/// Всё, что живёт столько же, сколько процесс Yuki.
///
/// Адаптеры создаются один раз: `enigo` держит платформенное состояние ввода,
/// а пересоздание COM-объектов на каждый вызов стоит миллисекунд, которых нет
/// в бюджете отзывчивости из ТЗ §37.
pub struct AppState {
    pub adapters: PlatformAdapters,
    pub storage: Storage,
}

impl AppState {
    pub fn new(adapters: PlatformAdapters, storage: Storage) -> Self {
        Self { adapters, storage }
    }
}
