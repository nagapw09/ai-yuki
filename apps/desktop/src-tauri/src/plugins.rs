//! Plugin System (ТЗ §20) и Self-Extension (ТЗ §18).
//!
//! Плагин Yuki — это папка с манифестом `yuki-plugin.json` и программой, которая
//! говорит по Model Context Protocol через свой стандартный ввод-вывод.
//!
//! # Почему плагин — это MCP-сервер
//!
//! ТЗ §20 требует локальный Plugin SDK, ТЗ §19 — поддержку MCP. Делать два
//! разных механизма расширения означало бы два реестра инструментов, два места
//! проверки разрешений и два способа сломаться. Плагин, говорящий по MCP,
//! получает всё уже построенное: реестр инструментов, Permission Gate, здоровье
//! возможности, отключение по требованию. Манифест добавляет то, чего у голого
//! MCP-сервера нет: имя, версию, заявленные разрешения и заявленный список
//! инструментов — то есть предмет permission review.
//!
//! # Чем плагин изолирован, а чем нет
//!
//! Изоляция здесь ровно одна и её надо называть честно: **отдельный процесс**.
//! Плагин запускается с правами пользователя и может делать всё, что может сам
//! пользователь. Валидация манифеста — это не песочница и не защита от вредоносного
//! кода; это проверка того, что установка осознанна и что заявленное совпадает с
//! действительным. Реальная граница проходит по инструментам: всё, что плагин
//! предлагает Yuki, вызывается только через Permission Gate (ТЗ §21, §22), и
//! опасное действие всё так же требует подтверждения.
//!
//! Из этого следует правило, которое обязана соблюдать и модель, и интерфейс:
//! установка плагина показывается пользователю целиком — откуда он, что запускает
//! и какие разрешения просит, — и происходит только после явного согласия.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{Manager, State};

use crate::state::AppState;

/// Имя файла манифеста.
pub const MANIFEST_FILE: &str = "yuki-plugin.json";

/// Каталог плагинов внутри данных приложения.
const PLUGINS_DIR: &str = "plugins";

/// Категории разрешений из ТЗ §21.
///
/// Список продублирован здесь намеренно: манифест приходит извне, и сверять его
/// надо с константой, а не с содержимым таблицы, которое пользователь мог
/// изменить. Тест ниже следит, чтобы список не разошёлся со схемой базы.
const PERMISSION_CATEGORIES: &[&str] = &[
    "microphone",
    "screen_recording",
    "accessibility",
    "files",
    "network",
    "shell",
    "camera",
    "notifications",
    "browser",
    "external_services",
];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// ── Манифест ────────────────────────────────────────────────────────────────────

/// Как запускается программа плагина.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRuntime {
    /// Программа: либо имя в PATH (`python`, `node`), либо путь внутри папки плагина.
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Переменные окружения процесса плагина.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// Манифест плагина.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Заявленные категории разрешений (ТЗ §21) — предмет permission review.
    #[serde(default)]
    pub permissions: Vec<String>,
    pub runtime: PluginRuntime,
    /// Инструменты, которые плагин обещает предоставить.
    #[serde(default)]
    pub tools: Vec<String>,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

/// Проверяет манифест целиком и возвращает **все** найденные несоответствия.
///
/// Именно все, а не первое: человек, который правит свой плагин, должен увидеть
/// список замечаний разом, а не открывать установку десять раз подряд.
pub fn validate(manifest: &PluginManifest) -> Vec<String> {
    let mut problems = Vec::new();

    let id = manifest.id.trim();
    if id.len() < 2 || id.len() > 64 {
        problems.push("id: от 2 до 64 символов".into());
    }
    if !id.is_empty()
        && !id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        problems.push("id: только строчная латиница, цифры, дефис и подчёркивание".into());
    }

    if manifest.name.trim().is_empty() {
        problems.push("name: пустое имя".into());
    }

    let parts: Vec<&str> = manifest.version.split('.').collect();
    if parts.len() != 3 || !parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        problems.push(format!(
            "version: ожидается вид 1.0.0, получено «{}»",
            manifest.version
        ));
    }

    for permission in &manifest.permissions {
        if !PERMISSION_CATEGORIES.contains(&permission.as_str()) {
            problems.push(format!("permissions: неизвестная категория «{permission}»"));
        }
    }

    let command = manifest.runtime.command.trim();
    if command.is_empty() {
        problems.push("runtime.command: не задана программа запуска".into());
    } else if Path::new(command).is_absolute() {
        // Абсолютный путь означает «запусти что-то вне папки плагина». Такой
        // плагин нельзя ни перенести, ни осмысленно проверить, а его манифест
        // перестаёт описывать то, что реально запустится.
        problems.push("runtime.command: абсолютный путь запрещён".into());
    } else if has_parent_segment(command) {
        problems.push("runtime.command: путь не должен выходить из папки плагина".into());
    }

    for arg in &manifest.runtime.args {
        if has_parent_segment(arg) {
            problems.push(format!("runtime.args: «{arg}» выходит из папки плагина"));
        }
    }

    problems
}

