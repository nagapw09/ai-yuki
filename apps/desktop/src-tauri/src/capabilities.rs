//! Capability Manager и Personal Capability Hub (ТЗ §17, §18).
//!
//! Возможность — это то, что Yuki умеет: встроенный набор инструментов, MCP-сервер,
//! плагин или подключённый API. Hub — единое место, где их видно и откуда их
//! добавляют. Публичного магазина нет и не планируется (ТЗ §17): каталог заготовок
//! поставляется с приложением, а всё остальное пользователь добавляет сам или
//! просит Yuki добавить за него.
//!
//! # Главный инвариант
//!
//! Возможность считается рабочей только после того, как с ней действительно
//! удалось поговорить. Установка MCP-сервера заканчивается рукопожатием и
//! запросом списка инструментов; не вышло — `health = failed` и текст причины,
//! а не «установлено». Это тот же запрет на необоснованный успех, что и в ТЗ §5.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tauri::{Manager, State};
use yuki_mcp::{McpClient, Transport};

use crate::catalog::{self, Integration};
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

/// Возможность в том виде, в каком её показывает Hub.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    /// `builtin` · `mcp` · `plugin` · `user_script` · `api`
    pub source: String,
    pub enabled: bool,
    /// `ok` · `degraded` · `failed` · `unknown`
    pub health: String,
    pub health_note: Option<String>,
    /// Инструменты, которые возможность приносит.
    pub tools: Vec<String>,
    /// Категории разрешений, которых она требует (ТЗ §21).
    pub permissions: Vec<String>,
    pub installed_at: i64,
}

/// Инструмент MCP-сервера в виде, пригодном для реестра агента.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSpec {
    /// Идентификатор в реестре Yuki: `mcp__<сервер>__<инструмент>`.
    pub id: String,
    pub server_id: String,
    pub tool_name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Описание сервера для добавления вручную (ТЗ §19: Add Server).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub id: String,
    pub label: String,
    /// `stdio` · `http` · `sse`
    pub transport: String,
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    pub url: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Ключ; попадает в хранилище ОС, а не в базу (ТЗ §29).
    pub secret: Option<String>,
    /// Имя переменной окружения, в которую подставить ключ.
    pub secret_env: Option<String>,
}

/// Подключённые MCP-серверы.
///
/// Живут ровно столько, сколько включена возможность: соединение по stdio — это
/// запущенный процесс, и держать его после выключения значит держать чужую
/// программу за спиной у пользователя.
#[derive(Default)]
pub struct McpRegistry {
    clients: std::sync::Mutex<HashMap<String, std::sync::Arc<McpClient>>>,
}

impl McpRegistry {
    pub fn insert(&self, id: String, client: McpClient) {
        if let Ok(mut guard) = self.clients.lock() {
            guard.insert(id, std::sync::Arc::new(client));
        }
    }

    pub fn disconnect(&self, id: &str) {
        if let Ok(mut guard) = self.clients.lock() {
            // Клиент закрывает процесс в Drop, поэтому достаточно выбросить его
            // из карты.
            guard.remove(id);
        }
    }

    pub fn client(&self, id: &str) -> Option<std::sync::Arc<McpClient>> {
        self.clients.lock().ok()?.get(id).cloned()
    }

    pub fn tools_of(&self, id: &str) -> Vec<McpToolSpec> {
        let Ok(guard) = self.clients.lock() else {
            return Vec::new();
        };
        guard
            .get(id)
            .map(|client| specs_for(id, client))
            .unwrap_or_default()
    }

    pub fn all_tools(&self) -> Vec<McpToolSpec> {
        let Ok(guard) = self.clients.lock() else {
            return Vec::new();
        };
        guard
            .iter()
            .flat_map(|(id, client)| specs_for(id, client))
            .collect()
    }
}

/// Имя инструмента в реестре Yuki.
///
/// Префикс обязателен: два сервера легко объявляют инструмент `search`, и без
/// разделения второй молча затёр бы первый в реестре.
fn qualified_id(server_id: &str, tool: &str) -> String {
    format!("mcp__{server_id}__{tool}")
}

/// Разбирает квалифицированное имя обратно на сервер и инструмент.
pub fn split_qualified(id: &str) -> Option<(&str, &str)> {
    id.strip_prefix("mcp__")?.split_once("__")
}

