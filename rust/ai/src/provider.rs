//! Контракт AI-провайдера (ТЗ §4).

use async_trait::async_trait;

use crate::types::{AiResult, ChatRequest, ChatResponse, StreamSink};

/// Тип провайдера. Определяет диалект API, а не конкретный сервис.
///
/// `OpenAiCompatible` покрывает сразу половину списка ТЗ §4 — OpenRouter, xAI,
/// Ollama, LM Studio и любой Custom API, — потому что все они говорят на диалекте
/// `/v1/chat/completions`. Отдельные реализации им не нужны, нужен только свой
/// `base_url`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Anthropic,
    OpenAi,
    Gemini,
    OpenAiCompatible,
}

impl ProviderKind {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "anthropic" | "claude" => Self::Anthropic,
            "openai" => Self::OpenAi,
            "gemini" | "google" => Self::Gemini,
            "openrouter" | "xai" | "ollama" | "lmstudio" | "custom" | "openai_compatible" => {
                Self::OpenAiCompatible
            }
            _ => return None,
        })
    }

    /// Адрес по умолчанию, если пользователь не задал свой.
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::OpenAi | Self::OpenAiCompatible => "https://api.openai.com/v1",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta",
        }
    }

    /// Модель по умолчанию для нового подключения.
    pub fn default_model(self) -> &'static str {
        match self {
            Self::Anthropic => "claude-opus-5",
            Self::OpenAi | Self::OpenAiCompatible => "gpt-4.1",
            Self::Gemini => "gemini-2.5-pro",
        }
    }

}

/// Нужен ли этому подключению ключ.
///
/// Признак привязан к конкретному сервису, а не к диалекту: OpenRouter и xAI
/// говорят на протоколе OpenAI, но ключ им нужен, а Ollama и LM Studio — это
/// локальные серверы на машине пользователя, и ключа у них нет вовсе.
pub fn requires_key(raw_kind: &str) -> bool {
    !matches!(raw_kind, "ollama" | "lmstudio" | "custom" | "openai_compatible")
}

/// Подключение к конкретному провайдеру.
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub base_url: String,
    /// Нужен ли ключ этому подключению — см. [`requires_key`].
    pub requires_key: bool,
    /// Ключ достаётся из OS secure storage непосредственно перед запросом
    /// и не сохраняется нигде за пределами этой структуры (ТЗ §29).
    pub api_key: Option<String>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    /// Один обмен с моделью. Текст уходит в `sink` по мере генерации,
    /// а полный ответ возвращается целиком — он нужен агентному циклу.
    async fn chat(&self, request: &ChatRequest, sink: &dyn StreamSink) -> AiResult<ChatResponse>;

    /// Проверка подключения для кнопки Test Connection (ТЗ §19, §17).
    ///
    /// Возвращает список доступных моделей: это одновременно и проверка ключа,
    /// и наполнение выпадающего списка в настройках — без траты токенов на
    /// пробный запрос к самой модели.
    async fn list_models(&self) -> AiResult<Vec<String>>;
}