/// Есть ли в пути сегмент `..`.
///
/// Сравниваются именно сегменты, а не подстроки: имя файла `..hidden` законно,
/// а `../secrets` — нет.
fn has_parent_segment(value: &str) -> bool {
    value
        .split(['/', '\\'])
        .any(|segment| segment == "..")
}

/// Читает и разбирает манифест из папки.
pub fn read_manifest(directory: &Path) -> Result<PluginManifest, String> {
    let path = directory.join(MANIFEST_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!("не удалось прочитать {}: {e}", path.display())
    })?;
    serde_json::from_str(&text).map_err(|e| format!("{MANIFEST_FILE}: {e}"))
}

// ── Записи для интерфейса ───────────────────────────────────────────────────────

/// Результат проверки папки до установки — основа permission review (ТЗ §18).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginReview {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub tools: Vec<String>,
    /// Что именно будет запущено — показывается пользователю дословно.
    pub command_line: String,
    pub location: String,
    /// Найденные несоответствия. Непустой список означает отказ в установке.
    pub problems: Vec<String>,
}

/// Установленный плагин.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    /// `local` · `git` · `dev_folder` · `generated`
    pub origin: String,
    pub location: String,
    pub permissions: Vec<String>,
    pub tools: Vec<String>,
    pub enabled: bool,
    pub installed_at: i64,
}

fn plugins_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(err)?.join(PLUGINS_DIR);
    std::fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir)
}

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Проверяет папку и показывает, что будет установлено (ТЗ §18: permission review).
///
/// Отдельной командой, а не частью установки: пользователь должен увидеть
/// разрешения и команду запуска **до** того, как чужой код будет запущен.
#[tauri::command]
pub fn plugin_review(path: String) -> Result<PluginReview, String> {
    let directory = PathBuf::from(&path);
    let manifest = read_manifest(&directory)?;
    let problems = validate(&manifest);

    let command_line = std::iter::once(manifest.runtime.command.clone())
        .chain(manifest.runtime.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");

    Ok(PluginReview {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        permissions: manifest.permissions,
        tools: manifest.tools,
        command_line,
        location: directory.display().to_string(),
        problems,
    })
}

#[tauri::command]
pub fn plugin_list(state: State<'_, AppState>) -> Result<Vec<PluginRecord>, String> {
    let rows: Vec<(String, String, String, String, String, i64, i64)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, version, origin, location, enabled, installed_at
                 FROM plugins ORDER BY name",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    let mut records = Vec::with_capacity(rows.len());
    for (id, name, version, origin, location, enabled, installed_at) in rows {
        // Разрешения и инструменты читаются из манифеста на диске, а не из копии
        // в базе: папку могли обновить (`git pull`, правка в dev-режиме), и
        // показывать устаревшее описание чужого кода — ровно та ошибка, ради
        // которой permission review и существует.
        let manifest = read_manifest(Path::new(&location)).ok();
        records.push(PluginRecord {
            permissions: manifest
                .as_ref()
                .map(|m| m.permissions.clone())
                .unwrap_or_default(),
            tools: manifest.as_ref().map(|m| m.tools.clone()).unwrap_or_default(),
            id,
            name,
            version,
            origin,
            location,
            enabled: enabled != 0,
            installed_at,
        });
    }
    Ok(records)
}

/// Откуда берётся плагин (ТЗ §20).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallArgs {
    /// `local` — скопировать папку · `dev_folder` — работать из папки автора ·
    /// `git` — склонировать репозиторий · `generated` — созданное самой Yuki.
    pub origin: String,
    /// Путь к папке (для `local`, `dev_folder`, `generated`).
    pub path: Option<String>,
    /// Адрес репозитория (для `git`).
    pub url: Option<String>,
}

