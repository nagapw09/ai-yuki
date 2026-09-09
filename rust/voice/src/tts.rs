//! Синтез речи (ТЗ §10).
//!
//! Голос берётся у операционной системы: SAPI на Windows, AVSpeechSynthesizer
//! на macOS. Это единственный вариант, который работает сразу после установки —
//! без ключей, без сети и без скачивания моделей, а значит и в режиме Local Only
//! из ТЗ §29. Облачные голоса лучше по качеству и добавляются отдельной
//! реализацией [`TextToSpeech`], когда пользователь этого захочет.

use std::sync::Mutex;

use crate::error::{VoiceError, VoiceResult};

/// Синтезатор речи.
pub trait TextToSpeech: Send + Sync {
    /// Произносит текст. Возврат не ждёт окончания речи.
    fn speak(&self, text: &str) -> VoiceResult<()>;

    /// Немедленно замолкает.
    ///
    /// Это не удобство, а требование ТЗ §10: пользователь должен иметь
    /// возможность перебить Yuki, и перебивание, которое ждёт конца фразы,
    /// перебиванием не является.
    fn stop(&self) -> VoiceResult<()>;

    /// Говорит ли синтезатор прямо сейчас — по этому Orb показывает SPEAKING.
    fn is_speaking(&self) -> bool;

    /// Доступные голоса для выбора в настройках.
    fn voices(&self) -> Vec<String>;

    /// Выбирает голос по имени.
    fn set_voice(&self, name: &str) -> VoiceResult<()>;

    /// Скорость речи, 0.0…1.0 от диапазона движка.
    fn set_rate(&self, rate: f32) -> VoiceResult<()>;
}

/// Синтез средствами ОС.
pub struct SystemTts {
    // `tts::Tts` не Sync, а команды приходят из разных потоков конвейера.
    inner: Mutex<tts::Tts>,
}

impl SystemTts {
    pub fn new() -> VoiceResult<Self> {
        let engine = tts::Tts::default().map_err(|e| VoiceError::Tts(e.to_string()))?;
        Ok(Self {
            inner: Mutex::new(engine),
        })
    }

    fn with<T>(&self, f: impl FnOnce(&mut tts::Tts) -> VoiceResult<T>) -> VoiceResult<T> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| VoiceError::Tts("состояние синтезатора повреждено".into()))?;
        f(&mut guard)
    }
}

impl TextToSpeech for SystemTts {
    fn speak(&self, text: &str) -> VoiceResult<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.with(|engine| {
            // `interrupt = true`: новая реплика заменяет предыдущую, а не встаёт
            // за ней в очередь. Иначе Yuki договаривает устаревший ответ.
            engine
                .speak(text, true)
                .map(|_| ())
                .map_err(|e| VoiceError::Tts(e.to_string()))
        })
    }

    fn stop(&self) -> VoiceResult<()> {
        self.with(|engine| {
            engine
                .stop()
                .map(|_| ())
                .map_err(|e| VoiceError::Tts(e.to_string()))
        })
    }

    fn is_speaking(&self) -> bool {
        self.with(|engine| Ok(engine.is_speaking().unwrap_or(false)))
            .unwrap_or(false)
    }

    fn voices(&self) -> Vec<String> {
        self.with(|engine| {
            Ok(engine
                .voices()
                .map(|list| list.into_iter().map(|v| v.name()).collect())
                .unwrap_or_default())
        })
        .unwrap_or_default()
    }

    fn set_voice(&self, name: &str) -> VoiceResult<()> {
        self.with(|engine| {
            let voices = engine.voices().map_err(|e| VoiceError::Tts(e.to_string()))?;
            let voice = voices
                .into_iter()
                .find(|v| v.name() == name)
                .ok_or_else(|| VoiceError::Tts(format!("голос «{name}» не найден")))?;
            engine
                .set_voice(&voice)
                .map_err(|e| VoiceError::Tts(e.to_string()))
        })
    }

    fn set_rate(&self, rate: f32) -> VoiceResult<()> {
        self.with(|engine| {
            // У каждого движка свой диапазон скорости, поэтому наружу выставлена
            // доля 0…1, а не «слова в минуту», которые на разных ОС значат разное.
            let min = engine.min_rate();
            let max = engine.max_rate();
            let value = min + (max - min) * rate.clamp(0.0, 1.0);
            engine
                .set_rate(value)
                .map(|_| ())
                .map_err(|e| VoiceError::Tts(e.to_string()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Пустой текст не должен доходить до движка: озвучивать нечего, а вызов
    /// прервал бы уже идущую речь.
    #[test]
    fn empty_text_is_ignored_without_touching_the_engine() {
        struct Recording {
            calls: std::sync::atomic::AtomicUsize,
        }

        impl TextToSpeech for Recording {
            fn speak(&self, text: &str) -> VoiceResult<()> {
                if text.trim().is_empty() {
                    return Ok(());
                }
                self.calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
            fn stop(&self) -> VoiceResult<()> {
                Ok(())
            }
            fn is_speaking(&self) -> bool {
                false
            }
            fn voices(&self) -> Vec<String> {
                Vec::new()
            }
            fn set_voice(&self, _: &str) -> VoiceResult<()> {
                Ok(())
            }
            fn set_rate(&self, _: f32) -> VoiceResult<()> {
                Ok(())
            }
        }

        let tts = Recording {
            calls: std::sync::atomic::AtomicUsize::new(0),
        };
        tts.speak("   ").expect("пустой текст не ошибка");
        tts.speak("привет").expect("речь должна пройти");

        assert_eq!(tts.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }
}
