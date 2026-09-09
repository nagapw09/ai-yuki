//! Голосовая сессия: микрофон → VAD → STT (ТЗ §10).
//!
//! Здесь склеиваются части конвейера и живёт единственное состояние: говорит
//! сейчас человек или нет. Дальше по цепочке (агент → TTS → динамик) сессия не
//! идёт намеренно — это уже оркестрация, и она в приложении.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::capture::{self, CaptureHandle, FRAME_MS, TARGET_RATE};
use crate::error::{VoiceError, VoiceResult};
use crate::vad::{rms, EnergyVad, SpeechDetector, VadConfig, VadEvent};

/// Максимальная длина одной фразы.
///
/// Ограничение защищает не от болтливости, а от залипшего VAD: если детектор
/// по какой-то причине не увидит конца, без потолка буфер будет расти, пока не
/// съест память, и в распознавание уйдёт получасовая запись.
const MAX_UTTERANCE_SECONDS: usize = 30;
const MAX_SAMPLES: usize = TARGET_RATE as usize * MAX_UTTERANCE_SECONDS;

/// Как слушать (ТЗ §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListenMode {
    /// Пользователь держит кнопку или нажал «говорить» — слушаем до его команды.
    PushToTalk,
    /// Постоянное прослушивание со словом пробуждения.
    WakeWord,
}

/// Что происходит в сессии.
#[derive(Debug, Clone)]
pub enum VoiceEvent {
    /// Текущая громкость 0…1 — по ней Orb дышит в такт голосу (ТЗ §13).
    Level(f32),
    SpeechStarted,
    /// Фраза закончилась, идёт распознавание.
    SpeechEnded,
    /// Распознанный текст.
    ///
    /// `addressed` показывает, обращались ли к Yuki: в режиме слова пробуждения
    /// фразы без обращения приходят сюда же, но с `false`. Решение, что с ними
    /// делать, принимает приложение, а не конвейер.
    Transcribed { text: String, addressed: bool },
    Error(String),
}

/// Сегмент речи, ушедший на распознавание.
pub struct Utterance {
    pub samples: Vec<f32>,
}

/// Варианты обращения к Yuki (ТЗ §10).
const WAKE_PHRASES: &[&str] = &[
    "hey yuki", "hey youki", "эй юки", "хей юки", "yuki", "юки", "юкки", "юке",
];

/// Убирает слово пробуждения и возвращает саму команду.
///
/// `None` означает, что к Yuki не обращались. Отдельно разбирается случай, когда
/// после обращения ничего нет: «Юки» — это обращение, на которое нужно отозваться,
/// а не пустая команда, поэтому возвращается пустая строка, а не `None`.
pub fn strip_wake_phrase(text: &str) -> Option<String> {
    let normalized = text
        .trim()
        .trim_start_matches(['«', '"', '\''])
        .to_lowercase()
        // Запятая после обращения — самый частый разделитель, и без её
        // отсечения команда начиналась бы со знака препинания.
        .replace(['!', '?'], "");

    for phrase in WAKE_PHRASES {
        if let Some(rest) = normalized.strip_prefix(phrase) {
            // Следом должна идти граница слова, иначе «юкитерий» сойдёт за обращение.
            let next = rest.chars().next();
            if next.is_some_and(|c| c.is_alphanumeric()) {
                continue;
            }

            let command = rest
                .trim_start_matches([',', '.', ':', ' ', '—', '-'])
                .trim();

            // Возвращаем кусок исходного текста, а не приведённый к нижнему
            // регистру: команда пойдёт модели, и регистр в ней осмыслен.
            let offset = text.len() - command.len();
            return Some(text[offset.min(text.len())..].trim().to_string());
        }
    }

    None
}

/// Работающая голосовая сессия.
pub struct VoiceSession {
    mode: ListenMode,
    capture: Option<CaptureHandle>,
    /// Сигнал «хватит слушать» для push-to-talk.
    finish: Arc<AtomicBool>,
    utterances: Option<Receiver<Utterance>>,
}

