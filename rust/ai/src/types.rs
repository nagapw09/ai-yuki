//! Единая модель диалога для всех провайдеров (ТЗ §4).
//!
//! Провайдеры говорят на трёх разных диалектах: Anthropic отдаёт массив блоков,
//! OpenAI — строку плюс отдельный список `tool_calls`, Gemini — `parts` c
//! `functionCall`. Наверх поднимается один общий формат, а перевод в диалект и
//! обратно живёт внутри каждой реализации. Иначе агентный цикл пришлось бы писать
//! трижды, а расхождения между провайдерами вылезали бы в UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
}

/// Блок содержимого сообщения.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    /// Модель просит выполнить инструмент.
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Результат выполнения, который уходит обратно модели.
    ///
    /// `is_error` обязателен и не имеет значения по умолчанию: провалившийся
    /// инструмент должен вернуться моделью как провалившийся, иначе она построит
    /// следующий шаг на несуществующем результате — ровно то, что запрещает ТЗ §5.
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// Блок рассуждений модели.
    ///
    /// Пользователю он не показывается — ТЗ §15 прямо запрещает показ внутренней
    /// цепочки рассуждений. Хранится он ради другого: на следующем витке агентного
    /// цикла блок нужно вернуть провайдеру **без изменений** вместе с подписью,
    /// иначе модель теряет собственный контекст размышления.
    Thinking {
        text: String,
        signature: Option<String>,
    },
}

impl ContentBlock {
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text { text: value.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub role: Role,
    pub content: Vec<ContentBlock>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: vec![ContentBlock::text(text)],
        }
    }
}

/// Описание инструмента для модели (ТЗ §4 Tool Registry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON Schema входа.
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub model: String,
    /// Системная инструкция — идентичность Yuki из ТЗ §44.
    pub system: Option<String>,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

/// Почему модель остановилась.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Ответ завершён.
    EndTurn,
    /// Модель ждёт результатов инструментов — цикл продолжается (ТЗ §5).
    ToolUse,
    /// Упёрлись в лимит вывода: ответ обрезан, и об этом нужно сказать честно.
    MaxTokens,
    /// Провайдер вернул причину, которой нет в нашей модели.
    Other,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub usage: Usage,
    /// Модель, которая реально ответила: провайдер может подменить её.
    pub model: String,
}

impl ChatResponse {
    /// Весь текст ответа, склеенный из текстовых блоков.
    pub fn text(&self) -> String {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Запрошенные вызовы инструментов.
    pub fn tool_uses(&self) -> Vec<(&str, &str, &Value)> {
        self.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolUse { id, name, input } => {
                    Some((id.as_str(), name.as_str(), input))
                }
                _ => None,
            })
            .collect()
    }
}

/// Куда провайдер отдаёт поток по мере генерации.
///
/// Первый токен должен появляться на экране за 1.5–2 секунды (ТЗ §37), поэтому
/// текст идёт наверх кусками, а не одним ответом в конце.
pub trait StreamSink: Send + Sync {
    /// Очередной кусок текста.
    fn text_delta(&self, delta: &str);

    /// Модель начала собирать вызов инструмента — UI показывает «Ищу файл…» (ТЗ §15).
    fn tool_use_started(&self, name: &str);
}

/// Сток, который ничего не делает: для неинтерактивных вызовов вроде проверки ключа.
pub struct NullSink;

impl StreamSink for NullSink {
    fn text_delta(&self, _delta: &str) {}
    fn tool_use_started(&self, _name: &str) {}
}

#[derive(Debug, thiserror::Error)]
pub enum AiError {
    #[error("ключ провайдера не задан")]
    MissingApiKey,

    #[error("сеть недоступна: {0}")]
    Network(String),

    /// Провайдер ответил ошибкой. Код нужен, чтобы отличить временный сбой
    /// от неверного ключа: первое можно повторить, второе — нет (ТЗ §33).
    #[error("провайдер вернул {status}: {message}")]
    Api { status: u16, message: String },

    #[error("не удалось разобрать ответ провайдера: {0}")]
    Decode(String),

    #[error("провайдер {0} не поддерживается")]
    UnknownProvider(String),

    #[error("запрос отменён")]
    Cancelled,
}

pub type AiResult<T> = Result<T, AiError>;

impl AiError {
    /// Имеет ли смысл повторить запрос (ТЗ §33, шаг 3).
    ///
    /// Повторяем только то, что могло быть временным. 401 и 400 повтором не
    /// лечатся — их нужно показать пользователю, а не молча долбить провайдера.
    pub fn is_retryable(&self) -> bool {
        match self {
            AiError::Network(_) => true,
            AiError::Api { status, .. } => *status == 429 || *status >= 500,
            _ => false,
        }
    }
}
