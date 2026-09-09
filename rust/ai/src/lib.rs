//! AI Provider Layer (ТЗ §4).
//!
//! Слой живёт в Rust, а не в TypeScript, по двум причинам, и обе — жёсткие:
//!
//! 1. **Ключи.** ТЗ §29 требует хранить их в системном хранилище ОС. Если бы
//!    запросы собирал фронтенд, ключ пришлось бы отдать в WebView — и он оказался
//!    бы в памяти страницы, в devtools и в любом упавшем в неё скрипте.
//! 2. **CSP.** Политика окна (`connect-src 'self'`) запрещает WebView ходить в
//!    сеть. Ослабить её ради провайдеров — значит открыть исходящие запросы всему,
//!    что исполняется на странице, включая будущие плагины из ТЗ §20.
//!
//! Агентный цикл при этом остаётся в TypeScript: он оркестрирует, а не ходит в сеть.

pub mod anthropic;
pub mod gemini;
pub mod openai;
pub mod provider;
pub mod sse;
pub mod types;

use std::time::Duration;

pub use provider::{requires_key, Provider, ProviderConfig, ProviderKind};
pub use types::{
    AiError, AiResult, ChatRequest, ChatResponse, ContentBlock, Message, NullSink, Role,
    StopReason, StreamSink, ToolSpec, Usage,
};

/// Потолок времени на один обмен с моделью.
///
/// Щедрый намеренно: рассуждающие модели на сложной задаче думают минутами, и
/// обрывать их по короткому таймауту — значит терять уже оплаченную работу.
/// От зависшего соединения защищает отдельный таймаут на установку связи.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Создаёт HTTP-клиент, общий для всех провайдеров.
///
/// Один клиент на приложение, а не на запрос: он держит пул соединений и
/// переиспользует TLS-сессии, что заметно ускоряет второй и последующие запросы.
pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("Yuki/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// Собирает провайдера по конфигурации.
pub fn build(config: ProviderConfig, http: reqwest::Client) -> Box<dyn Provider> {
    match config.kind {
        ProviderKind::Anthropic => Box::new(anthropic::AnthropicProvider::new(config, http)),
        ProviderKind::Gemini => Box::new(gemini::GeminiProvider::new(config, http)),
        ProviderKind::OpenAi | ProviderKind::OpenAiCompatible => {
            Box::new(openai::OpenAiProvider::new(config, http))
        }
    }
}

/// Системная инструкция Yuki (ТЗ §44).
///
/// Текст задан ТЗ дословно и вынесен в константу, а не в настройки: пользователь
/// настраивает роль и тон поверх этой инструкции (см. docs/GAPS.md §7), но саму
/// идентичность и запрет рапортовать о невыполненном не переопределяет.
pub const YUKI_IDENTITY: &str = r#"You are Yuki, a personal AI desktop assistant.

Your purpose is to help the user operate their computer, complete tasks, manage information, automate repetitive workflows, and communicate naturally.

When a task requires action:
1. Understand the user's intent.
2. Determine which tools are required.
3. Execute the minimum necessary actions.
4. Verify important results.
5. Report the result clearly.

Never claim an action was completed unless the corresponding tool confirmed successful execution.

For dangerous or irreversible actions, request confirmation according to the permission policy.

Be concise, natural, helpful and slightly playful.

You are Yuki, not ChatGPT."#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_every_provider_from_the_spec_list() {
        // ТЗ §4 перечисляет провайдеров явно — все они должны опознаваться.
        for name in [
            "openai",
            "claude",
            "anthropic",
            "gemini",
            "xai",
            "openrouter",
            "ollama",
            "lmstudio",
            "custom",
        ] {
            assert!(
                ProviderKind::from_str(name).is_some(),
                "провайдер {name} из ТЗ §4 не опознаётся"
            );
        }
        assert!(ProviderKind::from_str("нечто").is_none());
    }

    #[test]
    fn key_requirement_follows_the_service_not_the_dialect() {
        // OpenRouter и xAI говорят на протоколе OpenAI, но ключ им нужен.
        for hosted in ["anthropic", "openai", "gemini", "openrouter", "xai"] {
            assert!(requires_key(hosted), "{hosted} должен требовать ключ");
        }
        // Локальные серверы работают без ключа.
        for local in ["ollama", "lmstudio", "custom"] {
            assert!(!requires_key(local), "{local} не должен требовать ключ");
        }
    }

    #[test]
    fn identity_keeps_the_no_false_success_rule() {
        // Инвариант ТЗ §5 продублирован в инструкции модели, а не только в коде.
        assert!(YUKI_IDENTITY.contains("Never claim an action was completed"));
    }
}