impl VoiceSession {
    /// Запускает прослушивание.
    ///
    /// `on_event` вызывается из аудиопотока: он должен быть быстрым. Готовые
    /// сегменты речи забираются через [`VoiceSession::utterances`] — распознавание
    /// асинхронно и в аудиопотоке ему делать нечего.
    pub fn start<F>(mode: ListenMode, mut on_event: F) -> VoiceResult<Self>
    where
        F: FnMut(VoiceEvent) + Send + 'static,
    {
        let (tx, rx): (Sender<Utterance>, Receiver<Utterance>) = mpsc::channel();
        let finish = Arc::new(AtomicBool::new(false));
        let finish_in_stream = finish.clone();

        let mut vad = EnergyVad::new(VadConfig {
            frame_ms: FRAME_MS,
            ..VadConfig::default()
        });
        let mut buffer: Vec<f32> = Vec::with_capacity(TARGET_RATE as usize * 3);

        let capture = capture::start(move |frame| {
            on_event(VoiceEvent::Level(level_of(frame)));

            let forced = finish_in_stream.swap(false, Ordering::Relaxed);
            let event = vad.push_frame(frame);

            match event {
                VadEvent::SpeechStart => {
                    buffer.clear();
                    buffer.extend_from_slice(frame);
                    on_event(VoiceEvent::SpeechStarted);
                }
                VadEvent::Speech => {
                    if buffer.len() < MAX_SAMPLES {
                        buffer.extend_from_slice(frame);
                    }
                }
                VadEvent::SpeechEnd => {
                    on_event(VoiceEvent::SpeechEnded);
                    if tx
                        .send(Utterance {
                            samples: std::mem::take(&mut buffer),
                        })
                        .is_err()
                    {
                        // Приёмник уничтожен — сессия закрывается.
                        return;
                    }
                }
                VadEvent::Silence => {}
            }

            // Push-to-talk отпускают в любой момент, в том числе посреди слова:
            // отдаём накопленное, не дожидаясь паузы.
            if forced && !buffer.is_empty() {
                vad.reset();
                on_event(VoiceEvent::SpeechEnded);
                let _ = tx.send(Utterance {
                    samples: std::mem::take(&mut buffer),
                });
            }
        })?;

        Ok(Self {
            mode,
            capture: Some(capture),
            finish,
            utterances: Some(rx),
        })
    }

    pub fn mode(&self) -> ListenMode {
        self.mode
    }

    /// Просит отдать накопленное прямо сейчас — для отпущенной кнопки push-to-talk.
    pub fn finish_utterance(&self) {
        self.finish.store(true, Ordering::Relaxed);
    }

    /// Забирает очередь готовых сегментов речи.
    ///
    /// Отдаётся один раз и во владение: распознавание блокирует поток, и держать
    /// его рядом с сессией — значит держать блокирующий вызов там, где живёт
    /// состояние UI. Второй вызов вернёт `None`.
    pub fn take_utterances(&mut self) -> Option<Receiver<Utterance>> {
        self.utterances.take()
    }

    /// Останавливает захват.
    pub fn stop(mut self) -> VoiceResult<()> {
        match self.capture.take() {
            Some(capture) => {
                capture.stop();
                Ok(())
            }
            None => Err(VoiceError::NotRunning),
        }
    }
}

/// Приводит громкость кадра к 0…1 для анимации Orb.
///
/// Масштаб подобран под речь: RMS около 0.2 — это уже уверенный разговорный
/// уровень, и дальше расти анимации некуда.
fn level_of(frame: &[f32]) -> f32 {
    (rms(frame) / 0.2).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_wake_phrase_and_returns_the_command() {
        assert_eq!(
            strip_wake_phrase("Юки, открой браузер").as_deref(),
            Some("открой браузер")
        );
        assert_eq!(
            strip_wake_phrase("Hey Yuki what is the time").as_deref(),
            Some("what is the time")
        );
    }

    #[test]
    fn a_bare_address_is_still_an_address() {
        // «Юки?» — это оклик, на который надо отозваться, а не тишина.
        assert_eq!(strip_wake_phrase("Юки?").as_deref(), Some(""));
    }

    #[test]
    fn ignores_speech_that_is_not_addressed_to_yuki() {
        assert_eq!(strip_wake_phrase("открой браузер"), None);
        assert_eq!(strip_wake_phrase("надо бы позвонить"), None);
    }

    #[test]
    fn does_not_trigger_on_a_word_that_merely_starts_the_same() {
        assert_eq!(strip_wake_phrase("юкитерий это порода"), None);
    }

    #[test]
    fn preserves_case_of_the_command_itself() {
        // Регистр значим: модель получит текст как есть.
        assert_eq!(
            strip_wake_phrase("Юки, открой Chrome").as_deref(),
            Some("открой Chrome")
        );
    }

    #[test]
    fn level_saturates_instead_of_exceeding_one() {
        assert_eq!(level_of(&[1.0; 320]), 1.0);
        assert_eq!(level_of(&[0.0; 320]), 0.0);
        assert!((level_of(&[0.1; 320]) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn utterance_cap_matches_thirty_seconds() {
        assert_eq!(MAX_SAMPLES, 16_000 * 30);
    }
}
