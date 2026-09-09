//! Команды AI Provider Layer (ТЗ §4) и потоковый обмен с моделью.
//!
//! Ключи не пересекают границу процесса: они читаются из хранилища ОС прямо
//! перед запросом и живут только внутри этой функции (ТЗ §29). Команды
//! «прочитать ключ» здесь нет и не будет.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use yuki_ai::{
    ChatRequest, ChatResponse, Message, ProviderConfig, ProviderKind, StreamSink, ToolSpec,
    YUKI_IDENTITY,
};

use crate::state::AppState;

/// Событие с очередным куском текста ответа.
const EVENT_DELTA: &str = "yuki://chat-delta";
/// Событие о начале вызова инструмента — из него UI строит статус (ТЗ §15).
const EVENT_TOOL: &str = "yuki://chat-tool";

/// Подключение к провайдеру в том виде, в каком его показывают настройки.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRecord {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub base_url: String,
    pub default_model: String,
    pub is_default: bool,
    pub enabled: bool,
    /// Нужен ли ключ вообще: у локальных серверов его нет.
    pub requires_key: bool,
    /// Задан ли ключ. Само значение наружу не отдаётся никогда.
    pub has_key: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatDelta<'a> {
    request_id: &'a str,
    text: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatToolEvent<'a> {
    request_id: &'a str,
    tool: &'a str,
}

/// Отправляет поток модели во фронтенд.
struct AppSink {
    app: AppHandle,
    request_id: String,
}

impl StreamSink for AppSink {
    fn text_delta(&self, delta: &str) {
        // Сбой доставки события не должен ронять сам запрос: ответ всё равно
        // вернётся целиком, пользователь потеряет только эффект печати.
        let _ = self.app.emit(
            EVENT_DELTA,
            ChatDelta {
                request_id: &self.request_id,
                text: delta,
            },
        );
    }

    fn tool_use_started(&self, name: &str) {
        let _ = self.app.emit(
            EVENT_TOOL,
            ChatToolEvent {
                request_id: &self.request_id,
                tool: name,
            },
        );
    }
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Читает строку провайдера и собирает конфигурацию вместе с ключом.
///
/// `id = None` означает «провайдер по умолчанию»: именно так ходит агентный цикл,
/// которому незачем знать, кого выбрал пользователь.
fn resolve(state: &AppState, id: Option<&str>) -> Result<(ProviderConfig, String), String> {
    let row: (String, String, String, String, Option<String>) = state
        .storage
        .with_conn(|conn| match id {
            Some(id) => conn.query_row(
                "SELECT id, kind, base_url, default_model, secret_ref FROM providers WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            ),
            None => conn.query_row(
                "SELECT id, kind, base_url, default_model, secret_ref
                 FROM providers WHERE is_default = 1 AND enabled = 1 LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            ),
        })
        .map_err(|e| match e {
            crate::storage::StorageError::Sqlite(rusqlite::Error::QueryReturnedNoRows) => {
                "провайдер не настроен: откройте Настройки и добавьте ключ".to_string()
            }
            other => err(other),
        })?;

    let (_, raw_kind, base_url, model, secret_ref) = row;

    let kind = ProviderKind::from_str(&raw_kind)
        .ok_or_else(|| format!("неизвестный тип провайдера: {raw_kind}"))?;

    let api_key = secret_ref
        .as_deref()
        .and_then(|r| crate::secrets::get(r).ok().flatten());

    Ok((
        ProviderConfig {
            kind,
            requires_key: yuki_ai::requires_key(&raw_kind),
            base_url: if base_url.trim().is_empty() {
                kind.default_base_url().to_string()
            } else {
                base_url
            },
            api_key,
        },
        model,
    ))
}

// ── Настройки провайдеров (ТЗ §4, §17) ──────────────────────────────────────────

#[tauri::command]
pub fn provider_list(state: State<'_, AppState>) -> Result<Vec<ProviderRecord>, String> {
    let rows: Vec<(String, String, String, String, String, bool, bool, Option<String>)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, label, base_url, default_model, is_default, enabled, secret_ref
                 FROM providers ORDER BY label",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, i64>(5)? != 0,
                    r.get::<_, i64>(6)? != 0,
                    r.get(7)?,
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    Ok(rows
        .into_iter()
        .map(
            |(id, kind, label, base_url, default_model, is_default, enabled, secret_ref)| {
                ProviderRecord {
                    requires_key: yuki_ai::requires_key(&kind),
                    has_key: secret_ref.as_deref().is_some_and(crate::secrets::exists),
                    id,
                    kind,
                    label,
                    base_url,
                    default_model,
                    is_default,
                    enabled,
                }
            },
        )
        .collect())
}

#[tauri::command]
pub fn provider_save(
    state: State<'_, AppState>,
    id: String,
    base_url: Option<String>,
    default_model: Option<String>,
    enabled: Option<bool>,
) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            // COALESCE: не переданное поле остаётся прежним, а не обнуляется.
            conn.execute(
                "UPDATE providers SET
                   base_url      = COALESCE(?2, base_url),
                   default_model = COALESCE(?3, default_model),
                   enabled       = COALESCE(?4, enabled)
                 WHERE id = ?1",
                rusqlite::params![id, base_url, default_model, enabled.map(i64::from)],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Записывает ключ в хранилище ОС и включает провайдера.
#[tauri::command]
pub fn provider_set_key(
    state: State<'_, AppState>,
    id: String,
    key: String,
) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("ключ не может быть пустым".into());
    }

    let secret_ref = crate::secrets::provider_ref(&id);
    crate::secrets::set(&secret_ref, key.trim()).map_err(err)?;

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "UPDATE providers SET secret_ref = ?2, enabled = 1 WHERE id = ?1",
                rusqlite::params![id, secret_ref],
            )
        })
        .map(|_| ())
        .map_err(err)
}