fn specs_for(server_id: &str, client: &McpClient) -> Vec<McpToolSpec> {
    client
        .tools()
        .iter()
        .map(|tool| McpToolSpec {
            id: qualified_id(server_id, &tool.name),
            server_id: server_id.to_string(),
            tool_name: tool.name.clone(),
            description: tool.description.clone(),
            input_schema: tool.input_schema.clone(),
        })
        .collect()
}

// ── Каталог (ТЗ §17) ────────────────────────────────────────────────────────────

/// Доступные интеграции; `query` фильтрует по свободному запросу.
#[tauri::command]
pub fn integrations_list(query: Option<String>) -> Vec<&'static Integration> {
    catalog::search(query.as_deref().unwrap_or_default())
}

// ── Возможности ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn capability_list(state: State<'_, AppState>) -> Result<Vec<CapabilityRecord>, String> {
    capability_list_inner(&state)
}

fn string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Включает или выключает возможность.
///
/// Выключение MCP-сервера сразу закрывает соединение: оставленный процесс
/// продолжал бы жить и держать ресурсы, а его инструменты — числиться в реестре.
#[tauri::command]
pub async fn capability_set_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE capabilities SET enabled = ?2 WHERE id = ?1",
                rusqlite::params![id, enabled as i64],
            )?;
            conn.execute(
                "UPDATE mcp_servers SET enabled = ?2 WHERE id = ?1",
                rusqlite::params![id, enabled as i64],
            )
        })
        .map_err(err)?;

    if enabled {
        connect_server(&state, &id).await?;
    } else {
        state.mcp.disconnect(&id);
    }
    Ok(())
}

/// Удаляет возможность вместе с сервером и ключом.
#[tauri::command]
pub fn capability_remove(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.mcp.disconnect(&id);

    // Ключ убираем из хранилища ОС: удалённая возможность не должна оставлять
    // за собой доступ к чужому сервису.
    let _ = crate::secrets::delete(&crate::secrets::mcp_ref(&id));

    state
        .storage
        .with_conn(|conn| {
            conn.execute("DELETE FROM mcp_servers WHERE id = ?1", [&id])?;
            conn.execute("DELETE FROM capabilities WHERE id = ?1", [&id])
        })
        .map(|_| ())
        .map_err(err)
}

// ── MCP (ТЗ §19) ────────────────────────────────────────────────────────────────

/// Добавляет сервер и сразу проверяет его.
///
/// Проверка входит в добавление намеренно: сервер, который не поднялся, — это не
/// «добавленная возможность, которую надо потом протестировать», а неработающая
/// настройка, и пользователь должен узнать об этом сейчас.
#[tauri::command]
pub async fn mcp_add(
    state: State<'_, AppState>,
    server: McpServerInput,
) -> Result<CapabilityRecord, String> {
    if server.id.trim().is_empty() {
        return Err("не задан идентификатор сервера".into());
    }

    let secret_ref = match server.secret.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(secret) => {
            let reference = crate::secrets::mcp_ref(&server.id);
            crate::secrets::set(&reference, secret.trim()).map_err(err)?;
            Some(reference)
        }
        None => None,
    };

    let env = serde_json::to_string(&server.env).map_err(err)?;
    let args = serde_json::to_string(&server.args).map_err(err)?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO mcp_servers (id, label, transport, command, args, url, env, secret_ref, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)
                 ON CONFLICT(id) DO UPDATE SET
                   label = excluded.label, transport = excluded.transport,
                   command = excluded.command, args = excluded.args, url = excluded.url,
                   env = excluded.env, secret_ref = excluded.secret_ref, enabled = 1",
                rusqlite::params![
                    server.id,
                    server.label,
                    server.transport,
                    server.command,
                    args,
                    server.url,
                    env,
                    secret_ref
                ],
            )?;

            // Имя переменной окружения для ключа храним рядом с сервером:
            // без него неоткуда узнать, куда подставлять секрет при запуске.
            if let Some(name) = &server.secret_env {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    rusqlite::params![format!("mcp.{}.secret_env", server.id), name],
                )?;
            }

            conn.execute(
                "INSERT INTO capabilities (id, name, description, source, manifest, enabled, health)
                 VALUES (?1, ?2, '', 'mcp', ?3, 1, 'unknown')
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, enabled = 1",
                rusqlite::params![server.id, server.label, json!({}).to_string()],
            )
        })
        .map_err(err)?;

    connect_server(&state, &server.id).await?;
    single_capability(&state, &server.id)
}

