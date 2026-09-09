//! Чтение интерфейса через UI Automation (ТЗ §6).
//!
//! UIA — родной для Windows способ узнать, что на экране: он отдаёт роли,
//! состояния и точные границы элементов. Именно поэтому ТЗ §6 ставит его выше
//! компьютерного зрения: снимок экрана нужно распознавать и угадывать, а здесь
//! приложение само рассказывает о себе.

#![cfg(windows)]

use uiautomation::controls::ControlType;
use uiautomation::patterns::{
    UIExpandCollapsePattern, UIInvokePattern, UIScrollPattern, UISelectionItemPattern,
    UITogglePattern, UIValuePattern,
};
use uiautomation::types::{Handle, UIProperty};
use uiautomation::{UIAutomation, UIElement, UITreeWalker};
use yuki_system::{AccessibilityNode, Rect, SystemError, SystemResult};

use crate::limits::{self, Budget};
use crate::AccessibilityProvider;

pub struct WindowsAccessibility;

impl WindowsAccessibility {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsAccessibility {
    fn default() -> Self {
        Self::new()
    }
}

fn platform_err(e: impl std::fmt::Display) -> SystemError {
    SystemError::Platform(e.to_string())
}

/// Человеческое имя роли.
///
/// Модель выбирает элемент по роли, поэтому важно, чтобы она читалась так же,
/// как в вебе и в accessibility-стандартах, а не как внутреннее имя UIA.
fn role_name(control: ControlType) -> &'static str {
    match control {
        ControlType::Button => "button",
        ControlType::CheckBox => "checkbox",
        ControlType::ComboBox => "combobox",
        ControlType::Edit => "textbox",
        ControlType::Hyperlink => "link",
        ControlType::Image => "image",
        ControlType::List => "list",
        ControlType::ListItem => "listitem",
        ControlType::Menu => "menu",
        ControlType::MenuBar => "menubar",
        ControlType::MenuItem => "menuitem",
        ControlType::RadioButton => "radio",
        ControlType::ScrollBar => "scrollbar",
        ControlType::Slider => "slider",
        ControlType::Tab => "tablist",
        ControlType::TabItem => "tab",
        ControlType::Text => "text",
        ControlType::ToolBar => "toolbar",
        ControlType::Tree => "tree",
        ControlType::TreeItem => "treeitem",
        ControlType::Window => "window",
        ControlType::Document => "document",
        ControlType::Group => "group",
        ControlType::Pane => "pane",
        ControlType::Table => "table",
        ControlType::DataGrid => "grid",
        ControlType::Custom => "custom",
        _ => "element",
    }
}

/// Действия, которые элемент объявляет через свои паттерны.
///
/// Список нужен модели, чтобы не пытаться нажать на текст и не искать способ
/// ввести значение в кнопку.
fn actions_of(element: &UIElement) -> Vec<String> {
    let mut actions = Vec::new();

    if element.get_pattern::<UIInvokePattern>().is_ok() {
        actions.push("press".to_string());
    }
    if element.get_pattern::<UITogglePattern>().is_ok() {
        actions.push("toggle".to_string());
    }
    if element.get_pattern::<UIValuePattern>().is_ok() {
        actions.push("set_value".to_string());
    }
    if element.get_pattern::<UIExpandCollapsePattern>().is_ok() {
        actions.push("expand".to_string());
    }
    if element.get_pattern::<UISelectionItemPattern>().is_ok() {
        actions.push("select".to_string());
    }
    if element.get_pattern::<UIScrollPattern>().is_ok() {
        actions.push("scroll".to_string());
    }

    actions
}

fn value_of(element: &UIElement) -> Option<String> {
    element
        .get_property_value(UIProperty::ValueValue)
        .ok()
        .and_then(|v| v.get_string().ok())
        .map(|s| limits::truncate(s.trim()))
        .filter(|s| !s.is_empty())
}

fn bounds_of(element: &UIElement) -> Option<Rect> {
    element.get_bounding_rectangle().ok().map(|r| Rect {
        x: r.get_left(),
        y: r.get_top(),
        width: r.get_right() - r.get_left(),
        height: r.get_bottom() - r.get_top(),
    })
}