/// Устанавливает плагин: валидация → размещение → подключение (ТЗ §20).
#[tauri::command]
pub async fn plugin_install(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    args: PluginInstallArgs,
) -> Result<crate::capabilities::CapabilityRecord, String> {
    let root = plugins_root(&app)?;

    // Шаг 1. Получить папку с манифестом, ничего ещё не запуская.
    let source = match args.origin.as_str() {
        "git" => {
            let url = args.url.as_deref().unwrap_or_default();
            clone_repository(url, &root)?
        }
        "local" | "dev_folder" | "generated" => {
            let path = args
                .path
                .as_deref()
                .filter(|p| !p.trim().is_empty())
                .ok_or("не указана папка плагина")?;
            PathBuf::from(path)
        }
        other => return Err(format!("неизвестный источник плагина: {other}")),
    };

    // Шаг 2. Валидация до любого запуска — и повторно здесь, даже если интерфейс
    // уже показывал review: команда доступна и модели, и её нельзя обойти.
    let manifest = read_manifest(&source)?;
    let problems = validate(&manifest);
    if !problems.is_empty() {
        // Склонированный репозиторий за собой убираем: оставленная папка
        // выглядела бы как наполовину установленный плагин.
        if args.origin == "git" {
            let _ = std::fs::remove_dir_all(&source);
        }
        return Err(format!(
            "манифест не прошёл проверку:\n{}",
            problems.join("\n")
        ));
    }

    // Шаг 3. Разместить. Папка разработчика остаётся на месте: смысл dev-режима
    // в том, чтобы править исходники там, где они лежат.
    let location = match args.origin.as_str() {
        "dev_folder" => source.clone(),
        "git" => {
            let target = root.join(&manifest.id);
            if target != source {
                if target.exists() {
                    std::fs::remove_dir_all(&target).map_err(err)?;
                }
                std::fs::rename(&source, &target).map_err(err)?;
            }
            target
        }
        _ => {
            let target = root.join(&manifest.id);
            if target != source {
                copy_tree(&source, &target)?;
            }
            target
        }
    };

    let location_text = location.display().to_string();

    // Шаг 4. Записать плагин, сервер и возможность одной транзакцией.
    let manifest_json = serde_json::to_string(&manifest).map_err(err)?;
    let args_json = serde_json::to_string(&manifest.runtime.args).map_err(err)?;
    let env_json = serde_json::to_string(&manifest.runtime.env).map_err(err)?;
    let permissions_json = serde_json::to_string(&manifest.permissions).map_err(err)?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO plugins (id, name, version, origin, location, manifest, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, version = excluded.version,
                   origin = excluded.origin, location = excluded.location,
                   manifest = excluded.manifest, enabled = 1",
                rusqlite::params![
                    manifest.id,
                    manifest.name,
                    manifest.version,
                    args.origin,
                    location_text,
                    manifest_json
                ],
            )?;

            conn.execute(
                "INSERT INTO mcp_servers (id, label, transport, command, args, url, env, enabled)
                 VALUES (?1, ?2, 'stdio', ?3, ?4, NULL, ?5, 1)
                 ON CONFLICT(id) DO UPDATE SET
                   label = excluded.label, transport = 'stdio',
                   command = excluded.command, args = excluded.args,
                   env = excluded.env, enabled = 1",
                rusqlite::params![
                    manifest.id,
                    manifest.name,
                    manifest.runtime.command,
                    args_json,
                    env_json
                ],
            )?;

            conn.execute(
                "INSERT INTO capabilities
                   (id, name, description, version, source, manifest, enabled, health)
                 VALUES (?1, ?2, ?3, ?4, 'plugin', ?5, 1, 'unknown')
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name, description = excluded.description,
                   version = excluded.version, source = 'plugin', enabled = 1",
                rusqlite::params![
                    manifest.id,
                    manifest.name,
                    manifest.description,
                    manifest.version,
                    json!({ "permissions": manifest.permissions }).to_string()
                ],
            )?;

            // Разрешения кладём отдельным присваиванием: строка возможности могла
            // существовать раньше, и ветка UPDATE выше манифест не трогает.
            conn.execute(
                "UPDATE capabilities
                 SET manifest = json_set(manifest, '$.permissions', json(?2))
                 WHERE id = ?1",
                rusqlite::params![manifest.id, permissions_json],
            )
        })
        .map_err(err)?;

    // Шаг 5. Запустить и сверить обещанное с действительным.
    crate::capabilities::connect_server(&state, &manifest.id).await?;
    verify_declared_tools(&state, &manifest)?;

    crate::capabilities::single_capability(&state, &manifest.id)
}

