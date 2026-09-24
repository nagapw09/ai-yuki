//! Хранение команд и автоматизаций (ТЗ §16).
//!
//! Само выполнение живёт в TypeScript-ядре: шаг команды — это вызов инструмента,
//! а реестр инструментов и Permission Gate там же. Здесь только то, чего в ядре
//! быть не может: сохранение между запусками и регистрация глобальных сочетаний.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::state::AppState;

/// Команда в том виде, в каком её показывает и сохраняет интерфейс.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRecord {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// `phrase` · `hotkey` · `startup` · `manual` · `schedule` · `app`
    pub trigger_kind: String,
    pub phrase: Option<String>,
    pub hotkey: Option<String>,
    /// Расписание «ЧЧ:ММ|дни»: «09:00|1,2,3,4,5» — по будням в девять.
    #[serde(default)]
    pub schedule: Option<String>,
    /// Программа, с запуском которой выполняется команда.
    #[serde(default)]
    pub app: Option<String>,
    pub enabled: bool,
    /// Шаги программы; структура описана в ядре.
    pub steps: Vec<Value>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[tauri::command]
pub fn command_list(state: State<'_, AppState>) -> Result<Vec<CommandRecord>, String> {
    list(&state)
}

/// Отдельная функция: её зовёт и команда, и регистрация хоткеев при старте.
pub fn list(state: &AppState) -> Result<Vec<CommandRecord>, String> {
    let rows: Vec<(
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        bool,
        Option<String>,
        Option<String>,
    )> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, trigger_kind, phrase, hotkey, enabled, schedule, app
                 FROM commands ORDER BY name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get::<_, i64>(6)? != 0,
                    r.get(7)?,
                    r.get(8)?,
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    let mut commands = Vec::with_capacity(rows.len());
    for (id, name, description, trigger_kind, phrase, hotkey, enabled, schedule, app) in rows {
        let steps: Vec<Value> = state
            .storage
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT config FROM command_nodes WHERE command_id = ?1 ORDER BY position",
                )?;
                let rows = stmt.query_map([&id], |r| r.get::<_, String>(0))?;
                rows.collect::<rusqlite::Result<Vec<String>>>()
            })
            .map_err(err)?
            .into_iter()
            // Битый шаг пропускаем, но команду не теряем: одна испорченная
            // строка не должна прятать от пользователя всю автоматизацию.
            .filter_map(|raw| serde_json::from_str(&raw).ok())
            .collect();

        commands.push(CommandRecord {
            id,
            name,
            description,
            trigger_kind,
            phrase,
            hotkey,
            schedule,
            app,
            enabled,
            steps,
        });
    }

    Ok(commands)
}

/// Расписание вида «ЧЧ:ММ» или «ЧЧ:ММ|1,2,3» (дни недели 1–7).
fn valid_schedule(schedule: &str) -> bool {
    let (time, days) = schedule.split_once('|').unwrap_or((schedule, ""));
    let Some((h, m)) = time.trim().split_once(':') else {
        return false;
    };
    let time_ok = h.parse::<u8>().is_ok_and(|h| h < 24) && m.len() == 2 && m.parse::<u8>().is_ok_and(|m| m < 60);
    let days_ok = days
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .all(|d| d.parse::<u8>().is_ok_and(|d| (1..=7).contains(&d)));
    time_ok && days_ok
}

/// Создаёт или обновляет команду вместе с её шагами.
#[tauri::command]
pub fn command_save(
    app: AppHandle,
    state: State<'_, AppState>,
    command: CommandRecord,
) -> Result<(), String> {
    if command.id.trim().is_empty() {
        return Err("не задан идентификатор команды".into());
    }
    if command.name.trim().is_empty() {
        return Err("у команды должно быть имя".into());
    }
    if command.trigger_kind == "phrase" && command.phrase.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("для запуска по фразе нужна сама фраза".into());
    }
    if command.trigger_kind == "hotkey" && command.hotkey.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("для запуска по сочетанию нужно само сочетание".into());
    }
    if command.trigger_kind == "schedule" && !valid_schedule(command.schedule.as_deref().unwrap_or("")) {
        return Err("для запуска по расписанию нужно время, например 09:00".into());
    }
    if command.trigger_kind == "app" && command.app.as_deref().unwrap_or("").trim().is_empty() {
        return Err("для запуска вместе с программой нужно её имя".into());
    }

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO commands (id, name, description, trigger_kind, phrase, hotkey, enabled, schedule, app)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, description = excluded.description,
                   trigger_kind = excluded.trigger_kind, phrase = excluded.phrase,
                   hotkey = excluded.hotkey, enabled = excluded.enabled,
                   schedule = excluded.schedule, app = excluded.app",
                rusqlite::params![
                    command.id,
                    command.name.trim(),
                    command.description,
                    command.trigger_kind,
                    command.phrase,
                    command.hotkey,
                    command.enabled as i64,
                    command.schedule,
                    command.app
                ],
            )?;

            // Шаги переписываются целиком: сверять их поэлементно значит
            // оставлять расхождения при каждой пропущенной ветке.
            conn.execute(
                "DELETE FROM command_nodes WHERE command_id = ?1",
                [&command.id],
            )?;
            for (position, step) in command.steps.iter().enumerate() {
                conn.execute(
                    "INSERT INTO command_nodes (id, command_id, kind, config, position)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        format!("{}-{}", command.id, position),
                        command.id,
                        step["kind"].as_str().unwrap_or("action"),
                        step.to_string(),
                        position as i64
                    ],
                )?;
            }
            Ok(())
        })
        .map_err(err)?;

    // Сочетание могло появиться, измениться или исчезнуть — перерегистрируем всё.
    crate::hotkeys::resync(&app);
    Ok(())
}

#[tauri::command]
pub fn command_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            // Шаги уходят каскадом по внешнему ключу, но полагаться на это
            // молча не стоит: `PRAGMA foreign_keys` включается кодом, а не схемой.
            conn.execute("DELETE FROM command_nodes WHERE command_id = ?1", [&id])?;
            conn.execute("DELETE FROM commands WHERE id = ?1", [&id])
        })
        .map_err(err)?;

    crate::hotkeys::resync(&app);
    Ok(())
}

/// Сочетания клавиш всех включённых команд — их регистрирует слой хоткеев.
pub fn hotkeys(state: &AppState) -> Vec<(String, String)> {
    list(state)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| c.enabled && c.trigger_kind == "hotkey")
        .filter_map(|c| c.hotkey.filter(|h| !h.trim().is_empty()).map(|h| (c.id, h)))
        .collect()
}

#[cfg(test)]
mod schedule_tests {
    #[test]
    fn schedules_are_time_and_optional_weekdays() {
        assert!(super::valid_schedule("09:00"));
        assert!(super::valid_schedule("9:30|1,2,3,4,5"));
        assert!(super::valid_schedule("23:59|6,7"));
        assert!(!super::valid_schedule("24:00"));
        assert!(!super::valid_schedule("09:0"));
        assert!(!super::valid_schedule("09:00|0,8"));
        assert!(!super::valid_schedule("утром"));
    }
}
