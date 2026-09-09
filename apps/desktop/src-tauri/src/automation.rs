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
    /// `phrase` · `hotkey` · `startup` · `manual`
    pub trigger_kind: String,
    pub phrase: Option<String>,
    pub hotkey: Option<String>,
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
    let rows: Vec<(String, String, String, String, Option<String>, Option<String>, bool)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, trigger_kind, phrase, hotkey, enabled
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
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    let mut commands = Vec::with_capacity(rows.len());
    for (id, name, description, trigger_kind, phrase, hotkey, enabled) in rows {
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
            enabled,
            steps,
        });
    }

    Ok(commands)
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
    if command.trigger_kind == "phrase"
        && command.phrase.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("для запуска по фразе нужна сама фраза".into());
    }
    if command.trigger_kind == "hotkey"
        && command.hotkey.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("для запуска по сочетанию нужно само сочетание".into());
    }

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO commands (id, name, description, trigger_kind, phrase, hotkey, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, description = excluded.description,
                   trigger_kind = excluded.trigger_kind, phrase = excluded.phrase,
                   hotkey = excluded.hotkey, enabled = excluded.enabled",
                rusqlite::params![
                    command.id,
                    command.name.trim(),
                    command.description,
                    command.trigger_kind,
                    command.phrase,
                    command.hotkey,
                    command.enabled as i64
                ],
            )?;

            // Шаги переписываются целиком: сверять их поэлементно значит
            // оставлять расхождения при каждой пропущенной ветке.
            conn.execute("DELETE FROM command_nodes WHERE command_id = ?1", [&command.id])?;
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
        .filter_map(|c| {
            c.hotkey
                .filter(|h| !h.trim().is_empty())
                .map(|h| (c.id, h))
        })
        .collect()
}