/// Устанавливает интеграцию из каталога (ТЗ §17).
#[tauri::command]
pub async fn integration_install(
    state: State<'_, AppState>,
    id: String,
    secret: Option<String>,
) -> Result<CapabilityRecord, String> {
    let integration = catalog::find(&id)
        .ok_or_else(|| format!("в каталоге нет интеграции «{id}»"))?;

    if integration.secret_env.is_some() && secret.as_deref().unwrap_or("").trim().is_empty() {
        return Err(format!(
            "для этой интеграции нужен ключ: {}",
            integration.secret_hint.unwrap_or("см. документацию сервиса")
        ));
    }

    // Разрешения интеграции запоминаем до установки: манифест, который
    // соберётся после подключения, знает про инструменты, но не про категории.
    let permissions: Vec<String> = integration.permissions.iter().map(|p| p.to_string()).collect();

    let input = McpServerInput {
        id: integration.id.to_string(),
        label: integration.label.to_string(),
        transport: integration.transport.to_string(),
        command: Some(integration.command.to_string()),
        args: integration.args.iter().map(|a| a.to_string()).collect(),
        url: None,
        env: HashMap::new(),
        secret,
        secret_env: integration.secret_env.map(str::to_string),
    };

    let record = mcp_add(state.clone(), input).await?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE capabilities
                 SET description = ?2,
                     manifest = json_set(manifest, '$.permissions', json(?3))
                 WHERE id = ?1",
                rusqlite::params![
                    record.id,
                    integration.description,
                    serde_json::to_string(&permissions).unwrap_or_else(|_| "[]".into())
                ],
            )
        })
        .map_err(err)?;

    single_capability(&state, &record.id)
}

/// Проверяет подключение и обновляет список инструментов (ТЗ §19: Test Connection).
#[tauri::command]
pub async fn mcp_test(state: State<'_, AppState>, id: String) -> Result<Vec<String>, String> {
    connect_server(&state, &id).await?;
    Ok(state
        .mcp
        .tools_of(&id)
        .into_iter()
        .map(|t| t.tool_name)
        .collect())
}

/// Инструменты всех подключённых серверов — их реестр агента добавляет к своим.
#[tauri::command]
pub fn mcp_tools(state: State<'_, AppState>) -> Vec<McpToolSpec> {
    state.mcp.all_tools()
}

/// Вызывает инструмент MCP-сервера.
#[tauri::command]
pub async fn mcp_call(
    state: State<'_, AppState>,
    server_id: String,
    tool: String,
    arguments: serde_json::Value,
) -> Result<String, String> {
    let client = state
        .mcp
        .client(&server_id)
        .ok_or_else(|| format!("сервер «{server_id}» не подключён"))?;

    let result = client.call_tool(&tool, arguments).await.map_err(err)?;

    // Ошибку сервера возвращаем как ошибку, а не как текст результата: иначе
    // модель примет сообщение о сбое за успешный ответ (ТЗ §5).
    if result.is_error {
        return Err(if result.text.is_empty() {
            "сервер сообщил об ошибке без подробностей".to_string()
        } else {
            result.text
        });
    }
    Ok(result.text)
}

