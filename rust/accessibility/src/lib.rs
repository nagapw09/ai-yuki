//! Accessibility-дерево (ТЗ §6).
//!
//! ТЗ §6 задаёт приоритет однозначно: нативные Accessibility/UI API — основной
//! путь, computer vision и симуляция мыши — fallback. Причина в том, что дерево
//! даёт роли, состояния и точные границы элементов, тогда как vision-модель их
//! угадывает по картинке.
//!
//! # Состояние по платформам
//!
//! - **Windows** — реализовано через UI Automation.
//! - **macOS** — контракт зафиксирован, обход не реализован. AX API требует
//!   системного разрешения Accessibility (ТЗ §21), которое выдаётся вручную
//!   и вступает в силу только после перезапуска приложения; без живой проверки
//!   этого статуса обход возвращал бы пустое дерево, неотличимое от «на экране
//!   ничего нет». Пока честнее вернуть ошибку.

pub mod limits;
pub mod windows;

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

/// Заглушка для платформ, где обход ещё не реализован.
pub struct UnavailableAccessibility;

impl AccessibilityProvider for UnavailableAccessibility {
    fn tree(&self, _window_id: Option<u64>) -> SystemResult<AccessibilityNode> {
        Err(SystemError::NotImplemented(
            "чтение интерфейса на этой платформе пока не реализовано (ТЗ §6)",
        ))
    }

    fn is_permitted(&self) -> bool {
        false
    }
}

/// Провайдер для текущей платформы.
pub fn provider() -> Box<dyn AccessibilityProvider> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsAccessibility::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(UnavailableAccessibility)
    }
}

/// Компактное текстовое представление дерева для модели.
///
/// JSON здесь хуже отступов: он тратит на скобки и кавычки больше места, чем
/// на содержание, а вложенность и так видна по отступу. На дереве в сотни узлов
/// разница в объёме — заметная доля запроса.
pub fn render(node: &AccessibilityNode) -> String {
    let mut out = String::new();
    write_node(node, 0, &mut out);
    out
}

fn write_node(node: &AccessibilityNode, depth: usize, out: &mut String) {
    out.push_str(&"  ".repeat(depth));
    out.push_str(&node.role);

    if let Some(name) = &node.name {
        out.push_str(&format!(" \"{name}\""));
    }
    if let Some(value) = &node.value {
        out.push_str(&format!(" = \"{value}\""));
    }
    if !node.enabled {
        out.push_str(" [выключен]");
    }
    if node.focused {
        out.push_str(" [фокус]");
    }
    if !node.actions.is_empty() {
        out.push_str(&format!(" ({})", node.actions.join(", ")));
    }
    if let Some(b) = &node.bounds {
        // Координаты нужны как запасной путь: если элемент не объявляет действий,
        // остаётся кликнуть по его центру.
        out.push_str(&format!(
            " @{},{} {}×{}",
            b.x, b.y, b.width, b.height
        ));
    }
    out.push('\n');

    for child in &node.children {
        write_node(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_system::Rect;

    fn button() -> AccessibilityNode {
        AccessibilityNode {
            role: "button".into(),
            name: Some("Сохранить".into()),
            value: None,
            bounds: Some(Rect {
                x: 10,
                y: 20,
                width: 80,
                height: 24,
            }),
            enabled: true,
            focused: true,
            actions: vec!["press".into()],
            children: Vec::new(),
        }
    }

    #[test]
    fn renders_a_node_with_everything_the_model_needs_to_act() {
        let text = render(&button());
        assert!(text.contains("button \"Сохранить\""));
        assert!(text.contains("[фокус]"));
        assert!(text.contains("(press)"));
        assert!(text.contains("@10,20 80×24"));
    }

    #[test]
    fn nests_children_by_indentation() {
        let tree = AccessibilityNode {
            role: "window".into(),
            name: Some("Блокнот".into()),
            value: None,
            bounds: None,
            enabled: true,
            focused: false,
            actions: Vec::new(),
            children: vec![button()],
        };

        let text = render(&tree);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].starts_with("window"));
        assert!(lines[1].starts_with("  button"));
    }

    #[test]
    fn marks_disabled_elements_so_the_model_does_not_try_them() {
        let mut node = button();
        node.enabled = false;
        assert!(render(&node).contains("[выключен]"));
    }
}
