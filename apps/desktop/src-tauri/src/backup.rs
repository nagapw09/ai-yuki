//! Экспорт и импорт данных (`docs/GAPS.md` §11).
//!
//! # Что переносится, а что нет
//!
//! Переносится всё, что человек накопил сам: настройки, команды, память,
//! заметки, напоминания, подключённые возможности и плагины.
//!
//! **Секреты не переносятся никогда.** Ключи провайдеров, токены MCP-серверов и
//! refresh-токены календарей лежат в хранилище ОС (ТЗ §29), и вынуть их в файл
//! значило бы сделать резервную копию, которая сама по себе даёт доступ ко всем
//! сервисам человека. Копия, которую опасно потерять, — плохая копия.
//!
//! Вместо значений в файл попадает список того, что придётся ввести заново.
//! Молчать об этом нельзя: перенос, после которого половина возможностей молча
//! не работает, хуже отсутствия переноса.
//!
//! # Почему слияние, а не замена
//!
//! Импорт по умолчанию дополняет, а не стирает. Человек, переносящий данные на
//! новую машину, обычно уже успел там что-то настроить, и «восстановить из
//! копии» не должно означать «потерять сделанное сегодня». Полная замена есть
//! отдельным режимом и говорит о себе прямо.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::State;

use crate::state::AppState;

/// Версия формата. Поднимается, когда меняется состав таблиц.
const FORMAT_VERSION: u32 = 1;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Таблицы, которые входят в копию, и порядок их восстановления.
///
/// Порядок важен: `command_nodes` ссылается на `commands`, а внешние ключи
/// включены (`PRAGMA foreign_keys = ON`). Восстановление в обратном порядке
/// упало бы на первой же зависимой строке.
const TABLES: &[&str] = &[
    "settings",
    "providers",
    "permissions",
    "memories",
    "notes",
    "reminders",
    "commands",
    "command_nodes",
    "capabilities",
    "mcp_servers",
    "plugins",
    "calendar_accounts",
];

/// Столбцы, которые не покидают машину.
///
/// `embedding` — не секрет, но это десятки килобайт векторов, которые заново
/// считаются за один запрос; тащить их в файл переноса незачем.
const SKIPPED_COLUMNS: &[&str] = &["embedding"];

/// Что лежит в файле.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub version: u32,
    /// Сколько образцов голоса в копии.
    pub voices: usize,
    /// История чата: живёт в окне, поэтому окно её и отдаёт, и забирает обратно.
    pub chat: Option<Value>,
    /// Сколько строк по каждой таблице.
    pub counts: Vec<TableCount>,
    /// Что придётся ввести заново после переноса.
    pub secrets_to_reenter: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCount {
    pub table: String,
    pub rows: usize,
}

/// Режим импорта.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportMode {
    /// Дополнить: совпадающие по ключу строки заменяются, остальные остаются.
    Merge,
    /// Заменить: содержимое переносимых таблиц стирается перед восстановлением.
    Replace,
}

/// Собирает данные всех переносимых таблиц.
pub fn collect(storage: &crate::storage::Storage) -> Result<Value, String> {
    let mut tables = serde_json::Map::new();

    for table in TABLES {
        let rows = storage
            .with_conn(|conn| {
                let mut stmt = conn.prepare(&format!("SELECT * FROM {table}"))?;

                let names: Vec<String> = stmt
                    .column_names()
                    .into_iter()
                    .map(str::to_string)
                    .filter(|name| !SKIPPED_COLUMNS.contains(&name.as_str()))
                    .collect();

                let mut out = Vec::new();
                let mut query = stmt.query([])?;

                while let Some(row) = query.next()? {
                    let mut object = serde_json::Map::new();
                    for name in &names {
                        object.insert(name.clone(), to_json(row.get_ref(name.as_str())?));
                    }
                    out.push(Value::Object(object));
                }

                Ok(out)
            })
            .map_err(err)?;

        tables.insert((*table).to_string(), Value::Array(rows));
    }

    Ok(Value::Object(tables))
}

/// Значение SQLite в JSON.
///
/// BLOB не переносится: единственный такой столбец — вектор эмбеддинга, и он
/// уже отфильтрован. Всё остальное представимо без потерь.
fn to_json(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;

    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => json!(number),
        ValueRef::Text(text) => json!(String::from_utf8_lossy(text)),
        ValueRef::Blob(_) => Value::Null,
    }
}