/// Сверяет заявленные инструменты с теми, что сервер отдал на самом деле.
///
/// Расхождение — не отказ, а понижение здоровья: плагин работает, но описание,
/// по которому пользователь давал согласие, ему не соответствует. Молча принять
/// такое нельзя — permission review опирается ровно на это описание.
fn verify_declared_tools(state: &AppState, manifest: &PluginManifest) -> Result<(), String> {
    if manifest.tools.is_empty() {
        return Ok(());
    }

    let actual: Vec<String> = state
        .mcp
        .tools_of(&manifest.id)
        .into_iter()
        .map(|t| t.tool_name)
        .collect();

    let missing: Vec<&String> = manifest
        .tools
        .iter()
        .filter(|t| !actual.contains(t))
        .collect();
    let extra: Vec<&String> = actual
        .iter()
        .filter(|t| !manifest.tools.contains(t))
        .collect();

    if missing.is_empty() && extra.is_empty() {
        return Ok(());
    }

    let mut note = String::from("манифест разошёлся с сервером плагина:");
    if !missing.is_empty() {
        note.push_str(&format!(
            " обещаны, но отсутствуют — {};",
            join(&missing)
        ));
    }
    if !extra.is_empty() {
        note.push_str(&format!(
            " не заявлены в манифесте — {};",
            join(&extra)
        ));
    }

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE capabilities SET health = 'degraded', health_note = ?2 WHERE id = ?1",
                rusqlite::params![manifest.id, note],
            )
        })
        .map_err(err)?;

    Ok(())
}

