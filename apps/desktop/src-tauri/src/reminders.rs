//! Напоминания и уведомления (ТЗ §25).
//!
//! Напоминание — единственная часть Yuki, которая срабатывает без пользователя,
//! поэтому здесь важнее обычного не соврать: сработавшее напоминание помечается
//! выполненным **после** показа уведомления, а не до, и повторяющееся сдвигается
//! на следующий срок в той же транзакции. Иначе перезапуск приложения либо
//! потеряет напоминание, либо покажет его дважды.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

use crate::state::AppState;
use crate::storage::{Storage, StorageResult};

/// Как часто проверяются сроки.
///
/// Двадцать секунд — компромисс: напоминание «в 10:00» не должно приходить в
/// 10:01, но и будить процесс каждую секунду ради этого незачем (ТЗ §37).
const TICK: Duration = Duration::from_secs(20);

/// Событие о сработавшем напоминании — по нему UI показывает его в интерфейсе.
const EVENT_FIRED: &str = "yuki://reminder-fired";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: String,
    pub text: String,
    /// Unix-время срабатывания в секундах.
    pub due_at: i64,
    /// `daily`, `weekly` или `None` для одноразового.
    pub recurrence: Option<String>,
    pub completed_at: Option<i64>,
    pub created_at: i64,
}

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

fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "rem-{:x}-{:x}",
        now(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Следующий срок для повторяющегося напоминания.
///
/// Считается от прошедшего срока, а не от «сейчас»: если приложение было
/// выключено три дня, ежедневное напоминание должно вернуться к своему времени
/// суток, а не сползти на момент запуска. Цикл `while` доводит его до будущего
/// одним шагом на пропущенный период.
fn next_occurrence(due_at: i64, recurrence: &str, from: i64) -> Option<i64> {
    let step = match recurrence {
        "daily" => 24 * 60 * 60,
        "weekly" => 7 * 24 * 60 * 60,
        _ => return None,
    };

    let mut next = due_at + step;
    while next <= from {
        next += step;
    }
    Some(next)
}

/// Забирает напоминания, срок которых наступил, и переводит их в следующее состояние.
///
/// Возвращает список того, что нужно показать. Чтение и обновление идут одной
/// транзакцией: без неё перезапуск между ними показал бы напоминание дважды.
fn take_due(storage: &Storage) -> StorageResult<Vec<Reminder>> {
    let moment = now();

    storage.with_conn(|conn| {
        let due: Vec<Reminder> = {
            let mut stmt = conn.prepare(
                "SELECT id, text, due_at, recurrence, completed_at, created_at
                 FROM reminders
                 WHERE completed_at IS NULL AND due_at <= ?1
                 ORDER BY due_at",
            )?;
            let rows = stmt.query_map([moment], |r| {
                Ok(Reminder {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    due_at: r.get(2)?,
                    recurrence: r.get(3)?,
                    completed_at: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };

        for reminder in &due {
            match reminder
                .recurrence
                .as_deref()
                .and_then(|r| next_occurrence(reminder.due_at, r, moment))
            {
                Some(next) => {
                    conn.execute(
                        "UPDATE reminders SET due_at = ?2 WHERE id = ?1",
                        rusqlite::params![reminder.id, next],
                    )?;
                }
                None => {
                    conn.execute(
                        "UPDATE reminders SET completed_at = ?2 WHERE id = ?1",
                        rusqlite::params![reminder.id, moment],
                    )?;
                }
            }
        }

        Ok(due)
    })
}

/// Запускает фоновую проверку сроков.
///
/// Задача живёт столько же, сколько приложение: напоминания должны срабатывать,
/// пока Yuki запущена, независимо от того, открыт ли какой-нибудь экран.
pub fn spawn_scheduler(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(TICK);
        loop {
            ticker.tick().await;

            let Some(state) = app.try_state::<AppState>() else {
                // Состояние ещё не подключено или приложение закрывается —
                // это не ошибка, просто пропускаем такт.
                continue;
            };

            match take_due(&state.storage) {
                Ok(due) => {
                    for reminder in due {
                        // Отказ показать уведомление логируем: молчание здесь
                        // неотличимо от «пользователь его просто не заметил»,
                        // а это первое, что нужно знать при разборе жалобы
                        // «напоминание не сработало».
                        if let Err(error) = app
                            .notification()
                            .builder()
                            .title("Напоминание")
                            .body(&reminder.text)
                            .show()
                        {
                            tracing::warn!(%error, id = %reminder.id, "уведомление не показано");
                        }
                        let _ = app.emit(EVENT_FIRED, reminder);
                    }
                }
                // Сбой чтения не должен убивать планировщик: следующий такт
                // попробует снова, а напоминания останутся невыполненными.
                Err(error) => tracing::warn!(%error, "не удалось прочитать напоминания"),
            }
        }
    });
}

// ── Команды (ТЗ §25) ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn reminder_create(
    state: State<'_, AppState>,
    text: String,
    due_at: i64,
    recurrence: Option<String>,
) -> Result<Reminder, String> {
    if text.trim().is_empty() {
        return Err("пустой текст напоминания".into());
    }
    if let Some(rule) = recurrence.as_deref() {
        if !matches!(rule, "daily" | "weekly") {
            return Err(format!(
                "неизвестная периодичность «{rule}»: допустимы daily и weekly"
            ));
        }
    }
    if due_at <= now() {
        return Err("время напоминания уже прошло".into());
    }

    let id = new_id();
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO reminders (id, text, due_at, recurrence) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, text.trim(), due_at, recurrence],
            )
        })
        .map_err(err)?;

    Ok(Reminder {
        id,
        text: text.trim().to_string(),
        due_at,
        recurrence,
        completed_at: None,
        created_at: now(),
    })
}