/// Перечисляет, какие секреты придётся ввести заново.
///
/// Список строится по ссылкам в базе, а не по содержимому хранилища ОС: узнать,
/// что там лежит, можно только прочитав значения, а читать секреты ради отчёта
/// — ровно то, чего этот модуль избегает.
pub fn secrets_to_reenter(data: &Value) -> Vec<String> {
    let mut names = Vec::new();

    for (table, label) in [
        ("providers", "ключ провайдера"),
        ("mcp_servers", "доступ к серверу"),
    ] {
        for row in data[table].as_array().into_iter().flatten() {
            if row["secret_ref"].is_string() {
                let title = row["label"]
                    .as_str()
                    .or_else(|| row["id"].as_str())
                    .unwrap_or("без имени");
                names.push(format!("{label}: {title}"));
            }
        }
    }

    for row in data["calendar_accounts"].as_array().into_iter().flatten() {
        if let Some(provider) = row["provider"].as_str() {
            names.push(format!("вход в календарь: {provider}"));
        }
    }

    names
}

fn counts(data: &Value) -> Vec<TableCount> {
    TABLES
        .iter()
        .map(|table| TableCount {
            table: (*table).to_string(),
            rows: data[table].as_array().map(Vec::len).unwrap_or(0),
        })
        .collect()
}

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Самый большой образец голоса, который берём в копию: образец — это секунды
/// речи, и файл в десятки мегабайт — скорее ошибка, чем голос.
const VOICE_MAX_BYTES: u64 = 20 * 1024 * 1024;

/// Настройки, привязанные к этой машине: пути к программам и моделям. На новом
/// компьютере их нет, и перенесённые как есть они сломали бы голос.
const MACHINE_SETTINGS: &[&str] = &[
    "voice.tts.server.python",
    "voice.tts.server.script",
    "voice.tts.server.model",
    "voice.tts.samples",
];

/// Образцы голоса (`.wav` и текст к ним) из папки образцов.
fn collect_voices(state: &AppState) -> Vec<Value> {
    use base64::Engine;
    let Some(dir) = crate::avatar::setting(state, "voice.tts.samples") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("wav")))
        .filter(|p| std::fs::metadata(p).map(|m| m.len() <= VOICE_MAX_BYTES).unwrap_or(false))
        .filter_map(|p| {
            let name = p.file_name()?.to_string_lossy().into_owned();
            let audio = std::fs::read(&p).ok()?;
            let text = std::fs::read_to_string(p.with_extension("txt")).ok();
            Some(json!({
                "name": name,
                "audio": base64::engine::general_purpose::STANDARD.encode(audio),
                "text": text,
            }))
        })
        .collect()
}

/// Раскладывает образцы голоса в папку Yuki. Имена — только сами имена файлов:
/// путь из чужого файла копии не должен вывести запись за пределы папки.
fn restore_voices(app: &tauri::AppHandle, voices: &Value) -> Result<usize, String> {
    use base64::Engine;
    let Some(list) = voices.as_array() else {
        return Ok(0);
    };
    let dir = crate::voice_install::voices_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(err)?;
    let mut restored = 0;
    for voice in list {
        let Some(name) = voice["name"].as_str().and_then(|n| std::path::Path::new(n).file_name()) else {
            continue;
        };
        let Some(audio) = voice["audio"].as_str().and_then(|a| base64::engine::general_purpose::STANDARD.decode(a).ok()) else {
            continue;
        };
        let target = dir.join(name);
        std::fs::write(&target, audio).map_err(err)?;
        if let Some(text) = voice["text"].as_str() {
            std::fs::write(target.with_extension("txt"), text).map_err(err)?;
        }
        restored += 1;
    }
    Ok(restored)
}

/// Сохраняет копию в файл.
#[tauri::command]
pub fn backup_export(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    chat: Option<Value>,
) -> Result<BackupSummary, String> {
    let data = collect(&state.storage)?;
    let voices = collect_voices(&state);

    let document = json!({
        "format": "yuki-backup",
        "version": FORMAT_VERSION,
        "app_version": app.package_info().version.to_string(),
        "exported_at": now(),
        // Прямо в файле: тот, кто откроет его через год, должен понимать,
        // чего в нём нет.
        "note": "Секреты не входят в копию: они хранятся в системном хранилище ОС.",
        "tables": data,
        "voices": voices,
        "chat": chat,
    });

    std::fs::write(&path, serde_json::to_vec_pretty(&document).map_err(err)?)
        .map_err(|e| format!("{path}: {e}"))?;

    Ok(BackupSummary {
        version: FORMAT_VERSION,
        voices: document["voices"].as_array().map(Vec::len).unwrap_or(0),
        chat: None,
        counts: counts(&document["tables"]),
        secrets_to_reenter: secrets_to_reenter(&document["tables"]),
    })
}

