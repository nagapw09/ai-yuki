//! Память Yuki (ТЗ §9).
//!
//! Четыре типа с разным сроком жизни и разным назначением:
//!
//! - `short_term` — рабочие заметки внутри одной задачи, живут минуты;
//! - `session` — контекст текущего разговора, живёт до его конца;
//! - `long_term` — то, что Yuki знает о пользователе всегда: имя, язык,
//!   предпочтения, любимые приложения;
//! - `episodic` — что происходило: «вчера искали отчёт в Downloads».
//!
//! ТЗ §9 требует: хранить только разрешённые пользователем данные, память
//! локальная по умолчанию, и всё должно быть доступно для просмотра, правки и
//! удаления. Поэтому здесь нет ни одной записи, которую нельзя показать в UI,
//! а срок жизни задаётся явно, а не подразумевается.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;
use crate::storage::{Storage, StorageResult};

/// Сколько живёт краткосрочная запись, если срок не задан явно.
///
/// Час, а не «до перезапуска»: краткосрочная память, пережившая задачу, начинает
/// подсказывать модели вчерашние обстоятельства как сегодняшние.
const SHORT_TERM_TTL_SECONDS: i64 = 60 * 60;

/// Сколько записей долгосрочной памяти уходит в контекст одного запроса.
///
/// Ограничение обязательно: без него память со временем съедает весь бюджет
/// запроса и вытесняет собственно разговор.
const CONTEXT_LIMIT: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub kind: String,
    pub key: Option<String>,
    pub content: String,
    pub source: Option<String>,
    pub confidence: f64,
    pub expires_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Допустимые типы памяти. Проверяется до записи, а не только ограничением БД,
/// чтобы ошибка была понятной, а не «CHECK constraint failed».
fn validate_kind(kind: &str) -> Result<(), String> {
    match kind {
        "short_term" | "session" | "long_term" | "episodic" => Ok(()),
        other => Err(format!(
            "неизвестный тип памяти «{other}»: допустимы short_term, session, long_term, episodic"
        )),
    }
}

/// Удаляет протухшие записи.
///
/// Вызывается при старте и после каждой записи: просроченная запись, дожившая до
/// следующего запроса, попадёт в контекст модели как актуальная.
pub fn prune_expired(storage: &Storage) -> StorageResult<usize> {
    storage.with_conn(|conn| {
        conn.execute(
            "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at <= unixepoch()",
            [],
        )
    })
}

/// Идентификатор записи: монотонный и уникальный в пределах локальной базы.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("mem-{nanos:x}-{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

// ── Команды (ТЗ §9) ─────────────────────────────────────────────────────────────

/// Все записи или записи одного типа, свежие сверху.
#[tauri::command]
pub fn memory_list(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> Result<Vec<MemoryRecord>, String> {
    // Протухшее не должно попадать даже в UI: иначе пользователь увидит запись,
    // которой Yuki уже не пользуется.
    prune_expired(&state.storage).map_err(err)?;

    state
        .storage
        .with_conn(|conn| {
            let sql = "SELECT id, kind, key, content, source, confidence, expires_at,
                              created_at, updated_at
                       FROM memories
                       WHERE (?1 IS NULL OR kind = ?1)
                       ORDER BY updated_at DESC";
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([&kind], |r| {
                Ok(MemoryRecord {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    key: r.get(2)?,
                    content: r.get(3)?,
                    source: r.get(4)?,
                    confidence: r.get(5)?,
                    expires_at: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })?;
            rows.collect()
        })
        .map_err(err)
}

/// Сохраняет запись; при совпадении типа и ключа обновляет существующую.
///
/// Совпадение по ключу — это не оптимизация, а требование смысла: «любимый
/// браузер» должен быть один, а не накапливаться списком противоречивых фактов.
#[tauri::command]
pub fn memory_save(
    state: State<'_, AppState>,
    kind: String,
    content: String,
    key: Option<String>,
    source: Option<String>,
    ttl_seconds: Option<i64>,
) -> Result<MemoryRecord, String> {
    validate_kind(&kind)?;
    if content.trim().is_empty() {
        return Err("пустая запись памяти".into());
    }

    let expires_at = match (ttl_seconds, kind.as_str()) {
        (Some(ttl), _) if ttl > 0 => Some(now() + ttl),
        // Краткосрочная память обязана иметь срок: без него она перестаёт быть
        // краткосрочной и превращается в мусор, который никто не убирает.
        (None, "short_term") => Some(now() + SHORT_TERM_TTL_SECONDS),
        _ => None,
    };

    let existing: Option<String> = state
        .storage
        .with_conn(|conn| match &key {
            Some(k) => conn
                .query_row(
                    "SELECT id FROM memories WHERE kind = ?1 AND key = ?2",
                    rusqlite::params![kind, k],
                    |r| r.get::<_, String>(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                }),
            None => Ok(None),
        })
        .map_err(err)?;

    let id = existing.unwrap_or_else(new_id);

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO memories (id, kind, key, content, source, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                   content    = excluded.content,
                   source     = excluded.source,
                   expires_at = excluded.expires_at,
                   updated_at = unixepoch()",
                rusqlite::params![id, kind, key, content.trim(), source, expires_at],
            )
        })
        .map_err(err)?;

    prune_expired(&state.storage).map_err(err)?;

    state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT id, kind, key, content, source, confidence, expires_at,
                        created_at, updated_at
                 FROM memories WHERE id = ?1",
                [&id],
                |r| {
                    Ok(MemoryRecord {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        key: r.get(2)?,
                        content: r.get(3)?,
                        source: r.get(4)?,
                        confidence: r.get(5)?,
                        expires_at: r.get(6)?,
                        created_at: r.get(7)?,
                        updated_at: r.get(8)?,
                    })
                },
            )
        })
        .map_err(err)
}

