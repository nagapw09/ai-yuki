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
//! Обращение распознаётся так: VAD выделяет фразу, фраза распознаётся целиком,
//! и в тексте ищется «Юки». Это работает и не требует ни одной скачанной модели,
//! но не укладывается в бюджет ТЗ §37 (< 300 мс): реакция наступает после конца
//! фразы, а не в момент произнесения слова. Для настоящих 300 мс нужна отдельная
//! маленькая модель пробуждения, которая слушает поток непрерывно; это отмечено
//! в роадмапе и меняет только реализацию детектора, а не конвейер.

pub mod capture;
pub mod error;
pub mod session;
pub mod stt;
pub mod tts;
pub mod vad;

pub use capture::{default_input_name, input_devices, CaptureHandle, FRAME_MS, TARGET_RATE};
pub use error::{VoiceError, VoiceResult};
pub use session::{strip_wake_phrase, ListenMode, Utterance, VoiceEvent, VoiceSession};
pub use stt::{encode_wav, HttpStt, SpeechToText};
pub use tts::{SystemTts, TextToSpeech};
pub use vad::{EnergyVad, SpeechDetector, VadConfig, VadEvent};