/// Читает файл и рассказывает, что в нём, ничего не меняя.
#[tauri::command]
pub fn backup_preview(path: String) -> Result<BackupSummary, String> {
    let document = read_document(&path)?;

    Ok(BackupSummary {
        version: document["version"].as_u64().unwrap_or(0) as u32,
        voices: document["voices"].as_array().map(Vec::len).unwrap_or(0),
        chat: None,
        counts: counts(&document["tables"]),
        secrets_to_reenter: secrets_to_reenter(&document["tables"]),
    })
}

/// Восстанавливает данные из файла.
#[tauri::command]
pub fn backup_import(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
    mode: ImportMode,
) -> Result<BackupSummary, String> {
    let document = read_document(&path)?;
    let data = document["tables"].clone();

    state
        .storage
        .with_conn(|conn| {
            // Одной транзакцией: наполовину восстановленная база хуже, чем
            // невосстановленная — во второй хотя бы понятно, что произошло.
            let tx = conn.unchecked_transaction()?;

            if mode == ImportMode::Replace {
                // В обратном порядке: сначала зависимые таблицы.
                for table in TABLES.iter().rev() {
                    tx.execute(&format!("DELETE FROM {table}"), [])?;
                }
            }

            for table in TABLES {
                let Some(rows) = data[table].as_array() else {
                    continue;
                };

                for row in rows {
                    let Some(object) = row.as_object() else {
                        continue;
                    };
                    if object.is_empty() {
                        continue;
                    }

                    let columns: Vec<&String> = object.keys().collect();
                    let placeholders = (1..=columns.len())
                        .map(|i| format!("?{i}"))
                        .collect::<Vec<_>>()
                        .join(", ");

                    let sql = format!(
                        "INSERT OR REPLACE INTO {table} ({}) VALUES ({placeholders})",
                        columns
                            .iter()
                            .map(|c| format!("\"{c}\""))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );

                    let values: Vec<rusqlite::types::Value> =
                        columns.iter().map(|c| from_json(&object[*c])).collect();

                    tx.execute(&sql, rusqlite::params_from_iter(values.iter()))?;
                }
            }

            tx.commit()
        })
        .map_err(err)?;

    // Пути старой машины: сервиса голоса и модели здесь нет, пока голос не
    // установят кнопкой. Оставить их — значит голос, который молча не звучит.
    let python_here = crate::avatar::setting(&state, "voice.tts.server.python")
        .is_some_and(|p| std::path::Path::new(&p).exists());
    state
        .storage
        .with_conn(|conn| {
            for key in MACHINE_SETTINGS {
                if python_here && *key != "voice.tts.samples" {
                    continue;
                }
                conn.execute("DELETE FROM settings WHERE key = ?1", [key])?;
            }
            if !python_here {
                conn.execute(
                    "UPDATE settings SET value = 'system' WHERE key = 'voice.tts.engine'",
                    [],
                )?;
            }
            Ok(())
        })
        .map_err(err)?;

    let voices = restore_voices(&app, &document["voices"])?;
    if voices > 0 {
        let dir = crate::voice_install::voices_dir(&app)?;
        crate::avatar::set_setting(&state, "voice.tts.samples", &dir.display().to_string())?;
    }

    Ok(BackupSummary {
        version: document["version"].as_u64().unwrap_or(0) as u32,
        voices,
        chat: document.get("chat").cloned().filter(|c| !c.is_null()),
        counts: counts(&data),
        secrets_to_reenter: secrets_to_reenter(&data),
    })
}

/// Читает и проверяет файл копии.
fn read_document(path: &str) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let document: Value =
        serde_json::from_str(&text).map_err(|e| format!("это не файл копии Yuki: {e}"))?;

    if document["format"].as_str() != Some("yuki-backup") {
        return Err("это не файл копии Yuki".into());
    }

    let version = document["version"].as_u64().unwrap_or(0) as u32;
    if version > FORMAT_VERSION {
        // Файл из будущей версии может содержать таблицы, о которых мы не знаем.
        // Проглотить его молча значит потерять часть данных при восстановлении.
        return Err(format!(
            "копия сделана более новой версией Yuki (формат {version}, поддерживается {FORMAT_VERSION})"
        ));
    }

    Ok(document)
}