/// Подключается к серверу и записывает результат в здоровье возможности.
async fn connect_server(state: &AppState, id: &str) -> Result<(), String> {
    let row: (String, Option<String>, String, Option<String>, String, Option<String>) = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT transport, command, args, url, env, secret_ref
                 FROM mcp_servers WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
        })
        .map_err(|_| format!("сервер «{id}» не найден"))?;

    let (transport, command, args, url, env, secret_ref) = row;

    let mut env: HashMap<String, String> = serde_json::from_str(&env).unwrap_or_default();
    let args: Vec<String> = serde_json::from_str(&args).unwrap_or_default();

    // Ключ подставляется в окружение процесса в момент запуска и нигде не
    // сохраняется: ни в базе, ни в манифесте (ТЗ §29).
    if let Some(reference) = &secret_ref {
        if let Ok(Some(secret)) = crate::secrets::get(reference) {
            let name = state
                .storage
                .with_conn(|conn| {
                    conn.query_row(
                        "SELECT value FROM settings WHERE key = ?1",
                        [format!("mcp.{id}.secret_env")],
                        |r| r.get::<_, String>(0),
                    )
                })
                .unwrap_or_else(|_| "API_KEY".to_string());
            env.insert(name, secret);
        }
    }

    let authorization = secret_ref
        .as_deref()
        .and_then(|r| crate::secrets::get(r).ok().flatten())
        .map(|s| format!("Bearer {s}"));

    let descriptor = Transport::from_parts(
        &transport,
        command,
        args,
        env,
        url,
        authorization,
    )
    .map_err(err)?;

    let outcome = McpClient::connect(descriptor, state.http.clone()).await;

    match outcome {
        Ok(client) => {
            let tools: Vec<String> = client.tools().iter().map(|t| t.name.clone()).collect();
            let info = client.info().clone();
            state.mcp.insert(id.to_string(), client);

            let manifest = json!({
                "tools": tools,
                "server": { "name": info.name, "version": info.version,
                            "protocol": info.protocol_version },
            });

            state
                .storage
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE capabilities SET health = 'ok', health_note = NULL,
                             manifest = ?2, version = ?3 WHERE id = ?1",
                        rusqlite::params![id, manifest.to_string(), info.version],
                    )?;
                    conn.execute(
                        "UPDATE mcp_servers SET last_status = 'ok', last_checked_at = ?2
                         WHERE id = ?1",
                        rusqlite::params![id, now()],
                    )
                })
                .map_err(err)?;
            Ok(())
        }
        Err(error) => {
            let note = error.to_string();
            state.mcp.disconnect(id);
            state
                .storage
                .with_conn(|conn| {
                    conn.execute(
                        "UPDATE capabilities SET health = 'failed', health_note = ?2 WHERE id = ?1",
                        rusqlite::params![id, note],
                    )?;
                    conn.execute(
                        "UPDATE mcp_servers SET last_status = 'failed', last_checked_at = ?2
                         WHERE id = ?1",
                        rusqlite::params![id, now()],
                    )
                })
                .map_err(err)?;
            Err(error.to_string())
        }
    }
}

fn single_capability(state: &AppState, id: &str) -> Result<CapabilityRecord, String> {
    capability_list_inner(state)?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("возможность «{id}» не найдена"))
}

fn capability_list_inner(state: &AppState) -> Result<Vec<CapabilityRecord>, String> {
    // Отдельная функция, чтобы её могли звать и команда, и внутренний код:
    // `State` внутрь не пробросить.
    let rows: Vec<(String, String, String, String, String, bool, String, Option<String>, String, i64)> =
        state
            .storage
            .with_conn(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id, name, description, version, source, enabled, health,
                            health_note, manifest, installed_at
                     FROM capabilities ORDER BY source, name",
                )?;
                let rows = stmt.query_map([], |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get::<_, i64>(5)? != 0,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                        r.get(9)?,
                    ))
                })?;
                rows.collect()
            })
            .map_err(err)?;

    Ok(rows
        .into_iter()
        .map(
            |(id, name, description, version, source, enabled, health, health_note, manifest, installed_at)| {
                let manifest: serde_json::Value =
                    serde_json::from_str(&manifest).unwrap_or_else(|_| json!({}));
                CapabilityRecord {
                    id,
                    name,
                    description,
                    version,
                    source,
                    enabled,
                    health,
                    health_note,
                    tools: string_list(&manifest["tools"]),
                    permissions: string_list(&manifest["permissions"]),
                    installed_at,
                }
            },
        )
        .collect())
}

/// Подключает все включённые серверы при старте.
///
/// Сбой одного сервера не должен мешать остальным: неподнявшийся помечается
/// сломанным и виден в Hub, а Yuki продолжает работать с тем, что поднялось.
pub fn connect_enabled(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };

        let ids: Vec<String> = state
            .storage
            .with_conn(|conn| {
                let mut stmt =
                    conn.prepare("SELECT id FROM mcp_servers WHERE enabled = 1")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect()
            })
            .unwrap_or_default();

        for id in ids {
            if let Err(error) = connect_server(&state, &id).await {
                tracing::warn!(server = %id, %error, "MCP-сервер не подключился");
            }
        }
    });
}
