//! Симуляция ввода и буфер обмена (ТЗ §6, §30).
//!
//! Важно по ТЗ §6: симуляция мыши — **fallback**. Приоритетный путь взаимодействия с
//! интерфейсом — accessibility-дерево (`ScreenAdapter::accessibility_tree`), потому что
//! оно даёт точные роли и границы элементов, а не догадки по пикселям. Сюда агент
//! приходит, когда дерево недоступно или элемент не объявляет нужного действия.

use std::sync::Mutex;

use enigo::{
    Button, Coordinate, Direction, Enigo, Key, Keyboard as _, Mouse as _, Settings,
};
use yuki_system::{
    ClipboardAdapter, InputAdapter, Modifier, MouseButton, SystemError, SystemResult,
};

pub struct DesktopInputAdapter {
    // Enigo держит платформенное состояние и не Sync, поэтому живёт под мьютексом.
    enigo: Mutex<Enigo>,
}

impl DesktopInputAdapter {
    pub fn new() -> SystemResult<Self> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| SystemError::Platform(format!("не удалось инициализировать ввод: {e}")))?;
        Ok(Self {
            enigo: Mutex::new(enigo),
        })
    }

    fn with_enigo<T>(&self, f: impl FnOnce(&mut Enigo) -> SystemResult<T>) -> SystemResult<T> {
        let mut guard = self
            .enigo
            .lock()
            .map_err(|_| SystemError::Platform("состояние ввода повреждено".into()))?;
        f(&mut guard)
    }
}

/// Разбор имени клавиши в нотации Yuki: `a`, `enter`, `f5`, `left`, `pageup`, ...
fn parse_key(name: &str) -> SystemResult<Key> {
    let lower = name.trim().to_lowercase();

    // Функциональные клавиши задаются как f1..f12: в enigo это отдельные варианты,
    // а не индексируемый вариант, поэтому раскрываем список явно.
    if let Some(key) = parse_function_key(&lower) {
        return Ok(key);
    }

    Ok(match lower.as_str() {
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "escape" | "esc" => Key::Escape,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        _ => {
            let mut chars = lower.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => Key::Unicode(c),
                _ => {
                    return Err(SystemError::InvalidArgument(format!(
                        "неизвестная клавиша: {name}"
                    )))
                }
            }
        }
    })
}

fn parse_function_key(lower: &str) -> Option<Key> {
    Some(match lower {
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        _ => return None,
    })
}

fn modifier_key(m: Modifier) -> Key {
    match m {
        Modifier::Ctrl => Key::Control,
        Modifier::Alt => Key::Alt,
        Modifier::Shift => Key::Shift,
        Modifier::Meta => Key::Meta,
    }
}

fn map_button(b: MouseButton) -> Button {
    match b {
        MouseButton::Left => Button::Left,
        MouseButton::Right => Button::Right,
        MouseButton::Middle => Button::Middle,
    }
}

fn platform_err(e: impl std::fmt::Display) -> SystemError {
    SystemError::Platform(e.to_string())
}

impl InputAdapter for DesktopInputAdapter {
    fn type_text(&self, text: &str) -> SystemResult<()> {
        self.with_enigo(|e| e.text(text).map_err(platform_err))
    }

    fn press_key(&self, key: &str, modifiers: &[Modifier]) -> SystemResult<()> {
        let target = parse_key(key)?;
        self.with_enigo(|e| {
            for m in modifiers {
                e.key(modifier_key(*m), Direction::Press).map_err(platform_err)?;
            }
            let result = e.key(target, Direction::Click).map_err(platform_err);
            // Модификаторы отпускаем в любом случае: иначе Ctrl останется зажатым
            // на уровне ОС и сломает пользователю всю систему, а не только команду.
            for m in modifiers.iter().rev() {
                let _ = e.key(modifier_key(*m), Direction::Release);
            }
            result
        })
    }

    fn mouse_move(&self, x: i32, y: i32) -> SystemResult<()> {
        self.with_enigo(|e| e.move_mouse(x, y, Coordinate::Abs).map_err(platform_err))
    }

    fn mouse_click(&self, button: MouseButton) -> SystemResult<()> {
        self.with_enigo(|e| {
            e.button(map_button(button), Direction::Click)
                .map_err(platform_err)
        })
    }

    fn mouse_scroll(&self, dx: i32, dy: i32) -> SystemResult<()> {
        self.with_enigo(|e| {
            if dx != 0 {
                e.scroll(dx, enigo::Axis::Horizontal).map_err(platform_err)?;
            }
            if dy != 0 {
                e.scroll(dy, enigo::Axis::Vertical).map_err(platform_err)?;
            }
            Ok(())
        })
    }

    fn cursor_position(&self) -> SystemResult<(i32, i32)> {
        self.with_enigo(|e| e.location().map_err(platform_err))
    }
}

/// Буфер обмена (ТЗ §16 actions, §24 context awareness).
pub struct DesktopClipboardAdapter;

impl DesktopClipboardAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DesktopClipboardAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardAdapter for DesktopClipboardAdapter {
    fn read_text(&self) -> SystemResult<Option<String>> {
        let mut cb = arboard::Clipboard::new().map_err(platform_err)?;
        match cb.get_text() {
            Ok(text) => Ok(Some(text)),
            // Пустой или не-текстовый буфер — это не ошибка, а отсутствие текста.
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(platform_err(e)),
        }
    }

    fn write_text(&self, text: &str) -> SystemResult<()> {
        let mut cb = arboard::Clipboard::new().map_err(platform_err)?;
        cb.set_text(text.to_owned()).map_err(platform_err)
    }
}
