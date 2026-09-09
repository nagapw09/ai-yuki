//! Провайдер Anthropic — Messages API (ТЗ §4).

use async_trait::async_trait;
use serde_json::{json, Map, Value};

use crate::provider::{Provider, ProviderConfig};
use crate::sse::{read_events, Flow};
use crate::types::{
    AiError, AiResult, ChatRequest, ChatResponse, ContentBlock, Message, Role, StopReason,
    StreamSink, ToolSpec, Usage,
};

/// Версия API. Anthropic требует её в каждом запросе.
const API_VERSION: &str = "2023-06-01";

/// Потолок вывода по умолчанию.
///
/// Занижать нельзя: упёршийся в лимит ответ обрывается на полуслове, и агентный
/// цикл получает мусор вместо плана. Стрим снимает вопрос HTTP-таймаутов.
const DEFAULT_MAX_TOKENS: u32 = 16_000;

pub struct AnthropicProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl AnthropicProvider {
    pub fn new(config: ProviderConfig, http: reqwest::Client) -> Self {
        Self { config, http }
    }

    fn api_key(&self) -> AiResult<&str> {
        self.config
            .api_key
            .as_deref()
            .filter(|k| !k.trim().is_empty())
            .ok_or(AiError::MissingApiKey)
    }
}

/// Перевод блока в диалект Anthropic.
///
/// Своя сериализация вместо `#[serde]` на общем типе: наружу, в TypeScript, поля
/// уходят в camelCase, а Anthropic ждёт snake_case. Держать оба варианта на одном
/// типе нельзя, а расхождение проявилось бы только в рантайме.
fn block_to_wire(block: &ContentBlock) -> Value {
    match block {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::ToolUse { id, name, input } => {
            json!({ "type": "tool_use", "id": id, "name": name, "input": input })
        }
        ContentBlock::ToolResult {
            tool_use_id,
            content,
            is_error,
        } => json!({
            "type": "tool_result",
            "tool_use_id": tool_use_id,
            "content": content,
            "is_error": is_error,
        }),
        ContentBlock::Thinking { text, signature } => {
            let mut obj = Map::new();
            obj.insert("type".into(), json!("thinking"));
            obj.insert("thinking".into(), json!(text));
            // Подпись возвращаем ровно такой, какой получили: она подтверждает
            // провайдеру, что блок не подменён.
            if let Some(sig) = signature {
                obj.insert("signature".into(), json!(sig));
            }
            Value::Object(obj)
        }
    }
}

fn message_to_wire(message: &Message) -> Value {
    json!({
        "role": match message.role { Role::User => "user", Role::Assistant => "assistant" },
        "content": message.content.iter().map(block_to_wire).collect::<Vec<_>>(),
    })
}

fn tool_to_wire(tool: &ToolSpec) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

fn stop_reason_from_wire(value: Option<&str>) -> StopReason {
    match value {
        Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        _ => StopReason::Other,
    }
}

/// Блок, который собирается по кускам во время стрима.
enum Building {
    Text(String),
    /// Аргументы инструмента приходят как поток фрагментов JSON, поэтому текст
    /// копится целиком и разбирается один раз в конце.
    ToolUse {
        id: String,
        name: String,
        json_buf: String,
    },
    Thinking {
        text: String,
        signature: Option<String>,
    },
    /// Блок типа, которого мы не знаем: пропускаем, но не роняем разбор.
    Ignored,
}