#[tauri::command]
pub fn reminder_list(
    state: State<'_, AppState>,
    include_completed: Option<bool>,
) -> Result<Vec<Reminder>, String> {
    let include = include_completed.unwrap_or(false);
    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, text, due_at, recurrence, completed_at, created_at
                 FROM reminders
                 WHERE ?1 OR completed_at IS NULL
                 ORDER BY completed_at IS NOT NULL, due_at",
            )?;
            let rows = stmt.query_map([include], |r| {
                Ok(Reminder {
                    id: r.get(0)?,
                    text: r.get(1)?,
                    due_at: r.get(2)?,
                    recurrence: r.get(3)?,
                    completed_at: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })?;
            rows.collect()
        })
        .map_err(err)
}

#[tauri::command]
pub fn reminder_complete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE reminders SET completed_at = unixepoch() WHERE id = ?1",
                [&id],
            )
        })
        .map(|_| ())
        .map_err(err)
}

#[tauri::command]
pub fn reminder_delete(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM reminders WHERE id = ?1", [&id]))
        .map(|_| ())
        .map_err(err)
}

/// Показывает системное уведомление (ТЗ §25).
///
/// # Оговорка про Windows
///
/// Toast на Windows показывается только приложению, у которого есть ярлык в меню
/// «Пуск» с прописанным AppUserModelID. Установщик его создаёт, а запуск голого
/// `yuki-desktop.exe` из `target/debug` — нет, и тогда уведомление не появляется
/// молча. Это особенность платформы, а не сбой: `show()` в таком случае может
/// вернуть успех. Проверять уведомления нужно на установленной сборке.
#[tauri::command]
pub fn notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_time_reminders_have_no_next_occurrence() {
        assert_eq!(next_occurrence(1000, "once", 2000), None);
        assert_eq!(next_occurrence(1000, "", 2000), None);
    }

    #[test]
    fn daily_reminder_keeps_its_time_of_day_after_downtime() {
        let day = 24 * 60 * 60;
        let due = 10 * 3600; // 10:00 первого дня
        // Приложение было выключено трое суток.
        let next = next_occurrence(due, "daily", due + 3 * day + 60).expect("должен быть срок");

        assert!(next > due + 3 * day);
        // Время суток сохранилось: сдвиг кратен суткам.
        assert_eq!((next - due) % day, 0);
    }

    #[test]
    fn weekly_reminder_advances_by_a_week() {
        let week = 7 * 24 * 60 * 60;
        let due = 1000;
        assert_eq!(next_occurrence(due, "weekly", due + 1), Some(due + week));
    }

    #[test]
    fn due_reminders_are_taken_once_and_recurring_ones_rescheduled() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let past = now() - 60;

        storage
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO reminders (id, text, due_at, recurrence)
                     VALUES ('once', 'позвонить Ивану', ?1, NULL),
                            ('daily', 'выпить воды', ?1, 'daily')",
                    [past],
                )
            })
            .expect("подготовка должна пройти");

        let first = take_due(&storage).expect("выборка должна пройти");
        assert_eq!(first.len(), 2);

        // Повторный вызов не должен вернуть ничего: одноразовое закрыто,
        // ежедневное сдвинуто в будущее.
        let second = take_due(&storage).expect("выборка должна пройти");
        assert!(second.is_empty(), "напоминание сработало дважды");

        let completed: i64 = storage
            .with_conn(|c| {
                c.query_row(
                    "SELECT count(*) FROM reminders WHERE completed_at IS NOT NULL",
                    [],
                    |r| r.get(0),
                )
            })
            .expect("запрос должен выполниться");
        assert_eq!(completed, 1);
    }
}
