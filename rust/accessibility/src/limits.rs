//! Ограничения обхода дерева (ТЗ §6).
//!
//! Дерево реального приложения огромно: у браузера с открытой страницей это
//! десятки тысяч элементов. Отдать его модели целиком нельзя — оно не поместится
//! в запрос, а если поместится, то вытеснит собой всё остальное и будет стоить
//! дороже самой задачи. Поэтому обход ограничен с трёх сторон сразу, и каждое
//! ограничение решает свою проблему.

/// Насколько глубоко спускаться.
///
/// Осмысленные для управления элементы — кнопки, поля, пункты меню — лежат
/// неглубоко. Глубже начинается вёрстка: контейнеры внутри контейнеров, которые
/// ничего не добавляют к пониманию интерфейса.
pub const MAX_DEPTH: usize = 12;

/// Сколько всего узлов забирать.
///
/// Потолок на весь обход, а не на уровень: без него широкое дерево (длинный
/// список, таблица) разрастается вширь и съедает бюджет так же надёжно, как
/// глубокое — вглубь.
pub const MAX_NODES: usize = 400;

/// Сколько детей брать у одного узла.
///
/// Список из тысячи строк не станет понятнее от того, что модель увидит все
/// тысячу: первых достаточно, чтобы понять, что это список и что в нём.
pub const MAX_CHILDREN: usize = 40;

/// Максимальная длина текста в имени или значении элемента.
///
/// Значение текстового поля может быть целым документом; для понимания
/// интерфейса важно, что поле не пустое и что в нём примерно.
pub const MAX_TEXT: usize = 200;

/// Обрезает текст по границе символа, а не байта.
pub fn truncate(text: &str) -> String {
    if text.chars().count() <= MAX_TEXT {
        return text.to_string();
    }
    let cut: String = text.chars().take(MAX_TEXT).collect();
    format!("{cut}…")
}

/// Счётчик узлов, общий на весь обход.
pub struct Budget {
    remaining: usize,
}

impl Budget {
    pub fn new() -> Self {
        Self {
            remaining: MAX_NODES,
        }
    }

    /// Забирает один узел из бюджета. `false` — обход пора прекращать.
    pub fn take(&mut self) -> bool {
        if self.remaining == 0 {
            return false;
        }
        self.remaining -= 1;
        true
    }

    pub fn spent(&self) -> usize {
        MAX_NODES - self.remaining
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_on_character_boundaries() {
        let long = "я".repeat(MAX_TEXT + 50);
        let cut = truncate(&long);
        // Обрезка по байтам на кириллице дала бы битый символ.
        assert!(cut.ends_with('…'));
        assert_eq!(cut.chars().count(), MAX_TEXT + 1);
    }

    #[test]
    fn leaves_short_text_alone() {
        assert_eq!(truncate("кнопка"), "кнопка");
    }

    #[test]
    fn budget_runs_out_after_the_node_limit() {
        let mut budget = Budget::new();
        for _ in 0..MAX_NODES {
            assert!(budget.take());
        }
        assert!(!budget.take(), "бюджет узлов не ограничивает обход");
        assert_eq!(budget.spent(), MAX_NODES);
    }
}