/// Поиск по подстроке.
///
/// Векторный поиск отложен в фазу 4 вместе с эмбеддингами (колонка `embedding`
/// в схеме уже есть). Подстрока — честная замена на текущем объёме: локальная
/// память измеряется десятками записей, а не тысячами.
#[tauri::command]
pub fn memory_search(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<MemoryRecord>, String> {
    prune_expired(&state.storage).map_err(err)?;
    let limit = limit.unwrap_or(20).min(200);
    let needle = format!("%{}%", query.trim().to_lowercase());

    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, key, content, source, confidence, expires_at,
                        created_at, updated_at
                 FROM memories
                 WHERE lower(content) LIKE ?1 OR lower(coalesce(key, '')) LIKE ?1
                 ORDER BY updated_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![needle, limit], |r| {
                Ok(MemoryRecord {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    key: r.get(2)?,
                    content: r.get(3)?,
                    source: r.get(4)?,
                    confidence: r.get(5)?,
                    expires_at: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                })
            })?;
            rows.collect()
        })
        .map_err(err)
}

#[tauri::command]
pub fn memory_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM memories WHERE id = ?1", [&id]))
        .map(|_| ())
        .map_err(err)
}

/// Очистка: всей памяти или одного типа (ТЗ §9, «Clear All»).
#[tauri::command]
pub fn memory_clear(state: State<'_, AppState>, kind: Option<String>) -> Result<usize, String> {
    if let Some(kind) = &kind {
        validate_kind(kind)?;
    }
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "DELETE FROM memories WHERE (?1 IS NULL OR kind = ?1)",
                [&kind],
            )
        })
        .map_err(err)
}

/// Память, которая уходит в системную инструкцию перед запросом (ТЗ §9, §24).
///
/// Берутся только долгосрочные факты и свежие эпизоды: краткосрочная и сессионная
/// память живут внутри самого разговора, и дублировать их в системный блок —
/// значит платить за один и тот же текст дважды.
#[tauri::command]
pub fn memory_context(state: State<'_, AppState>) -> Result<String, String> {
    prune_expired(&state.storage).map_err(err)?;

    let rows: Vec<(String, Option<String>, String)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT kind, key, content FROM memories
                 WHERE kind IN ('long_term', 'episodic')
                 ORDER BY kind = 'long_term' DESC, updated_at DESC
                 LIMIT ?1",
            )?;
            let rows = stmt.query_map([CONTEXT_LIMIT as i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    if rows.is_empty() {
        return Ok(String::new());
    }

    let mut out = String::from("Что ты помнишь о пользователе:\n");
    for (kind, key, content) in rows {
        match key {
            Some(k) => out.push_str(&format!("- [{kind}] {k}: {content}\n")),
            None => out.push_str(&format!("- [{kind}] {content}\n")),
        }
    }
    out.push_str(
        "\nОпирайся на это, но не пересказывай без нужды. Если факт устарел — обнови его через инструмент памяти.",
    );
    Ok(out)
}

fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(storage: &Storage, kind: &str, key: Option<&str>, content: &str, expires: Option<i64>) {
        storage
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO memories (id, kind, key, content, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![new_id(), kind, key, content, expires],
                )
            })
            .expect("вставка должна пройти");
    }

    #[test]
    fn rejects_unknown_memory_kind() {
        assert!(validate_kind("long_term").is_ok());
        assert!(validate_kind("wishful").is_err());
    }

    #[test]
    fn prune_removes_only_expired_records() {
        let storage = Storage::in_memory().expect("база должна открыться");
        insert(&storage, "short_term", None, "протухло", Some(now() - 10));
        insert(&storage, "short_term", None, "ещё живо", Some(now() + 600));
        insert(&storage, "long_term", None, "без срока", None);

        let removed = prune_expired(&storage).expect("очистка должна пройти");
        assert_eq!(removed, 1);

        let left: i64 = storage
            .with_conn(|c| c.query_row("SELECT count(*) FROM memories", [], |r| r.get(0)))
            .expect("запрос должен выполниться");
        assert_eq!(left, 2);
    }

    #[test]
    fn context_prefers_long_term_over_episodes() {
        let storage = Storage::in_memory().expect("база должна открыться");
        insert(&storage, "episodic", None, "искали отчёт", None);
        insert(&storage, "long_term", Some("имя"), "Алекс", None);
        // Сессионная и краткосрочная память в системный блок не попадают.
        insert(&storage, "session", None, "сейчас обсуждаем сборку", None);

        let rows: Vec<(String, Option<String>, String)> = storage
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT kind, key, content FROM memories
                     WHERE kind IN ('long_term', 'episodic')
                     ORDER BY kind = 'long_term' DESC, updated_at DESC LIMIT 40",
                )?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
                rows.collect()
            })
            .expect("запрос должен выполниться");

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "long_term");
    }
}
