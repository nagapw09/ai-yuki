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
//!
//! # Почему пороги не фиксированные
//!
//! Диагностика на реальном микрофоне показала, почему фиксированный порог не
//! годится: тихая комната давала RMS около 0.047 — вдвое выше разумного
//! стартового порога. С фиксированным порогом VAD считал бы шум речью
//! непрерывно, фраза не заканчивалась бы никогда, а режим слова пробуждения
//! молчал бы до упора в ограничение длины.
//!
//! Поэтому порог отсчитывается от текущего уровня шума: речь должна быть во
//! столько-то раз громче фона.
//!
//! Оценка шума — нижний перцентиль громкости за последние пару секунд. Он
//! устойчив к речи по построению: в любом двухсекундном окне разговора есть
//! паузы между словами, и именно они попадают в нижнюю часть распределения.
//! Сглаживание среднего пришлось бы выключать на время речи, а значит —
//! угадывать, где она, то есть решать ровно ту задачу, ради которой всё это
//! и считается.
//!
//! Именно перцентиль, а не строгий минимум: замер на живом микрофоне показал,
//! что фон гуляет вчетверо — от 0.012 в провалах до 0.050 на всплесках.
//! Минимум цепляется за самый глубокий провал и занижает оценку настолько,
//! что всплески того же фона продолжают считаться речью.
//!
//! Решение о кадре принимается по оценке, собранной на **предыдущих** кадрах,
//! и только потом оценка обновляется. Иначе первый же громкий кадр объявил бы
//! сам себя фоном.
//!
//! # Прогрев
//!
//! Первые полсекунды детектор только слушает и ничего не решает. Причина
//! обнаружилась на живом микрофоне: до измерения фона работал абсолютный порог,
//! шум комнаты его брал, детектор защёлкивался в «речь» — и больше не выходил,
//! потому что порог продолжения тоже был ниже шума. Полсекунды молчания в начале
//! пользователь не замечает, а без них режим слова пробуждения не работает вовсе.

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
    /// Нижняя граница порога начала речи.
    ///
    /// Именно нижняя, а не сама граница: реальный порог поднимается над уровнем
    /// шума комнаты — см. [`VadConfig::noise_start_ratio`].
    pub start_threshold: f32,
    /// Нижняя граница порога, ниже которого речь считается прерванной.
    pub stop_threshold: f32,
    /// Во сколько раз речь должна быть громче фонового шума, чтобы её начать.
    pub noise_start_ratio: f32,
    /// Во сколько раз громче шума должен быть звук, чтобы речь продолжалась.
    pub noise_stop_ratio: f32,
    /// За сколько миллисекунд ищется минимум громкости.
    pub noise_window_ms: u32,
    /// Сколько миллисекунд детектор только слушает, ничего не решая.
    pub warmup_ms: u32,
    /// Какой перцентиль окна берётся за уровень фона, 0…1.
    pub noise_percentile: f32,
    /// Потолок порога начала речи.
    ///
    /// Ограничивается именно порог, а не оценка шума: в шумной комнате
    /// пропорциональный порог уполз бы выше обычной громкости речи, и Yuki
    /// перестала бы слышать пользователя вовсе. Лучше несколько ложных
    /// срабатываний, которые отсеет распознавание, чем глухота.
    pub max_start_threshold: f32,
    /// Потолок порога продолжения речи.
    pub max_stop_threshold: f32,
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
            // Абсолютный минимум: ниже этого уровня речи в цифровом сигнале
            // просто не бывает, как бы тихо ни было в комнате.
            start_threshold: 0.02,
            stop_threshold: 0.010,
            // Коэффициенты подобраны по замеру на реальном микрофоне: фон в
            // тихой комнате колебался от 0.014 в паузах до 0.047 на всплесках,
            // то есть впятеро. Втрое от минимума не хватало — всплески фона
            // продолжали считаться речью.
            noise_start_ratio: 4.0,
            noise_stop_ratio: 2.5,
            // Две секунды: в разговоре за это время обязательно есть пауза.
            noise_window_ms: 2000,
            warmup_ms: 500,
            // Четверть кадров окна тише этого уровня. Ниже — оценка цепляется
            // за случайные провалы, выше — в неё начинает попадать речь.
            noise_percentile: 0.25,
            // Обычная речь в микрофон даёт RMS примерно 0.15…0.4, поэтому
            // выше 0.12 порог поднимать нельзя.
            max_start_threshold: 0.12,
            max_stop_threshold: 0.07,
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
    /// Громкости последних кадров — по ним ищется минимум.
    recent: std::collections::VecDeque<f32>,
    /// Текущая оценка фонового шума.
    noise_floor: f32,
}