impl Building {
    fn finish(self) -> Option<ContentBlock> {
        match self {
            Building::Text(text) if text.is_empty() => None,
            Building::Text(text) => Some(ContentBlock::Text { text }),
            Building::ToolUse { id, name, json_buf } => {
                // Пустой буфер означает инструмент без аргументов — это валидно.
                let input = if json_buf.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&json_buf).unwrap_or_else(|_| json!({}))
                };
                Some(ContentBlock::ToolUse { id, name, input })
            }
            Building::Thinking { text, signature } => {
                Some(ContentBlock::Thinking { text, signature })
            }
            Building::Ignored => None,
        }
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn chat(&self, request: &ChatRequest, sink: &dyn StreamSink) -> AiResult<ChatResponse> {
        let key = self.api_key()?;

        let mut body = json!({
            "model": request.model,
            "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            "stream": true,
            "messages": request.messages.iter().map(message_to_wire).collect::<Vec<_>>(),
        });

        if let Some(system) = &request.system {
            body["system"] = json!(system);
        }
        if !request.tools.is_empty() {
            body["tools"] = json!(request.tools.iter().map(tool_to_wire).collect::<Vec<_>>());
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }

        let response = self
            .http
            .post(format!("{}/v1/messages", self.config.base_url))
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        let response = ensure_ok(response).await?;

        let mut blocks: Vec<Building> = Vec::new();
        let mut usage = Usage::default();
        let mut stop_reason = StopReason::EndTurn;
        let mut model = request.model.clone();

        read_events(response, |data| {
            let event: Value = serde_json::from_str(data)
                .map_err(|e| AiError::Decode(format!("{e}: {data}")))?;

            match event.get("type").and_then(Value::as_str) {
                Some("message_start") => {
                    let message = &event["message"];
                    if let Some(m) = message.get("model").and_then(Value::as_str) {
                        model = m.to_string();
                    }
                    usage.input_tokens = message["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
                }

                Some("content_block_start") => {
                    let index = event["index"].as_u64().unwrap_or(0) as usize;
                    let block = &event["content_block"];
                    let building = match block.get("type").and_then(Value::as_str) {
                        Some("text") => Building::Text(
                            block["text"].as_str().unwrap_or_default().to_string(),
                        ),
                        Some("tool_use") => {
                            let name = block["name"].as_str().unwrap_or_default().to_string();
                            // ТЗ §15: пользователю показываем безопасный статус,
                            // а не то, какие аргументы модель собирается передать.
                            sink.tool_use_started(&name);
                            Building::ToolUse {
                                id: block["id"].as_str().unwrap_or_default().to_string(),
                                name,
                                json_buf: String::new(),
                            }
                        }
                        Some("thinking") => Building::Thinking {
                            text: block["thinking"].as_str().unwrap_or_default().to_string(),
                            signature: block
                                .get("signature")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                        },
                        _ => Building::Ignored,
                    };
                    // Индексы приходят по порядку, но полагаться на это не стоит.
                    while blocks.len() <= index {
                        blocks.push(Building::Ignored);
                    }
                    blocks[index] = building;
                }

                Some("content_block_delta") => {
                    let index = event["index"].as_u64().unwrap_or(0) as usize;
                    let delta = &event["delta"];
                    let Some(slot) = blocks.get_mut(index) else {
                        return Ok(Flow::Continue);
                    };

                    match (delta.get("type").and_then(Value::as_str), slot) {
                        (Some("text_delta"), Building::Text(buf)) => {
                            let piece = delta["text"].as_str().unwrap_or_default();
                            buf.push_str(piece);
                            sink.text_delta(piece);
                        }
                        (Some("input_json_delta"), Building::ToolUse { json_buf, .. }) => {
                            json_buf.push_str(delta["partial_json"].as_str().unwrap_or_default());
                        }
                        (Some("thinking_delta"), Building::Thinking { text, .. }) => {
                            text.push_str(delta["thinking"].as_str().unwrap_or_default());
                        }
                        (Some("signature_delta"), Building::Thinking { signature, .. }) => {
                            let piece = delta["signature"].as_str().unwrap_or_default();
                            signature.get_or_insert_with(String::new).push_str(piece);
                        }
                        _ => {}
                    }
                }

                Some("message_delta") => {
                    stop_reason =
                        stop_reason_from_wire(event["delta"]["stop_reason"].as_str());
                    if let Some(out) = event["usage"]["output_tokens"].as_u64() {
                        usage.output_tokens = out as u32;
                    }
                }

                Some("message_stop") => return Ok(Flow::Stop),

                Some("error") => {
                    let message = event["error"]["message"]
                        .as_str()
                        .unwrap_or("провайдер не уточнил причину");
                    return Err(AiError::Api {
                        status: 0,
                        message: message.to_string(),
                    });
                }

                _ => {}
            }

            Ok(Flow::Continue)
        })
        .await?;

        Ok(ChatResponse {
            content: blocks.into_iter().filter_map(Building::finish).collect(),
            stop_reason,
            usage,
            model,
        })
    }

    async fn list_models(&self) -> AiResult<Vec<String>> {
        let key = self.api_key()?;

        let response = self
            .http
            .get(format!("{}/v1/models", self.config.base_url))
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        let body: Value = ensure_ok(response)
            .await?
            .json()
            .await
            .map_err(|e| AiError::Decode(e.to_string()))?;

        Ok(body["data"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|m| m["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }
}

/// Превращает HTTP-ошибку в [`AiError::Api`] с текстом от провайдера.
///
/// Без этого пользователь видит «сбой запроса» вместо «неверный ключ» — а ТЗ §33
/// требует сообщать реальную причину.
pub(crate) async fn ensure_ok(response: reqwest::Response) -> AiResult<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }

    let text = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            if text.is_empty() {
                "провайдер не вернул подробностей".to_string()
            } else {
                text
            }
        });

    Err(AiError::Api {
        status: status.as_u16(),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_stop_reasons_and_keeps_unknown_separate() {
        assert_eq!(stop_reason_from_wire(Some("end_turn")), StopReason::EndTurn);
        assert_eq!(stop_reason_from_wire(Some("tool_use")), StopReason::ToolUse);
        assert_eq!(
            stop_reason_from_wire(Some("max_tokens")),
            StopReason::MaxTokens
        );
        assert_eq!(stop_reason_from_wire(Some("refusal")), StopReason::Other);
        assert_eq!(stop_reason_from_wire(None), StopReason::Other);
    }

    #[test]
    fn serialises_tool_result_in_snake_case_for_the_wire() {
        let wire = block_to_wire(&ContentBlock::ToolResult {
            tool_use_id: "toolu_1".into(),
            content: "готово".into(),
            is_error: false,
        });
        assert_eq!(wire["type"], "tool_result");
        assert_eq!(wire["tool_use_id"], "toolu_1");
        assert_eq!(wire["is_error"], false);
    }

    #[test]
    fn omits_signature_when_thinking_block_has_none() {
        let wire = block_to_wire(&ContentBlock::Thinking {
            text: "…".into(),
            signature: None,
        });
        assert!(wire.get("signature").is_none());
    }

    #[test]
    fn parses_accumulated_tool_arguments() {
        let block = Building::ToolUse {
            id: "toolu_1".into(),
            name: "open_app".into(),
            json_buf: r#"{"app":"Chrome"}"#.into(),
        }
        .finish()
        .expect("блок должен собраться");

        match block {
            ContentBlock::ToolUse { input, .. } => assert_eq!(input["app"], "Chrome"),
            other => panic!("ожидался tool_use, получено {other:?}"),
        }
    }

    #[test]
    fn treats_empty_tool_arguments_as_empty_object() {
        let block = Building::ToolUse {
            id: "toolu_1".into(),
            name: "list_windows".into(),
            json_buf: String::new(),
        }
        .finish()
        .expect("блок должен собраться");

        match block {
            ContentBlock::ToolUse { input, .. } => assert!(input.is_object()),
            other => panic!("ожидался tool_use, получено {other:?}"),
        }
    }

    #[test]
    fn drops_empty_text_blocks() {
        assert!(Building::Text(String::new()).finish().is_none());
    }
}
