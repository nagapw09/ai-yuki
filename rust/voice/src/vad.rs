//! Детектор речевой активности (ТЗ §10).
//!
//! Задача VAD здесь одна и вполне конкретная: понять, где фраза началась и где
//! закончилась, чтобы отдать в распознавание ровно её, а не тишину вокруг.
//!
//! # Почему энергия, а не нейросеть
//!
//! Silero VAD точнее на шумном фоне, но это ONNX-модель, которую нужно скачать,
//! хранить и обновлять. Энергетический детектор с гистерезисом решает задачу
//! «человек говорит в микрофон в комнате» и не требует ни байта загрузки —
//! важное свойство для режима Local Only из ТЗ §29. Подмена на Silero сводится
//! к замене одной реализации [`SpeechDetector`], поэтому откладывать её безопасно.
//!
//! # Гистерезис
//!
//! Один порог на вход и выход даёт дребезг: громкость речи всё время пересекает
//! любую границу, и сегмент рвётся на куски посреди слова. Поэтому порогов два —
//! начать говорить труднее, чем продолжать, — а конец фразы подтверждается
//! паузой заданной длины, а не первым же тихим кадром.

/// Что произошло на очередном кадре.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// Тишина продолжается.
    Silence,
    /// Речь началась.
    SpeechStart,
    /// Речь продолжается.
    Speech,
    /// Речь закончилась: пауза достаточно длинная, фразу можно отдавать дальше.
    SpeechEnd,
}

#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// Порог RMS, выше которого кадр считается речью.
    pub start_threshold: f32,
    /// Порог, ниже которого речь считается прерванной. Всегда ниже стартового.
    pub stop_threshold: f32,
    /// Сколько миллисекунд тишины подряд завершают фразу.
    pub silence_ms: u32,
    /// Минимальная длина фразы: короче — это щелчок или кашель, а не команда.
    pub min_speech_ms: u32,
    /// Длина кадра в миллисекундах.
    pub frame_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            // Значения подобраны под нормализованный сигнал -1.0…1.0.
            start_threshold: 0.02,
            stop_threshold: 0.010,
            // 700 мс: короче — режет фразу на паузе между словами, длиннее —
            // пользователь ждёт реакции уже после того, как договорил.
            silence_ms: 700,
            min_speech_ms: 250,
            frame_ms: 20,
        }
    }
}

/// Интерфейс детектора: позволяет заменить энергию на модель, не трогая конвейер.
pub trait SpeechDetector: Send {
    fn push_frame(&mut self, frame: &[f32]) -> VadEvent;
    fn reset(&mut self);
}

pub struct EnergyVad {
    config: VadConfig,
    speaking: bool,
    silence_frames: u32,
    speech_frames: u32,
}

impl EnergyVad {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speaking: false,
            silence_frames: 0,
            speech_frames: 0,
        }
    }

    fn frames_for(&self, ms: u32) -> u32 {
        // Не меньше одного кадра: нулевой порог означал бы мгновенное срабатывание.
        (ms / self.config.frame_ms.max(1)).max(1)
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(VadConfig::default())
    }
}

/// Среднеквадратичная громкость кадра.
pub fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

impl SpeechDetector for EnergyVad {
    fn push_frame(&mut self, frame: &[f32]) -> VadEvent {
        let level = rms(frame);

        if self.speaking {
            if level >= self.config.stop_threshold {
                self.silence_frames = 0;
                self.speech_frames += 1;
                return VadEvent::Speech;
            }

            self.silence_frames += 1;
            if self.silence_frames < self.frames_for(self.config.silence_ms) {
                // Пауза внутри фразы — человек делает вдох, а не заканчивает мысль.
                return VadEvent::Speech;
            }

            self.speaking = false;
            let long_enough = self.speech_frames >= self.frames_for(self.config.min_speech_ms);
            self.speech_frames = 0;
            self.silence_frames = 0;

            // Слишком короткий всплеск — это стук по столу; отдавать его
            // в распознавание значит тратить запрос и получать мусор.
            return if long_enough {
                VadEvent::SpeechEnd
            } else {
                VadEvent::Silence
            };
        }

        if level >= self.config.start_threshold {
            self.speaking = true;
            self.speech_frames = 1;
            self.silence_frames = 0;
            return VadEvent::SpeechStart;
        }

        VadEvent::Silence
    }

    fn reset(&mut self) {
        self.speaking = false;
        self.silence_frames = 0;
        self.speech_frames = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 320; // 20 мс при 16 кГц

    fn loud() -> Vec<f32> {
        vec![0.2; FRAME]
    }

    fn quiet() -> Vec<f32> {
        vec![0.0005; FRAME]
    }

    /// Громкость между порогами: продолжает речь, но не начинает её.
    fn middling() -> Vec<f32> {
        vec![0.015; FRAME]
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_constant_signal_is_its_level() {
        assert_eq!(rms(&[0.0; 10]), 0.0);
        assert!((rms(&[0.5; 10]) - 0.5).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    #[test]
    fn detects_start_and_end_of_a_phrase() {
        let mut vad = EnergyVad::default();

        assert_eq!(vad.push_frame(&quiet()), VadEvent::Silence);
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);

        for _ in 0..20 {
            assert_eq!(vad.push_frame(&loud()), VadEvent::Speech);
        }

        // 700 мс тишины при кадре 20 мс — это 35 кадров.
        for _ in 0..34 {
            assert_eq!(vad.push_frame(&quiet()), VadEvent::Speech);
        }
        assert_eq!(vad.push_frame(&quiet()), VadEvent::SpeechEnd);
    }

    #[test]
    fn short_pause_inside_a_phrase_does_not_end_it() {
        let mut vad = EnergyVad::default();
        vad.push_frame(&loud());

        // Полсекунды паузы — вдох между словами.
        for _ in 0..25 {
            assert_eq!(vad.push_frame(&quiet()), VadEvent::Speech);
        }
        assert_eq!(vad.push_frame(&loud()), VadEvent::Speech);
    }

    #[test]
    fn hysteresis_keeps_quiet_speech_going_but_will_not_start_on_it() {
        let mut vad = EnergyVad::default();

        // Уровень между порогами фразу не начинает.
        for _ in 0..10 {
            assert_eq!(vad.push_frame(&middling()), VadEvent::Silence);
        }

        // Но уже начатую — продолжает.
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);
        assert_eq!(vad.push_frame(&middling()), VadEvent::Speech);
    }

    #[test]
    fn discards_clicks_that_are_too_short_to_be_speech() {
        let mut vad = EnergyVad::default();

        // Один громкий кадр — 20 мс, минимум 250 мс.
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);
        for _ in 0..34 {
            vad.push_frame(&quiet());
        }
        // Фраза закончилась, но отдавать нечего.
        assert_eq!(vad.push_frame(&quiet()), VadEvent::Silence);
    }

    #[test]
    fn reset_forgets_an_unfinished_phrase() {
        let mut vad = EnergyVad::default();
        vad.push_frame(&loud());
        vad.reset();
        // После сброса громкий кадр снова начинает фразу, а не продолжает старую.
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);
    }
}