fn from_json(value: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as Sql;

    match value {
        Value::Null => Sql::Null,
        Value::Bool(flag) => Sql::Integer(i64::from(*flag)),
        Value::Number(number) => number
            .as_i64()
            .map(Sql::Integer)
            .or_else(|| number.as_f64().map(Sql::Real))
            .unwrap_or(Sql::Null),
        Value::String(text) => Sql::Text(text.clone()),
        // Массивы и объекты в схеме хранятся строками JSON.
        other => Sql::Text(other.to_string()),
    }
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
    use crate::storage::Storage;

    fn storage_with_data() -> Storage {
        let storage = Storage::in_memory().expect("база должна открыться");
        storage
            .with_conn(|conn| {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('ui.theme', 'light')",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO notes (id, title, body) VALUES ('n1', 'Идея', 'Текст')",
                    [],
                )?;
                conn.execute(
                    "INSERT INTO memories (id, kind, key, content, embedding)
                     VALUES ('m1', 'long_term', 'имя', 'Алексей', X'0102030405060708')",
                    [],
                )
            })
            .expect("подготовка должна пройти");
        storage
    }

    #[test]
    fn collects_every_declared_table() {
        let data = collect(&storage_with_data()).expect("сбор должен пройти");
        for table in TABLES {
            assert!(data[table].is_array(), "таблицы {table} нет в копии");
        }
    }

    #[test]
    fn keeps_the_rows_a_person_created() {
        let data = collect(&storage_with_data()).expect("сбор должен пройти");
        assert_eq!(data["notes"].as_array().expect("заметки").len(), 1);
        assert_eq!(data["notes"][0]["body"], "Текст");
    }

    #[test]
    fn leaves_embeddings_out_of_the_file() {
        // Векторы считаются заново за один запрос, а весят десятки килобайт.
        let data = collect(&storage_with_data()).expect("сбор должен пройти");
        let memory = &data["memories"][0];
        assert_eq!(memory["content"], "Алексей");
        assert!(memory.get("embedding").is_none(), "вектор попал в копию");
    }

    #[test]
    fn names_the_secrets_that_will_have_to_be_typed_again() {
        let data = json!({
            "providers": [{ "id": "openai", "label": "OpenAI", "secret_ref": "provider:openai" }],
            "mcp_servers": [{ "id": "github", "label": "GitHub", "secret_ref": "mcp:github" }],
            "calendar_accounts": [{ "provider": "google" }],
        });

        let names = secrets_to_reenter(&data);
        assert_eq!(names.len(), 3);
        assert!(names.iter().any(|n| n.contains("OpenAI")));
        assert!(names.iter().any(|n| n.contains("google")));
    }

    #[test]
    fn a_provider_without_a_key_is_not_listed_as_something_to_retype() {
        let data = json!({
            "providers": [{ "id": "ollama", "label": "Ollama", "secret_ref": null }],
            "mcp_servers": [],
            "calendar_accounts": [],
        });
        assert!(secrets_to_reenter(&data).is_empty());
    }

    #[test]
    fn dependent_tables_are_restored_after_the_ones_they_reference() {
        // Внешние ключи включены: обратный порядок упал бы на первой же строке.
        let commands = TABLES
            .iter()
            .position(|t| *t == "commands")
            .expect("commands");
        let nodes = TABLES
            .iter()
            .position(|t| *t == "command_nodes")
            .expect("command_nodes");
        assert!(commands < nodes);
    }

    #[test]
    fn refuses_a_file_that_is_not_a_backup() {
        let path = std::env::temp_dir().join("yuki-not-a-backup.json");
        std::fs::write(&path, r#"{"hello":"world"}"#).expect("файл должен записаться");

        let outcome = read_document(&path.display().to_string());
        let _ = std::fs::remove_file(&path);

        assert!(outcome.is_err());
    }

    #[test]
    fn refuses_a_backup_from_a_newer_version() {
        // Файл из будущего может содержать таблицы, о которых мы не знаем;
        // проглотить его молча значит потерять их при восстановлении.
        let path = std::env::temp_dir().join("yuki-future-backup.json");
        std::fs::write(
            &path,
            r#"{"format":"yuki-backup","version":99,"tables":{}}"#,
        )
        .expect("файл должен записаться");

        let outcome = read_document(&path.display().to_string());
        let _ = std::fs::remove_file(&path);

        let message = outcome.expect_err("должна быть ошибка");
        assert!(message.contains("более новой"), "{message}");
    }

    #[test]
    fn json_values_survive_the_trip_into_sqlite() {
        use rusqlite::types::Value as Sql;

        assert!(matches!(from_json(&Value::Null), Sql::Null));
        assert!(matches!(from_json(&json!(42)), Sql::Integer(42)));
        assert!(matches!(from_json(&json!(true)), Sql::Integer(1)));
        assert!(matches!(from_json(&json!("текст")), Sql::Text(_)));
        // Массив в схеме хранится строкой JSON — так его и записываем.
        assert!(matches!(from_json(&json!(["a", "b"])), Sql::Text(_)));
    }
}
