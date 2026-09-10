//! Заметки (`docs/GAPS.md` §5).
//!
//! # Почему это отдельная сущность, а не память
//!
//! ТЗ §24 приводит сценарий «сохрани это в заметки», но модуля заметок в ТЗ нет
//! — ни в §25, где только календарь и напоминания, ни в §31, где нет таблицы.
//! Сложить заметки в память (ТЗ §9) было бы ошибкой: память Yuki читает сама и
//! подмешивает в контекст запроса, а заметка — это текст, который человек
//! написал для себя. Смешать их значит либо утопить память в чужих черновиках,
//! либо молча отправлять личные записи в модель на каждом запросе.
//!
//! Поэтому заметки — свой список: Yuki кладёт в него по просьбе и достаёт по
//! просьбе, но не читает его без спроса.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    /// Закреплённые идут первыми и не теряются в длинном списке.
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Придумывает заголовок по первой строке текста.
///
/// Заголовок нужен списку, но требовать его от человека, который диктует мысль
/// на ходу, — лишний вопрос. Берём первую строку, режем по границе слова:
/// обрубок посреди слова читается как ошибка.
pub fn derive_title(body: &str) -> String {
    const LIMIT: usize = 60;

    let first = body.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let trimmed = first.trim();

    if trimmed.chars().count() <= LIMIT {
        return trimmed.to_string();
    }

    let head: String = trimmed.chars().take(LIMIT).collect();
    match head.rsplit_once(' ') {
        Some((word_boundary, _)) if !word_boundary.trim().is_empty() => {
            format!("{}…", word_boundary.trim_end())
        }
        _ => format!("{head}…"),
    }
}

fn new_id() -> String {
    format!("note-{}", now())
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: row.get(0)?,
        title: row.get(1)?,
        body: row.get(2)?,
        pinned: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
    })
}

const COLUMNS: &str = "id, title, body, pinned, created_at, updated_at";

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Список заметок: закреплённые сверху, дальше свежие.
#[tauri::command]
pub fn note_list(state: State<'_, AppState>, query: Option<String>) -> Result<Vec<Note>, String> {
    let needle = query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(|q| format!("%{}%", q.to_lowercase()));

    state
        .storage
        .with_conn(|conn| {
            let sql = format!(
                "SELECT {COLUMNS} FROM notes
                 WHERE ?1 IS NULL
                    OR lower(title) LIKE ?1
                    OR lower(body) LIKE ?1
                 ORDER BY pinned DESC, updated_at DESC
                 LIMIT 500"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([needle], |r| read(r))?;
            rows.collect()
        })
        .map_err(err)
}

/// Создаёт или обновляет заметку.
#[tauri::command]
pub fn note_save(
    state: State<'_, AppState>,
    id: Option<String>,
    title: Option<String>,
    body: String,
    pinned: Option<bool>,
) -> Result<Note, String> {
    if body.trim().is_empty() {
        return Err("пустую заметку сохранять не в чем".into());
    }

    let id = id.filter(|s| !s.trim().is_empty()).unwrap_or_else(new_id);
    let title = title
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| derive_title(&body));

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO notes (id, title, body, pinned) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                   title = excluded.title,
                   body = excluded.body,
                   -- Закрепление не сбрасывается правкой текста: человек менял
                   -- содержание, а не решение держать заметку наверху.
                   pinned = COALESCE(?5, pinned),
                   updated_at = unixepoch()",
                rusqlite::params![
                    id,
                    title,
                    body.trim(),
                    pinned.unwrap_or(false) as i64,
                    pinned.map(i64::from)
                ],
            )?;

            conn.query_row(
                &format!("SELECT {COLUMNS} FROM notes WHERE id = ?1"),
                [&id],
                |r| read(r),
            )
        })
        .map_err(err)
}

#[tauri::command]
pub fn note_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM notes WHERE id = ?1", [&id]))
        .map(|_| ())
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_first_non_empty_line_as_a_title() {
        assert_eq!(derive_title("Купить молоко\nи хлеб"), "Купить молоко");
        assert_eq!(derive_title("\n\n  Идея  \nдальше"), "Идея");
    }

    #[test]
    fn cuts_a_long_title_on_a_word_boundary() {
        let body = "Позвонить Ивану насчёт договора аренды и уточнить сроки поставки оборудования";
        let title = derive_title(body);

        assert!(title.ends_with('…'));
        assert!(title.chars().count() <= 61, "{title}");
        // Обрубок посреди слова читается как ошибка ввода.
        assert!(!title.contains("обору…"), "{title}");
    }

    #[test]
    fn a_single_very_long_word_is_cut_anyway() {
        // Без пробелов границы слова нет — режем как есть, но с многоточием,
        // иначе заголовок молча притворится полным.
        let body = "а".repeat(200);
        let title = derive_title(&body);
        assert!(title.ends_with('…'));
        assert_eq!(title.chars().count(), 61);
    }

    #[test]
    fn an_empty_body_gives_an_empty_title_rather_than_a_panic() {
        assert_eq!(derive_title(""), "");
        assert_eq!(derive_title("   \n  "), "");
    }

    #[test]
    fn counts_characters_not_bytes() {
        // Кириллица занимает два байта на символ: обрезание по байтам разрубило
        // бы символ пополам и дало невалидный UTF-8.
        let body = "я".repeat(80);
        let title = derive_title(&body);
        assert_eq!(title.chars().count(), 61);
    }
}