#[tauri::command]
pub fn provider_clear_key(state: State<'_, AppState>, id: String) -> Result<(), String> {
    crate::secrets::delete(&crate::secrets::provider_ref(&id)).map_err(err)?;
    state
        .storage
        .with_conn(|conn| {
            // Провайдер без ключа не может быть ни включённым, ни выбранным
            // по умолчанию — иначе следующий запрос упадёт уже в сети.
            conn.execute(
                "UPDATE providers SET enabled = 0, is_default = 0 WHERE id = ?1",
                [&id],
            )
        })
        .map(|_| ())
        .map_err(err)
}

#[tauri::command]
pub fn provider_set_default(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute("UPDATE providers SET is_default = 0", [])?;
            conn.execute(
                "UPDATE providers SET is_default = 1, enabled = 1 WHERE id = ?1",
                [&id],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Test Connection: проверяет ключ и возвращает список моделей (ТЗ §17, §19).
#[tauri::command]
pub async fn provider_test(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<String>, String> {
    let (config, _) = resolve(&state, Some(&id))?;
    let provider = yuki_ai::build(config, state.http.clone());
    provider.list_models().await.map_err(err)
}

// ── Обмен с моделью (ТЗ §5) ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatArgs {
    /// Идентификатор запроса: по нему фронтенд связывает события с вызовом.
    pub request_id: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    /// Провайдер; `None` — выбранный по умолчанию.
    pub provider_id: Option<String>,
    /// Модель; `None` — заданная у провайдера.
    pub model: Option<String>,
    /// Дополнение к системной инструкции: роль, память, контекст (ТЗ §9, §24).
    pub system_extra: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Один обмен с моделью.
///
/// Цикл агента живёт в TypeScript и вызывает эту команду по разу на шаг: сюда
/// приходит вся история, отсюда уходит один ответ. Rust не хранит состояние
/// диалога — иначе пришлось бы дублировать его в двух местах и синхронизировать.
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    state: State<'_, AppState>,
    args: ChatArgs,
) -> Result<ChatResponse, String> {
    let (config, default_model) = resolve(&state, args.provider_id.as_deref())?;

    let model = args
        .model
        .filter(|m| !m.trim().is_empty())
        .unwrap_or(default_model);
    if model.trim().is_empty() {
        return Err("не выбрана модель: укажите её в настройках провайдера".into());
    }

    // Идентичность Yuki (ТЗ §44) всегда идёт первой и не может быть вытеснена
    // пользовательской настройкой роли — та лишь дописывается следом.
    let system = match args.system_extra.as_deref().map(str::trim) {
        Some(extra) if !extra.is_empty() => format!("{YUKI_IDENTITY}\n\n{extra}"),
        _ => YUKI_IDENTITY.to_string(),
    };

    let request = ChatRequest {
        model,
        system: Some(system),
        messages: args.messages,
        tools: args.tools,
        max_tokens: args.max_tokens,
        temperature: args.temperature,
    };

    let sink = AppSink {
        app: app.clone(),
        request_id: args.request_id,
    };

    let provider = yuki_ai::build(config, state.http.clone());
    provider.chat(&request, &sink).await.map_err(err)
}
