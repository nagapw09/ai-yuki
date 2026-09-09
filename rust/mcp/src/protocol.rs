//! Типы Model Context Protocol (ТЗ §19).
//!
//! MCP — это JSON-RPC 2.0 поверх выбранного транспорта. Здесь только то, что
//! нужно Yuki: рукопожатие, список инструментов и их вызов. Ресурсы и промпты
//! протокол тоже описывает, но в Capability Hub они пока не участвуют, и тянуть
//! их сюда «на всякий случай» значит поддерживать код, который никто не вызывает.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Версия протокола, которую заявляет Yuki при рукопожатии.
///
/// Сервер вправе ответить другой: протокол требует, чтобы клиент принял ответ
/// сервера, если умеет с ним работать. Мы принимаем любую — расхождение
/// проявится на конкретном методе, а не на подключении, и там его видно точнее.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("не удалось запустить сервер: {0}")]
    Spawn(String),

    #[error("сервер не отвечает")]
    Timeout,

    #[error("транспорт разорван: {0}")]
    Transport(String),

    /// Сервер вернул ошибку JSON-RPC.
    #[error("сервер вернул ошибку {code}: {message}")]
    Rpc { code: i64, message: String },

    #[error("не удалось разобрать ответ: {0}")]
    Decode(String),

    #[error("неизвестный транспорт: {0}")]
    UnknownTransport(String),
}

pub type McpResult<T> = Result<T, McpError>;

impl McpError {
    /// Стоит ли повторить (ТЗ §33).
    ///
    /// Ошибка самого сервера — это ответ по существу, повтор её не изменит.
    /// Обрыв транспорта и таймаут могут быть разовыми.
    pub fn is_retryable(&self) -> bool {
        matches!(self, McpError::Timeout | McpError::Transport(_))
    }
}

/// Инструмент, объявленный MCP-сервером.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// JSON Schema аргументов.
    #[serde(default = "empty_schema")]
    pub input_schema: Value,
}

fn empty_schema() -> Value {
    json!({ "type": "object", "properties": {} })
}

/// Результат вызова инструмента.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallResult {
    /// Текстовое содержимое, склеенное из блоков ответа.
    pub text: String,
    /// Сервер сообщил, что вызов завершился ошибкой.
    pub is_error: bool,
}

/// Сведения о сервере из рукопожатия.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
    pub protocol_version: String,
}

/// Собирает тело запроса JSON-RPC.
pub fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

/// Собирает уведомление — запрос без идентификатора и без ответа.
pub fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

/// Параметры рукопожатия.
pub fn initialize_params() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        // Пока Yuki ничего не предлагает серверу со своей стороны: ни выборки,
        // ни корней рабочих каталогов. Пустой объект честнее, чем заявленная
        // возможность, на вызов которой мы ответим ошибкой.
        "capabilities": {},
        "clientInfo": { "name": "Yuki", "version": env!("CARGO_PKG_VERSION") },
    })
}

/// Разбирает ответ JSON-RPC: либо результат, либо ошибка сервера.
pub fn parse_response(value: &Value) -> McpResult<Value> {
    if let Some(error) = value.get("error") {
        return Err(McpError::Rpc {
            code: error["code"].as_i64().unwrap_or(0),
            message: error["message"]
                .as_str()
                .unwrap_or("сервер не уточнил причину")
                .to_string(),
        });
    }

    value
        .get("result")
        .cloned()
        .ok_or_else(|| McpError::Decode("в ответе нет ни result, ни error".into()))
}

/// Достаёт список инструментов из ответа `tools/list`.
pub fn parse_tools(result: &Value) -> Vec<McpTool> {
    result["tools"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|t| {
                    let name = t["name"].as_str()?.to_string();
                    Some(McpTool {
                        name,
                        description: t["description"].as_str().unwrap_or_default().to_string(),
                        // Схема может называться и inputSchema, и input_schema:
                        // сервера пишут разные люди, и ломаться из-за этого
                        // на живом сервере было бы обидно.
                        input_schema: t
                            .get("inputSchema")
                            .or_else(|| t.get("input_schema"))
                            .cloned()
                            .unwrap_or_else(empty_schema),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Склеивает содержимое ответа `tools/call` в текст.
///
/// Протокол допускает блоки нескольких типов; в текст превращается то, что
/// вообще можно прочитать. Про остальные честно сообщается их тип — модель
/// должна знать, что ответ был, но прочитать его не удалось.
pub fn parse_call_result(result: &Value) -> McpCallResult {
    let is_error = result["isError"].as_bool().unwrap_or(false);

    let mut parts = Vec::new();
    for block in result["content"].as_array().into_iter().flatten() {
        match block["type"].as_str() {
            Some("text") => {
                if let Some(text) = block["text"].as_str() {
                    parts.push(text.to_string());
                }
            }
            Some(other) => parts.push(format!("[{other}: содержимое не текстовое]")),
            None => {}
        }
    }

    // Некоторые сервера кладут полезную нагрузку в structuredContent.
    if parts.is_empty() {
        if let Some(structured) = result.get("structuredContent") {
            parts.push(structured.to_string());
        }
    }

    McpCallResult {
        text: parts.join("\n"),
        is_error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_server_error_into_a_typed_failure() {
        let response = json!({
            "jsonrpc": "2.0", "id": 1,
            "error": { "code": -32601, "message": "метод не найден" }
        });

        match parse_response(&response) {
            Err(McpError::Rpc { code, message }) => {
                assert_eq!(code, -32601);
                assert_eq!(message, "метод не найден");
            }
            other => panic!("ожидалась ошибка RPC, получено {other:?}"),
        }
    }

    #[test]
    fn accepts_both_spellings_of_the_schema_field() {
        let camel = parse_tools(&json!({
            "tools": [{ "name": "a", "inputSchema": { "type": "object" } }]
        }));
        let snake = parse_tools(&json!({
            "tools": [{ "name": "b", "input_schema": { "type": "object" } }]
        }));

        assert_eq!(camel[0].input_schema["type"], "object");
        assert_eq!(snake[0].input_schema["type"], "object");
    }

    #[test]
    fn tool_without_schema_still_loads_with_an_empty_one() {
        let tools = parse_tools(&json!({ "tools": [{ "name": "ping" }] }));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].input_schema["type"], "object");
    }

    #[test]
    fn skips_entries_without_a_name_instead_of_failing() {
        // Один битый инструмент не должен лишать пользователя всего сервера.
        let tools = parse_tools(&json!({
            "tools": [{ "description": "без имени" }, { "name": "ok" }]
        }));
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "ok");
    }

    #[test]
    fn joins_text_blocks_and_keeps_the_error_flag() {
        let result = parse_call_result(&json!({
            "content": [
                { "type": "text", "text": "первая" },
                { "type": "text", "text": "вторая" }
            ],
            "isError": true
        }));

        assert_eq!(result.text, "первая\nвторая");
        assert!(result.is_error);
    }

    #[test]
    fn reports_non_text_blocks_instead_of_dropping_them_silently() {
        let result = parse_call_result(&json!({
            "content": [{ "type": "image", "data": "..." }]
        }));
        assert!(result.text.contains("image"));
    }

    #[test]
    fn falls_back_to_structured_content_when_there_are_no_blocks() {
        let result = parse_call_result(&json!({ "structuredContent": { "count": 3 } }));
        assert!(result.text.contains("count"));
    }
}
