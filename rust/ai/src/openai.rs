//! Провайдер диалекта `/v1/chat/completions` (ТЗ §4).
//!
//! Покрывает сам OpenAI и всё, что повторяет его протокол: OpenRouter, xAI,
//! Ollama, LM Studio и произвольный Custom API. Отличаются они только адресом
//! и наличием ключа, поэтому реализация одна.

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::anthropic::ensure_ok;
use crate::provider::{Provider, ProviderConfig};
use crate::sse::{read_events, Flow};
use crate::types::{
    AiError, AiResult, ChatRequest, ChatResponse, ContentBlock, Message, Role, StopReason,
    StreamSink, ToolSpec, Usage,
};

pub struct OpenAiProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl OpenAiProvider {
    pub fn new(config: ProviderConfig, http: reqwest::Client) -> Self {
        Self { config, http }
    }

    /// Добавляет Bearer, если ключ задан.
    ///
    /// Локальные серверы (Ollama, LM Studio) ключа не требуют, и слать пустой
    /// заголовок им нельзя — часть из них на этом падает.
    fn authorize(&self, builder: reqwest::RequestBuilder) -> AiResult<reqwest::RequestBuilder> {
        match self.config.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            Some(key) => Ok(builder.bearer_auth(key)),
            None if self.config.requires_key => Err(AiError::MissingApiKey),
            None => Ok(builder),
        }
    }
}

fn tool_to_wire(tool: &ToolSpec) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
}

/// Превращает одно наше сообщение в одно или несколько сообщений OpenAI.
///
/// Расширение один-ко-многим неизбежно: в нашей модели результат инструмента —
/// это блок внутри пользовательского сообщения, а у OpenAI это отдельное
/// сообщение с ролью `tool`. Схлопнуть их в одно нельзя — сервер отвергнет запрос.
fn message_to_wire(message: &Message) -> Vec<Value> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut images: Vec<Value> = Vec::new();

    for block in &message.content {
        match block {
            ContentBlock::Text { text: t } => text.push_str(t),
            ContentBlock::ToolUse { id, name, input } => tool_calls.push(json!({
                "id": id,
                "type": "function",
                "function": { "name": name, "arguments": input.to_string() },
            })),
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } => out.push(json!({
                "role": "tool",
                "tool_call_id": tool_use_id,
                "content": content,
            })),
            ContentBlock::Image { media_type, data } => images.push(json!({
                "type": "image_url",
                // Диалект принимает картинку только как data-URL, отдельного
                // поля для base64 в нём нет.
                "image_url": { "url": format!("data:{media_type};base64,{data}") },
            })),
            // Рассуждения — часть протокола Anthropic; здесь их отправлять некуда.
            ContentBlock::Thinking { .. } => {}
        }
    }

    if !text.is_empty() || !tool_calls.is_empty() || !images.is_empty() {
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };

        // Со строкой в `content` картинку передать нельзя: как только она
        // появляется, поле обязано стать массивом частей.
        let content = if images.is_empty() {
            json!(text)
        } else {
            let mut parts = Vec::new();
            if !text.is_empty() {
                parts.push(json!({ "type": "text", "text": text }));
            }
            parts.extend(images);
            json!(parts)
        };

        let mut msg = json!({ "role": role, "content": content });
        if !tool_calls.is_empty() {
            msg["tool_calls"] = Value::Array(tool_calls);
        }
        // Сообщения с ролью `tool` обязаны идти после вызвавшего их ответа
        // ассистента, поэтому текст вставляем в начало, а не дописываем в конец.
        out.insert(0, msg);
    }

    out
}

fn stop_reason_from_wire(value: Option<&str>) -> StopReason {
    match value {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::Other,
    }
}

/// Накопитель вызова инструмента: аргументы приходят фрагментами.
#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    arguments: String,
    /// Сообщили ли уже в UI о начале вызова.
    announced: bool,
}