/// Стоит ли вообще включать узел в дерево.
///
/// Отсекается служебная вёрстка: безымянный контейнер нулевого размера без
/// действий не говорит модели ничего, но занимает место в бюджете и внимании.
fn is_meaningful(node: &AccessibilityNode) -> bool {
    if node.name.is_some() || node.value.is_some() || !node.actions.is_empty() {
        return true;
    }
    // Пустой контейнер оставляем, только если у него есть содержимое.
    !node.children.is_empty()
}

fn build(
    walker: &UITreeWalker,
    element: &UIElement,
    depth: usize,
    budget: &mut Budget,
) -> Option<AccessibilityNode> {
    if depth > limits::MAX_DEPTH || !budget.take() {
        return None;
    }

    let control = element.get_control_type().unwrap_or(ControlType::Custom);
    let name = element
        .get_name()
        .ok()
        .map(|n| limits::truncate(n.trim()))
        .filter(|n| !n.is_empty());

    let mut children = Vec::new();
    if let Ok(first) = walker.get_first_child(element) {
        let mut current = Some(first);
        while let Some(child) = current {
            if children.len() >= limits::MAX_CHILDREN {
                break;
            }
            if let Some(node) = build(walker, &child, depth + 1, budget) {
                children.push(node);
            }
            current = walker.get_next_sibling(&child).ok();
        }
    }

    let node = AccessibilityNode {
        role: role_name(control).to_string(),
        name,
        value: value_of(element),
        bounds: bounds_of(element),
        enabled: element.is_enabled().unwrap_or(true),
        focused: element.has_keyboard_focus().unwrap_or(false),
        actions: actions_of(element),
        children,
    };

    is_meaningful(&node).then_some(node)
}

impl AccessibilityProvider for WindowsAccessibility {
    fn tree(&self, window_id: Option<u64>) -> SystemResult<AccessibilityNode> {
        let automation = UIAutomation::new().map_err(platform_err)?;

        let root = match window_id {
            Some(id) => {
                // Handle принимает сырой isize — это и есть HWND, каким его
                // отдаёт системный слой. Тянуть сюда крейт `windows` только ради
                // конструктора значит привязаться к его версии в двух местах.
                let handle = Handle::from(id as isize);
                automation
                    .element_from_handle(handle)
                    .map_err(|e| SystemError::NotFound(format!("окно #{id}: {e}")))?
            }
            None => automation.get_focused_element().map_err(platform_err)?,
        };

        let walker = automation.get_control_view_walker().map_err(platform_err)?;
        let mut budget = Budget::new();

        let tree = build(&walker, &root, 0, &mut budget).ok_or_else(|| {
            SystemError::NotFound("окно не отдало ни одного значимого элемента".into())
        })?;

        tracing::debug!(nodes = budget.spent(), "прочитано дерево интерфейса");
        Ok(tree)
    }

    fn is_permitted(&self) -> bool {
        // Windows не требует отдельного разрешения на чтение чужого интерфейса:
        // UIA доступен любому процессу пользователя. Согласие пользователя
        // проверяется уровнем выше, политикой разрешений Yuki (ТЗ §21).
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: Option<&str>, actions: Vec<&str>, children: usize) -> AccessibilityNode {
        AccessibilityNode {
            role: "group".into(),
            name: name.map(str::to_string),
            value: None,
            bounds: None,
            enabled: true,
            focused: false,
            actions: actions.into_iter().map(str::to_string).collect(),
            children: (0..children)
                .map(|_| AccessibilityNode {
                    role: "text".into(),
                    name: Some("что-то".into()),
                    value: None,
                    bounds: None,
                    enabled: true,
                    focused: false,
                    actions: Vec::new(),
                    children: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn keeps_elements_that_say_something_about_the_interface() {
        assert!(is_meaningful(&node(Some("Сохранить"), vec![], 0)));
        assert!(is_meaningful(&node(None, vec!["press"], 0)));
        assert!(is_meaningful(&node(None, vec![], 2)));
    }

    #[test]
    fn drops_empty_layout_containers() {
        // Безымянный контейнер без действий и без содержимого — это вёрстка,
        // а не интерфейс.
        assert!(!is_meaningful(&node(None, vec![], 0)));
    }

    #[test]
    fn maps_control_types_to_familiar_role_names() {
        assert_eq!(role_name(ControlType::Button), "button");
        assert_eq!(role_name(ControlType::Edit), "textbox");
        assert_eq!(role_name(ControlType::Hyperlink), "link");
    }
}
