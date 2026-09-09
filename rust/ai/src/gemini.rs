//! Провайдер Google Gemini — `generateContent` (ТЗ §4).

use std::collections::HashMap;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::anthropic::ensure_ok;
use crate::provider::{Provider, ProviderConfig};
use crate::sse::{read_events, Flow};
use crate::types::{
    AiError, AiResult, ChatRequest, ChatResponse, ContentBlock, Message, Role, StopReason,
    StreamSink, ToolSpec, Usage,
};

pub struct GeminiProvider {
    config: ProviderConfig,
    http: reqwest::Client,
}

impl GeminiProvider {
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

/// Идентификатор вызова, которого у Gemini нет.
///
/// Anthropic и OpenAI связывают запрос и результат по id, Gemini — по имени
/// функции. Чтобы агентный цикл оставался одинаковым для всех провайдеров, id
/// синтезируется здесь и разбирается обратно при отправке результата.
fn synthetic_id(name: &str, ordinal: usize) -> String {
    format!("gemini:{name}:{ordinal}")
}

fn name_from_synthetic_id(id: &str) -> Option<&str> {
    id.strip_prefix("gemini:")
        .and_then(|rest| rest.rsplit_once(':').map(|(name, _)| name))
}

fn tool_to_wire(tool: &ToolSpec) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.input_schema,
    })
}

/// Переводит историю в формат `contents`.
///
/// Имя функции для `functionResponse` берётся из id вызова: сначала пробуем
/// разобрать синтезированный id, а если история пришла от другого провайдера —
/// ищем по карте, собранной из предыдущих ходов.
fn messages_to_contents(messages: &[Message]) -> Vec<Value> {
    let mut names: HashMap<&str, &str> = HashMap::new();
    for message in messages {
        for block in &message.content {
            if let ContentBlock::ToolUse { id, name, .. } = block {
                names.insert(id.as_str(), name.as_str());
            }
        }
    }

    let mut contents = Vec::new();
    for message in messages {
        let mut parts = Vec::new();
        let mut responses = Vec::new();

        for block in &message.content {
            match block {
                ContentBlock::Text { text } => parts.push(json!({ "text": text })),
                ContentBlock::ToolUse { name, input, .. } => {
                    parts.push(json!({ "functionCall": { "name": name, "args": input } }))
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let name = name_from_synthetic_id(tool_use_id)
                        .or_else(|| names.get(tool_use_id.as_str()).copied())
                        .unwrap_or("tool");
                    // Ошибку кладём в отдельное поле: строка «ошибка: …» в поле
                    // результата неотличима для модели от успешного ответа.
                    let payload = if *is_error {
                        json!({ "error": content })
                    } else {
                        json!({ "result": content })
                    };
                    responses.push(json!({
                        "functionResponse": { "name": name, "response": payload }
                    }));
                }
                ContentBlock::Thinking { .. } => {}
            }
        }

        if !parts.is_empty() {
            contents.push(json!({
                "role": match message.role { Role::User => "user", Role::Assistant => "model" },
                "parts": parts,
            }));
        }
        if !responses.is_empty() {
            contents.push(json!({ "role": "user", "parts": responses }));
        }
    }

    contents
}

