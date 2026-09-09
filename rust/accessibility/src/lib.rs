//! Accessibility-дерево (ТЗ §6).
//!
//! ТЗ §6 задаёт приоритет однозначно: нативные Accessibility/UI API — основной путь,
//! computer vision и симуляция мыши — fallback. Причина в том, что дерево даёт роли,
//! состояния и точные границы элементов, тогда как vision-модель их угадывает.
//!
//! # Состояние реализации
//!
//! Обход дерева требует разного платформенного стека — UI Automation (COM) на Windows
//! и AX API (Objective-C runtime) на macOS — и **обязательно** сопровождается выдачей
//! системного разрешения Accessibility на macOS (ТЗ §21) с перезапуском приложения.
//! Поэтому обход вынесен в фазу 2 роадмапа вместе с остальным экранным пониманием.
//!
//! Сейчас крейт фиксирует контракт и честно возвращает [`SystemError::NotImplemented`],
//! чтобы агент по инварианту ТЗ §5 не мог отрапортовать об успехе там, где действия не было.

use yuki_system::{AccessibilityNode, SystemError, SystemResult};

/// Обход accessibility-дерева окна.
pub trait AccessibilityProvider: Send + Sync {
    /// Дерево указанного окна; `None` — активное окно.
    fn tree(&self, window_id: Option<u64>) -> SystemResult<AccessibilityNode>;

    /// Выдано ли системное разрешение на доступ к accessibility (ТЗ §21).
    ///
    /// На macOS без него API молча отдаёт пустое дерево, поэтому проверка обязана
    /// предшествовать обходу, а не следовать за пустым результатом.
    fn is_permitted(&self) -> bool;
}

/// Заглушка контракта до фазы 2.
pub struct UnavailableAccessibility;

impl AccessibilityProvider for UnavailableAccessibility {
    fn tree(&self, _window_id: Option<u64>) -> SystemResult<AccessibilityNode> {
        Err(SystemError::NotImplemented(
            "обход accessibility-дерева появится в фазе 2 (ТЗ §6)",
        ))
    }

    fn is_permitted(&self) -> bool {
        false
    }
}

/// Провайдер для текущей платформы.
pub fn provider() -> Box<dyn AccessibilityProvider> {
    Box::new(UnavailableAccessibility)
}
