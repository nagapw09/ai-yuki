//! Голосовой конвейер Yuki (ТЗ §10).
//!
//! ```text
//! Микрофон → VAD → STT → Агент → TTS → Динамик
//! ```
//!
//! Крейт закрывает края этой цепочки: захват и распознавание слева, синтез
//! справа. Середина — агент — живёт в приложении, и знать о нём здесь незачем.
//!
//! # Что про слово пробуждения
//!
//! Два пути, и выбор между ними делает пользователь.
//!
//! **Без записанных образцов** обращение ищется в уже распознанном тексте: VAD
//! выделяет фразу, фраза уходит на распознавание целиком, и в тексте ищется
//! «Юки». Настройки не требует, но бюджет ТЗ §37 (< 300 мс) не выдерживает —
//! отзыв наступает после того, как человек договорил, плюс время сети.
//!
//! **С записанными образцами** ([`wake`]) обращение узнаётся локально по звуку,
//! и отзыв приходит в момент произнесения слова. Заодно фразы без обращения
//! перестают уходить на распознавание вовсе — это и деньги, и приватность.
//!
//! Цена второго пути: он зависит от голоса и требует записать слово три раза.
//! Поэтому первый путь остался как умолчание, а не был выброшен.

pub mod capture;
pub mod error;
pub mod mfcc;
pub mod session;
pub mod stt;
pub mod tts;
pub mod tts_http;
pub mod vad;
pub mod wake;

pub use capture::{default_input_name, input_devices, CaptureHandle, FRAME_MS, TARGET_RATE};
pub use error::{VoiceError, VoiceResult};
pub use session::{strip_wake_phrase, ListenMode, Utterance, VoiceEvent, VoiceSession};
pub use stt::{encode_wav, HttpStt, SpeechToText};
pub use tts::{SystemTts, TextToSpeech};
pub use tts_http::{HttpTts, HttpTtsConfig};
pub use vad::{EnergyVad, SpeechDetector, VadConfig, VadEvent};
pub use wake::{WakeDetector, WakeModel, ENROLL_SAMPLES};