impl EnergyVad {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            speaking: false,
            silence_frames: 0,
            speech_frames: 0,
            recent: std::collections::VecDeque::new(),
            // Ноль означает «фон ещё не измерен»: до первых измерений работают
            // абсолютные пороги.
            noise_floor: 0.0,
        }
    }

    /// Порог начала речи с учётом шума.
    pub fn start_threshold(&self) -> f32 {
        (self.noise_floor * self.config.noise_start_ratio)
            .clamp(self.config.start_threshold, self.config.max_start_threshold)
    }

    /// Порог, ниже которого речь считается прерванной.
    pub fn stop_threshold(&self) -> f32 {
        (self.noise_floor * self.config.noise_stop_ratio)
            .clamp(self.config.stop_threshold, self.config.max_stop_threshold)
    }

    /// Текущая оценка уровня шума — её показывает диагностика микрофона.
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// Добавляет громкость кадра в окно и пересчитывает оценку шума.
    fn observe(&mut self, level: f32) {
        let window = (self.config.noise_window_ms / self.config.frame_ms.max(1)).max(1) as usize;

        self.recent.push_back(level);
        while self.recent.len() > window {
            self.recent.pop_front();
        }

        if self.recent.is_empty() {
            self.noise_floor = 0.0;
            return;
        }

        // Частичная сортировка вместо полной: нужен один элемент, а не порядок.
        let mut levels: Vec<f32> = self.recent.iter().copied().collect();
        let index = ((levels.len() - 1) as f32 * self.config.noise_percentile.clamp(0.0, 1.0))
            .round() as usize;
        levels.select_nth_unstable_by(index, |a, b| a.total_cmp(b));
        self.noise_floor = levels[index];
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

        // Пока фон не измерен, судить о громкости не по чему.
        if self.recent.len() < self.frames_for(self.config.warmup_ms) as usize {
            self.observe(level);
            return VadEvent::Silence;
        }

        let event = self.decide(level);
        // Оценка обновляется после решения — см. пояснение в шапке модуля.
        self.observe(level);
        event
    }

    fn reset(&mut self) {
        self.speaking = false;
        self.silence_frames = 0;
        self.speech_frames = 0;
        // Окно громкостей намеренно сохраняется: комната между фразами не
        // меняется, а повторный замер стоил бы первых слов следующей фразы.
    }
}

impl EnergyVad {
    fn decide(&mut self, level: f32) -> VadEvent {
        if self.speaking {
            if level >= self.stop_threshold() {
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

        if level >= self.start_threshold() {
            self.speaking = true;
            self.speech_frames = 1;
            self.silence_frames = 0;
            return VadEvent::SpeechStart;
        }

        VadEvent::Silence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: usize = 320; // 20 мс при 16 кГц

    fn loud() -> Vec<f32> {
        vec![0.2; FRAME]
    }

    /// Прогревает детектор тишиной: до этого он ничего не решает.
    fn warmed() -> EnergyVad {
        let mut vad = EnergyVad::default();
        for _ in 0..30 {
            vad.push_frame(&quiet());
        }
        vad
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
        let mut vad = warmed();

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
        let mut vad = warmed();
        vad.push_frame(&loud());

        // Полсекунды паузы — вдох между словами.
        for _ in 0..25 {
            assert_eq!(vad.push_frame(&quiet()), VadEvent::Speech);
        }
        assert_eq!(vad.push_frame(&loud()), VadEvent::Speech);
    }

    #[test]
    fn hysteresis_keeps_quiet_speech_going_but_will_not_start_on_it() {
        let mut vad = warmed();

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
        let mut vad = warmed();

        // Один громкий кадр — 20 мс, минимум 250 мс.
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);
        for _ in 0..34 {
            vad.push_frame(&quiet());
        }
        // Фраза закончилась, но отдавать нечего.
        assert_eq!(vad.push_frame(&quiet()), VadEvent::Silence);
    }

    #[test]
    fn decides_nothing_until_the_background_is_measured() {
        // Громкий кадр в первые полсекунды не начинает фразу: фон ещё неизвестен,
        // и именно на этом детектор раньше защёлкивался навсегда.
        let mut vad = EnergyVad::default();
        assert_eq!(vad.push_frame(&loud()), VadEvent::Silence);
    }

    #[test]
    fn room_noise_stops_counting_as_speech_once_the_floor_is_measured() {
        // Уровень, измеренный на реальном микрофоне в тихой комнате: он выше
        // абсолютного порога, и без адаптации VAD считал бы его речью вечно.
        let noise = vec![0.047_f32; FRAME];
        let mut vad = EnergyVad::default();

        for _ in 0..60 {
            vad.push_frame(&noise);
        }
        vad.reset();

        assert!(
            vad.noise_floor() > 0.04,
            "фон не измерен: {}",
            vad.noise_floor()
        );
        assert_eq!(
            vad.push_frame(&noise),
            VadEvent::Silence,
            "шум комнаты всё ещё принимается за речь при пороге {}",
            vad.start_threshold()
        );
    }

    #[test]
    fn noise_estimate_ignores_occasional_dips() {
        // Фон гуляет: провалы до 0.01, обычный уровень 0.04. Оценка должна
        // держаться обычного уровня, а не цепляться за провалы.
        let mut vad = EnergyVad::default();
        for i in 0..100 {
            let level = if i % 10 == 0 { 0.01 } else { 0.04 };
            vad.push_frame(&vec![level; FRAME]);
        }
        assert!(
            vad.noise_floor() > 0.02,
            "оценка провалилась до {}",
            vad.noise_floor()
        );
    }

    #[test]
    fn speech_still_breaks_through_the_raised_threshold() {
        let noise = vec![0.047_f32; FRAME];
        let mut vad = EnergyVad::default();
        for _ in 0..60 {
            vad.push_frame(&noise);
        }
        vad.reset();

        // Обычная речь громче фона и обязана быть услышанной.
        assert_eq!(vad.push_frame(&vec![0.2; FRAME]), VadEvent::SpeechStart);
    }

    #[test]
    fn threshold_never_climbs_above_ordinary_speech() {
        // Очень шумное окружение не должно делать Yuki глухой.
        let mut vad = EnergyVad::default();
        for _ in 0..200 {
            vad.push_frame(&vec![0.5; FRAME]);
        }
        assert!(
            vad.start_threshold() <= 0.12,
            "порог уполз до {}",
            vad.start_threshold()
        );
    }

    #[test]
    fn reset_forgets_an_unfinished_phrase() {
        let mut vad = warmed();
        vad.push_frame(&loud());
        vad.reset();
        // После сброса громкий кадр снова начинает фразу, а не продолжает старую.
        assert_eq!(vad.push_frame(&loud()), VadEvent::SpeechStart);
    }
}