#[async_trait]
impl Provider for OpenAiProvider {
    async fn chat(&self, request: &ChatRequest, sink: &dyn StreamSink) -> AiResult<ChatResponse> {
        let mut messages: Vec<Value> = Vec::new();
        if let Some(system) = &request.system {
            messages.push(json!({ "role": "system", "content": system }));
        }
        for message in &request.messages {
            messages.extend(message_to_wire(message));
        }

        let mut body = json!({
            "model": request.model,
            "stream": true,
            // Без этого поля usage в стриме не приходит вовсе.
            "stream_options": { "include_usage": true },
            "messages": messages,
        });

        if !request.tools.is_empty() {
            body["tools"] = json!(request.tools.iter().map(tool_to_wire).collect::<Vec<_>>());
        }
        if let Some(max) = request.max_tokens {
            body["max_tokens"] = json!(max);
        }
        if let Some(temp) = request.temperature {
            body["temperature"] = json!(temp);
        }

        let builder = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url));
        let response = self
            .authorize(builder)?
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        let response = ensure_ok(response).await?;

        let mut text = String::new();
        let mut calls: Vec<PartialCall> = Vec::new();
        let mut stop_reason = StopReason::EndTurn;
        let mut usage = Usage::default();
        let mut model = request.model.clone();

        read_events(response, |data| {
            // Признак конца потока в этом диалекте — не JSON, а литерал.
            if data.trim() == "[DONE]" {
                return Ok(Flow::Stop);
            }

            let event: Value = serde_json::from_str(data)
                .map_err(|e| AiError::Decode(format!("{e}: {data}")))?;

            if let Some(err) = event.get("error") {
                return Err(AiError::Api {
                    status: 0,
                    message: err["message"].as_str().unwrap_or("ошибка провайдера").to_string(),
                });
            }

            if let Some(m) = event.get("model").and_then(Value::as_str) {
                model = m.to_string();
            }
            if let Some(u) = event.get("usage").filter(|u| !u.is_null()) {
                usage.input_tokens = u["prompt_tokens"].as_u64().unwrap_or(0) as u32;
                usage.output_tokens = u["completion_tokens"].as_u64().unwrap_or(0) as u32;
            }

            let Some(choice) = event["choices"].get(0) else {
                return Ok(Flow::Continue);
            };

            if let Some(reason) = choice["finish_reason"].as_str() {
                stop_reason = stop_reason_from_wire(Some(reason));
            }

            let delta = &choice["delta"];
            if let Some(piece) = delta["content"].as_str() {
                text.push_str(piece);
                sink.text_delta(piece);
            }

            if let Some(tool_calls) = delta["tool_calls"].as_array() {
                for call in tool_calls {
                    let index = call["index"].as_u64().unwrap_or(0) as usize;
                    while calls.len() <= index {
                        calls.push(PartialCall::default());
                    }
                    let slot = &mut calls[index];

                    if let Some(id) = call["id"].as_str() {
                        slot.id = id.to_string();
                    }
                    if let Some(name) = call["function"]["name"].as_str() {
                        slot.name.push_str(name);
                    }
                    if let Some(args) = call["function"]["arguments"].as_str() {
                        slot.arguments.push_str(args);
                    }
                    // Сообщаем в UI один раз и только когда имя уже известно.
                    if !slot.announced && !slot.name.is_empty() {
                        slot.announced = true;
                        sink.tool_use_started(&slot.name);
                    }
                }
            }

            Ok(Flow::Continue)
        })
        .await?;

        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(ContentBlock::Text { text });
        }
        for call in calls {
            if call.name.is_empty() {
                continue;
            }
            let input = serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}));
            content.push(ContentBlock::ToolUse {
                id: call.id,
                name: call.name,
                input,
            });
        }

        Ok(ChatResponse {
            content,
            stop_reason,
            usage,
            model,
        })
    }

    async fn list_models(&self) -> AiResult<Vec<String>> {
        let builder = self.http.get(format!("{}/models", self.config.base_url));
        let response = self
            .authorize(builder)?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_tool_results_into_separate_tool_messages() {
        let message = Message {
            role: Role::User,
            content: vec![
                ContentBlock::ToolResult {
                    tool_use_id: "call_1".into(),
                    content: "Chrome открыт".into(),
                    is_error: false,
                },
                ContentBlock::text("а теперь найди файл"),
            ],
        };

        let wire = message_to_wire(&message);
        assert_eq!(wire.len(), 2);
        // Текст пользователя должен стоять первым, иначе сообщение с ролью tool
        // окажется раньше ответа ассистента, который его породил.
        assert_eq!(wire[0]["role"], "user");
        assert_eq!(wire[1]["role"], "tool");
        assert_eq!(wire[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn serialises_assistant_tool_calls_with_string_arguments() {
        let message = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: "call_1".into(),
                name: "open_app".into(),
                input: json!({ "app": "Chrome" }),
            }],
        };

        let wire = message_to_wire(&message);
        assert_eq!(wire.len(), 1);
        // Диалект требует именно строку, а не вложенный объект.
        assert_eq!(
            wire[0]["tool_calls"][0]["function"]["arguments"],
            json!(r#"{"app":"Chrome"}"#)
        );
    }

    #[test]
    fn drops_thinking_blocks_that_have_no_place_in_this_dialect() {
        let message = Message {
            role: Role::Assistant,
            content: vec![ContentBlock::Thinking {
                text: "…".into(),
                signature: None,
            }],
        };
        assert!(message_to_wire(&message).is_empty());
    }

    #[test]
    fn sends_images_as_data_urls_inside_a_parts_array() {
        let message = Message {
            role: Role::User,
            content: vec![
                ContentBlock::text("что на экране?"),
                ContentBlock::Image {
                    media_type: "image/png".into(),
                    data: "QUJD".into(),
                },
            ],
        };

        let wire = message_to_wire(&message);
        assert_eq!(wire.len(), 1);

        let parts = wire[0]["content"].as_array().expect("должен быть массив частей");
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,QUJD");
    }

    #[test]
    fn keeps_plain_text_as_a_string_when_there_are_no_images() {
        // Без картинок поле остаётся строкой: часть серверов этого диалекта
        // массив для простого текста не принимает.
        let message = Message {
            role: Role::User,
            content: vec![ContentBlock::text("привет")],
        };
        assert!(message_to_wire(&message)[0]["content"].is_string());
    }

    #[test]
    fn maps_finish_reasons() {
        assert_eq!(stop_reason_from_wire(Some("stop")), StopReason::EndTurn);
        assert_eq!(
            stop_reason_from_wire(Some("tool_calls")),
            StopReason::ToolUse
        );
        assert_eq!(stop_reason_from_wire(Some("length")), StopReason::MaxTokens);
    }
}