fn join(items: &[&String]) -> String {
    items
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Удаляет плагин: соединение, записи и — для копий — файлы.
#[tauri::command]
pub fn plugin_remove(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    let row: Option<(String, String)> = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT origin, location FROM plugins WHERE id = ?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(err)?;

    let Some((origin, location)) = row else {
        return Err(format!("плагин «{id}» не установлен"));
    };

    state.mcp.disconnect(&id);

    state
        .storage
        .with_conn(|conn| {
            conn.execute("DELETE FROM plugins WHERE id = ?1", [&id])?;
            conn.execute("DELETE FROM mcp_servers WHERE id = ?1", [&id])?;
            conn.execute("DELETE FROM capabilities WHERE id = ?1", [&id])
        })
        .map_err(err)?;

    // Папку разработчика не трогаем никогда: она принадлежит человеку, а не
    // Yuki, и удалить её вместе с исходниками — потеря работы, а не уборка.
    if origin != "dev_folder" {
        let root = plugins_root(&app)?;
        let path = PathBuf::from(&location);
        if path.starts_with(&root) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    Ok(())
}

// ── Self-Extension: заготовка плагина (ТЗ §18) ──────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScaffoldArgs {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Создаёт рабочую заготовку плагина и возвращает путь к ней.
///
/// Установку не выполняет намеренно (ТЗ §18): Yuki вправе написать расширение,
/// но запустить его — отдельное решение человека, принятое после review.
#[tauri::command]
pub fn plugin_scaffold(app: tauri::AppHandle, args: ScaffoldArgs) -> Result<String, String> {
    let manifest = PluginManifest {
        id: args.id.trim().to_string(),
        name: args.name.trim().to_string(),
        version: default_version(),
        description: args.description.trim().to_string(),
        permissions: args.permissions,
        runtime: PluginRuntime {
            // На Windows команды `python3` обычно нет, на macOS — обычно нет
            // голого `python`. Заготовка должна запускаться там, где её создали.
            command: if cfg!(windows) { "python" } else { "python3" }.to_string(),
            args: vec!["server.py".to_string()],
            env: HashMap::new(),
        },
        tools: vec!["ping".to_string()],
    };

    let problems = validate(&manifest);
    if !problems.is_empty() {
        return Err(problems.join("; "));
    }

    let directory = plugins_root(&app)?.join(format!("{}-draft", manifest.id));
    std::fs::create_dir_all(&directory).map_err(err)?;

    std::fs::write(
        directory.join(MANIFEST_FILE),
        serde_json::to_string_pretty(&manifest).map_err(err)?,
    )
    .map_err(err)?;
    std::fs::write(directory.join("server.py"), SERVER_TEMPLATE.trim_start()).map_err(err)?;

    Ok(directory.display().to_string())
}

/// Заготовка MCP-сервера на Python без внешних зависимостей.
///
/// Без зависимостей намеренно: заготовка должна запуститься сразу после
/// создания, а не после `pip install` в окружение, которого может не быть.
const SERVER_TEMPLATE: &str = r#"
"""Заготовка плагина Yuki: MCP-сервер поверх stdin/stdout."""
import json
import sys

# Windows по умолчанию отдаёт консольную кодировку, и кириллица в ответе
# превращается в ошибку кодирования. Протокол требует UTF-8.
sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")

PROTOCOL_VERSION = "2025-06-18"

TOOLS = [
    {
        "name": "ping",
        "description": "Проверка связи: возвращает переданный текст.",
        "inputSchema": {
            "type": "object",
            "properties": {"text": {"type": "string"}},
            "required": ["text"],
        },
    }
]


def call_tool(name, arguments):
    """Выполняет инструмент. Добавляйте свои ветки здесь."""
    if name == "ping":
        return arguments.get("text", "")
    raise ValueError(f"неизвестный инструмент: {name}")


def handle(request):
    method = request.get("method")

    if method == "initialize":
        return {
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "yuki-plugin", "version": "0.1.0"},
        }
    if method == "tools/list":
        return {"tools": TOOLS}
    if method == "tools/call":
        params = request.get("params", {})
        text = call_tool(params.get("name"), params.get("arguments", {}))
        return {"content": [{"type": "text", "text": str(text)}]}

    raise ValueError(f"неизвестный метод: {method}")


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue

        request = json.loads(line)
        # Уведомления ответа не требуют: у них нет идентификатора.
        if "id" not in request:
            continue

        try:
            response = {"jsonrpc": "2.0", "id": request["id"], "result": handle(request)}
        except Exception as error:  # noqa: BLE001 - причину обязан увидеть пользователь
            response = {
                "jsonrpc": "2.0",
                "id": request["id"],
                "error": {"code": -32000, "message": str(error)},
            }

        sys.stdout.write(json.dumps(response, ensure_ascii=False) + "\n")
        sys.stdout.flush()


if __name__ == "__main__":
    main()
"#;

// ── Вспомогательное ─────────────────────────────────────────────────────────────

/// Клонирует репозиторий во временную папку внутри каталога плагинов.
fn clone_repository(url: &str, root: &Path) -> Result<PathBuf, String> {
    let url = url.trim();
    // Адрес, начинающийся с дефиса, был бы разобран git как ключ командной
    // строки — это подмена команды, а не адрес репозитория.
    if !(url.starts_with("https://") || url.starts_with("http://") || url.starts_with("git@")) {
        return Err("адрес репозитория должен начинаться с https://, http:// или git@".into());
    }

    let target = root.join(format!(".clone-{}", std::process::id()));
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(err)?;
    }

    let output = std::process::Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(url)
        .arg(&target)
        .output()
        .map_err(|e| format!("не удалось запустить git: {e}"))?;

    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&target);
        return Err(format!(
            "git clone не удался: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    Ok(target)
}

/// Копирует дерево файлов, пропуская служебные каталоги.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(err)?;

    for entry in std::fs::read_dir(from).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();

        // История репозитория и чужие зависимости весят больше самого плагина
        // и не нужны для запуска.
        if name_text == ".git" || name_text == "node_modules" || name_text == "__pycache__" {
            continue;
        }

        let source = entry.path();
        let destination = to.join(&name);

        if entry.file_type().map_err(err)?.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            std::fs::copy(&source, &destination).map_err(err)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "notes".into(),
            name: "Заметки".into(),
            version: "1.0.0".into(),
            description: String::new(),
            permissions: vec!["files".into()],
            runtime: PluginRuntime {
                command: "python".into(),
                args: vec!["server.py".into()],
                env: HashMap::new(),
            },
            tools: vec!["add_note".into()],
        }
    }

    #[test]
    fn accepts_a_well_formed_manifest() {
        assert!(validate(&manifest()).is_empty());
    }

    #[test]
    fn rejects_an_absolute_command() {
        let mut m = manifest();
        m.runtime.command = if cfg!(windows) {
            r"C:\Windows\System32\cmd.exe".into()
        } else {
            "/bin/sh".into()
        };
        let problems = validate(&m);
        assert!(
            problems.iter().any(|p| p.contains("абсолютный путь")),
            "{problems:?}"
        );
    }

    #[test]
    fn rejects_a_path_that_climbs_out_of_the_plugin_folder() {
        let mut m = manifest();
        m.runtime.args = vec!["../../secrets.txt".into()];
        assert!(validate(&m).iter().any(|p| p.contains("выходит из папки")));
    }

    #[test]
    fn a_filename_that_merely_starts_with_dots_is_fine() {
        // `..hidden` — законное имя файла, а не выход из каталога.
        assert!(!has_parent_segment("..hidden"));
        assert!(has_parent_segment("dir/../other"));
        assert!(has_parent_segment(r"dir\..\other"));
    }

    #[test]
    fn rejects_permissions_outside_the_spec() {
        let mut m = manifest();
        m.permissions = vec!["files".into(), "root".into()];
        let problems = validate(&m);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("root"));
    }

    #[test]
    fn reports_every_problem_at_once() {
        let mut m = manifest();
        m.id = "Плагин!".into();
        m.name = "  ".into();
        m.version = "1.0".into();
        // Три независимых замечания должны прийти вместе, а не по одному.
        assert!(validate(&m).len() >= 3);
    }

    #[test]
    fn permission_categories_match_the_database_schema() {
        // Список в коде и список в схеме — два места, и разойтись им нельзя:
        // манифест с законной категорией иначе будет отвергнут на ровном месте.
        let schema = include_str!("storage/schema.sql");
        for category in PERMISSION_CATEGORIES {
            assert!(
                schema.contains(&format!("('{category}')")),
                "категории «{category}» нет в схеме"
            );
        }
    }

    #[test]
    fn a_scaffolded_manifest_passes_its_own_validation() {
        // Заготовка, которую Yuki создаёт сама, обязана проходить ту же проверку,
        // что и чужой плагин, — иначе self-extension упирается в собственный шлагбаум.
        let scaffolded = PluginManifest {
            id: "draft".into(),
            name: "Черновик".into(),
            version: default_version(),
            description: String::new(),
            permissions: Vec::new(),
            runtime: PluginRuntime {
                command: if cfg!(windows) { "python" } else { "python3" }.to_string(),
                args: vec!["server.py".into()],
                env: HashMap::new(),
            },
            tools: vec!["ping".into()],
        };
        assert!(validate(&scaffolded).is_empty());
    }

    #[test]
    fn refuses_repository_addresses_that_could_pass_for_git_options() {
        let root = std::env::temp_dir();
        assert!(clone_repository("--upload-pack=touch /tmp/pwned", &root).is_err());
        assert!(clone_repository("file:///etc", &root).is_err());
    }
}
