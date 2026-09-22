//! Состояние приложения, разделяемое всеми Tauri-командами.

use yuki_system::PlatformAdapters;

use crate::calendar::TokenCache;
use crate::capabilities::McpRegistry;
use crate::diagnostics::LogBuffer;
use crate::speech_local::SpeechState;
use crate::storage::Storage;
use crate::voice::VoiceState;

/// Всё, что живёт столько же, сколько процесс Yuki.
///
/// Адаптеры создаются один раз: `enigo` держит платформенное состояние ввода,
/// а пересоздание COM-объектов на каждый вызов стоит миллисекунд, которых нет
/// в бюджете отзывчивости из ТЗ §37.
pub struct AppState {
    pub chat_requests:
        std::sync::Mutex<std::collections::HashMap<String, tokio::sync::oneshot::Sender<()>>>,
    pub adapters: PlatformAdapters,
    pub storage: Storage,
    /// Один HTTP-клиент на всё приложение: он держит пул соединений и
    /// переиспользует TLS-сессии, что срезает задержку второго и последующих
    /// запросов к провайдеру — а бюджет первого токена задан в ТЗ §37.
    pub http: reqwest::Client,
    /// Голосовой режим (ТЗ §10). Пустой, пока пользователь его не включил.
    pub voice: VoiceState,
    /// Локальный движок распознавания: процесс whisper.cpp рядом с Yuki.
    pub speech: SpeechState,
    /// Подключённые MCP-серверы (ТЗ §19).
    pub mcp: McpRegistry,
    /// Последние строки журнала для отчёта о поломке (docs/GAPS.md §14).
    pub logs: LogBuffer,
    /// Access-токены календарей (ТЗ §25). Только в памяти: они живут
    /// час, и записывать их на диск ради экономии одного обновления не стоит.
    pub calendar: TokenCache,
}

impl AppState {
    pub fn new(
        adapters: PlatformAdapters,
        storage: Storage,
        http: reqwest::Client,
        logs: LogBuffer,
    ) -> Self {
        Self {
            chat_requests: Default::default(),
            adapters,
            storage,
            http,
            logs,
            voice: VoiceState::default(),
            speech: SpeechState::default(),
            mcp: McpRegistry::default(),
            calendar: TokenCache::default(),
        }
    }
}