#[async_trait]
impl Provider for GeminiProvider {
    async fn chat(&self, request: &ChatRequest, sink: &dyn StreamSink) -> AiResult<ChatResponse> {
        let key = self.api_key()?;

        let mut body = json!({ "contents": messages_to_contents(&request.messages) });

        if let Some(system) = &request.system {
            body["systemInstruction"] = json!({ "parts": [{ "text": system }] });
        }
        if !request.tools.is_empty() {
            body["tools"] = json!([{
                "functionDeclarations": request.tools.iter().map(tool_to_wire).collect::<Vec<_>>()
            }]);
        }

        let mut generation = json!({});
        if let Some(max) = request.max_tokens {
            generation["maxOutputTokens"] = json!(max);
        }
        if let Some(temp) = request.temperature {
            generation["temperature"] = json!(temp);
        }
        if generation.as_object().is_some_and(|o| !o.is_empty()) {
            body["generationConfig"] = generation;
        }

        let url = format!(
            "{}/models/{}:streamGenerateContent?alt=sse",
            self.config.base_url, request.model
        );

        let response = self
            .http
            .post(url)
            // Ключ в заголовке, а не в query: query-строка попадает в логи прокси.
            .header("x-goog-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        let response = ensure_ok(response).await?;

        let mut text = String::new();
        let mut tool_uses: Vec<ContentBlock> = Vec::new();
        let mut usage = Usage::default();
        let mut finish: Option<String> = None;

        read_events(response, |data| {
            let event: Value = serde_json::from_str(data)
                .map_err(|e| AiError::Decode(format!("{e}: {data}")))?;

            if let Some(err) = event.get("error") {
                return Err(AiError::Api {
                    status: err["code"].as_u64().unwrap_or(0) as u16,
                    message: err["message"].as_str().unwrap_or("ошибка провайдера").to_string(),
                });
            }

            if let Some(meta) = event.get("usageMetadata") {
                usage.input_tokens = meta["promptTokenCount"].as_u64().unwrap_or(0) as u32;
                usage.output_tokens = meta["candidatesTokenCount"].as_u64().unwrap_or(0) as u32;
            }

            let Some(candidate) = event["candidates"].get(0) else {
                return Ok(Flow::Continue);
            };
            if let Some(reason) = candidate["finishReason"].as_str() {
                finish = Some(reason.to_string());
            }

            for part in candidate["content"]["parts"].as_array().into_iter().flatten() {
                if let Some(piece) = part["text"].as_str() {
                    text.push_str(piece);
                    sink.text_delta(piece);
                }
                if let Some(call) = part.get("functionCall") {
                    let name = call["name"].as_str().unwrap_or_default().to_string();
                    sink.tool_use_started(&name);
                    tool_uses.push(ContentBlock::ToolUse {
                        id: synthetic_id(&name, tool_uses.len()),
                        name,
                        input: call["args"].clone(),
                    });
                }
            }

            Ok(Flow::Continue)
        })
        .await?;

        // У Gemini нет отдельной причины остановки для вызова инструмента:
        // о нём говорит само наличие functionCall в ответе.
        let stop_reason = if !tool_uses.is_empty() {
            StopReason::ToolUse
        } else {
            match finish.as_deref() {
                Some("STOP") => StopReason::EndTurn,
                Some("MAX_TOKENS") => StopReason::MaxTokens,
                None => StopReason::EndTurn,
                _ => StopReason::Other,
            }
        };

        let mut content = Vec::new();
        if !text.is_empty() {
            content.push(ContentBlock::Text { text });
        }
        content.extend(tool_uses);

        Ok(ChatResponse {
            content,
            stop_reason,
            usage,
            model: request.model.clone(),
        })
    }

    async fn list_models(&self) -> AiResult<Vec<String>> {
        let key = self.api_key()?;

        let response = self
            .http
            .get(format!("{}/models", self.config.base_url))
            .header("x-goog-api-key", key)
            .send()
            .await
            .map_err(|e| AiError::Network(e.to_string()))?;

        let body: Value = ensure_ok(response)
            .await?
            .json()
            .await
            .map_err(|e| AiError::Decode(e.to_string()))?;

        Ok(body["models"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|m| m["name"].as_str())
                    // API отдаёт «models/gemini-2.5-pro», а в запрос идёт короткое имя.
                    .map(|n| n.trim_start_matches("models/").to_string())
                    .collect()
            })
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_synthetic_call_ids() {
        let id = synthetic_id("open_app", 3);
        assert_eq!(name_from_synthetic_id(&id), Some("open_app"));
    }

    #[test]
    fn ignores_ids_from_other_providers() {
        assert_eq!(name_from_synthetic_id("toolu_abc"), None);
    }

    #[test]
    fn resolves_tool_name_from_earlier_turn_for_foreign_ids() {
        let messages = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "toolu_abc".into(),
                    name: "file_search".into(),
                    input: json!({}),
                }],
            },
            Message {
                role: Role::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "toolu_abc".into(),
                    content: "нашла 2 файла".into(),
                    is_error: false,
                }],
            },
        ];

        let contents = messages_to_contents(&messages);
        let response = &contents[1]["parts"][0]["functionResponse"];
        assert_eq!(response["name"], "file_search");
        assert_eq!(response["response"]["result"], "нашла 2 файла");
    }

    #[test]
    fn marks_failed_tool_results_as_errors() {
        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: synthetic_id("open_app", 0),
                content: "приложение не найдено".into(),
                is_error: true,
            }],
        }];

        let contents = messages_to_contents(&messages);
        let response = &contents[0]["parts"][0]["functionResponse"]["response"];
        assert!(response.get("error").is_some());
        assert!(response.get("result").is_none());
    }
}
