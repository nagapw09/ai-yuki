//! Слово пробуждения: детектор rustpotter.
//!
//! # Почему не собственная реализация
//!
//! Своя ([`crate::wake`]) сравнивает признаки записанных образцов с окном
//! потока динамическим выравниванием — тот же принцип. Разница в мелочах,
//! которые и решают, услышит ли она хозяина в обычной комнате:
//!
//! * **усиление.** Один и тот же человек говорит то громче, то тише, и на
//!   разном расстоянии от микрофона. rustpotter приводит громкость окна к
//!   громкости образцов перед сравнением; наша реализация сравнивала как есть.
//!   Насколько это помогает тихому «Юки» из другого конца комнаты, без живого
//!   микрофона не измерить — проверено только то, что вреда от этого нет.
//! * **усреднённый образец.** Сначала окно сравнивается с одним усреднённым
//!   образцом, и только прошедшее сравнивается с каждым по отдельности —
//!   дешёвая отсечка перед дорогой проверкой.
//! * **несколько подтверждений.** Совпадение объявляется не по одному окну, а
//!   по нескольким подряд, и из них берётся лучшее. Случайный всплеск в чужом
//!   разговоре одно окно пройти может, пять подряд — нет. Это и есть ответ на
//!   срабатывание посреди чужого разговора.
//!
//! # Что это дало
//!
//! `wake_speech` записывает обращение системным голосом и прогоняет через оба
//! детектора четыре обращения и одиннадцать посторонних фраз — чужой разговор
//! и её собственные ответы:
//!
//! ```text
//!                          услышала обращений   ложных отзывов
//! сравнение с образцами          4 из 4            7 из 11
//! rustpotter                     4 из 4            0 из 11
//! ```
//!
//! Семь ложных отзывов из одиннадцати — это и есть жалоба «взяло одно слово из
//! чужого разговора и начало говорить», воспроизведённая без микрофона.
//! Прежний детектор просыпался даже на «Готово, открыла браузер» — на своём
//! собственном ответе.
//!
//! Чужим голосом rustpotter не слышит обращение вовсе (0 из 4). Для личного
//! ассистента это скорее свойство, чем недостаток: чужой человек не разбудит
//! его, сказав «Юки». Но означает оно и то, что записать слово должен тот, кто
//! будет обращаться.
//!
//! # Чего это стоит
//!
//! Дороже прежнего пути втрое: `wake_bench` на этой машине даёт 8,4 мс
//! процессора на секунду звука против 2,6 мс. В долях это 0,84 % одного ядра
//! против 0,26 % — то есть на четырёхъядерной машине из минимальных требований
//! пятая часть процента, круглосуточно. Названная цена за то, чтобы не
//! просыпаться от чужого разговора, принята сознательно; окно проверки шире
//! этого запаса в разы, и поводов экономить здесь нет.
//!
//! # Что осталось прежним
//!
//! Эталон по-прежнему строится из трёх записей самого пользователя: готовых
//! моделей для слова «Юки» не существует, а обучать нейросеть на трёх записях
//! нечем. Значит, зависимость от голоса никуда не делась — чужой человек,
//! сказавший «Юки», скорее всего услышан не будет. Это цена личного
//! ассистента, и называть её надо прямо.
//!
//! Ни сети, ни ключей, ни моделей в дистрибутиве: всё считается на месте
//! (ТЗ §29).

use std::collections::HashMap;

use rustpotter::{
    Rustpotter, RustpotterConfig, ScoreMode, WakewordLoad, WakewordRef,
    WakewordRefBuildFromBuffers, WakewordSave,
};

use crate::mfcc::SAMPLE_RATE;

/// Имя эталона. Видно только в отладочном выводе детектора.
const WAKE_NAME: &str = "yuki";

/// Сколько кепстральных коэффициентов брать с кадра.
///
/// Столько же берёт `rustpotter-cli` по умолчанию. Значение зашито в эталон:
/// менять его, не перезаписав слово, нельзя.
const MFCC_SIZE: u16 = 16;

/// Порог совпадения, 0…1.
///
/// Умолчание rustpotter. Ниже — начинает отзываться на похожие слова, выше —
/// перестаёт узнавать хозяина, когда тот говорит быстрее обычного.
pub const THRESHOLD: f32 = 0.5;

/// Порог по усреднённому образцу — дешёвая отсечка перед полным сравнением.
const AVG_THRESHOLD: f32 = 0.2;

/// Сколько окон подряд должны совпасть, прежде чем объявить обращение.
///
/// Умолчание rustpotter. Это и есть защита от случайного совпадения в чужом
/// разговоре: одно окно пройти может, пять подряд — практически нет.
const MIN_SCORES: usize = 5;

/// Насколько тише пика считается тишиной при обрезке образца.
const TRIM_FLOOR_RATIO: f32 = 0.15;

/// Абсолютный пол обрезки: тише этого — тишина при любом пике.
const TRIM_FLOOR: f32 = 0.01;

/// Окно, которым измеряется громкость при обрезке.
const TRIM_WINDOW_MS: usize = 10;

/// Сколько тишины оставить вокруг слова.
///
/// Совсем впритык обрезать нельзя: у «Ю» тихое начало, и срезанный призвук
/// меняет признаки первых кадров сильнее, чем кажется.
const TRIM_PAD_MS: usize = 80;

/// Обрезает тишину вокруг слова.
///
/// Образец записывается две секунды, а слово в нём занимает полсекунды.
/// rustpotter строит окно сравнения по длине образца: если оставить тишину,
/// детектор будет ждать двухсекундное окно и сравнивать слово с четвертью
/// этого окна, утопив совпадение в тишине вокруг.
pub fn trim(samples: &[f32]) -> &[f32] {
    let window = (SAMPLE_RATE as usize / 1000) * TRIM_WINDOW_MS;
    if samples.len() <= window * 2 {
        return samples;
    }

    let loudness: Vec<f32> = samples
        .chunks(window)
        .map(|chunk| (chunk.iter().map(|s| s * s).sum::<f32>() / chunk.len() as f32).sqrt())
        .collect();

    let peak = loudness.iter().copied().fold(0.0f32, f32::max);
    let floor = (peak * TRIM_FLOOR_RATIO).max(TRIM_FLOOR);

    let first = loudness.iter().position(|&level| level > floor);
    let last = loudness.iter().rposition(|&level| level > floor);
    let (Some(first), Some(last)) = (first, last) else {
        // Ничего громче пола — обрезать нечего, пусть разбирается вызывающий.
        return samples;
    };

    let pad = TRIM_PAD_MS / TRIM_WINDOW_MS;
    let from = first.saturating_sub(pad) * window;
    let to = ((last + 1 + pad) * window).min(samples.len());
    &samples[from..to]
}

/// Собирает эталон из записанных образцов и возвращает его в формате rustpotter.
///
/// Возвращаются байты, а не структура: они кладутся в настройки как есть, и
/// приложению не нужно знать, что у эталона внутри.
pub fn build(samples: &[Vec<f32>]) -> Result<Vec<u8>, String> {
    if samples.len() < 2 {
        // По одному образцу усреднённый эталон не построить, а без него
        // дешёвая отсечка не работает.
        return Err("нужно хотя бы два образца".into());
    }

    let mut buffers: HashMap<String, Vec<u8>> = HashMap::new();
    for (index, audio) in samples.iter().enumerate() {
        let trimmed = trim(audio);
        if trimmed.len() < SAMPLE_RATE as usize / 10 {
            return Err(format!("образец {} короче 100 мс", index + 1));
        }
        buffers.insert(format!("sample-{index}.wav"), wav(trimmed)?);
    }

    let wakeword = WakewordRef::new_from_sample_buffers(
        WAKE_NAME.to_string(),
        Some(THRESHOLD),
        Some(AVG_THRESHOLD),
        buffers,
        MFCC_SIZE,
    )?;

    wakeword.save_to_buffer()
}

/// Кодирует отрезок в WAV: строитель эталона принимает только его.
fn wav(samples: &[f32]) -> Result<Vec<u8>, String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut out = std::io::Cursor::new(Vec::<u8>::new());
    {
        let mut writer = hound::WavWriter::new(&mut out, spec).map_err(|e| e.to_string())?;
        for &sample in samples {
            let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(value).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
    }
    Ok(out.into_inner())
}

/// Скользящий детектор поверх rustpotter.
///
/// Поток приходит кадрами по 20 мс, а rustpotter принимает ровно свои 30 мс,
/// поэтому кадры пересобираются здесь.
pub struct Spotter {
    detector: Rustpotter,
    /// Звук, которого не хватило на целый кадр rustpotter.
    pending: Vec<f32>,
    /// Сколько отсчётов он ждёт за раз.
    per_frame: usize,
    /// Совпадение последнего срабатывания — для диагностики.
    last_score: f32,
    /// Порог, с которым собран детектор.
    threshold: f32,
}

impl Spotter {
    /// Разворачивает сохранённый эталон.
    pub fn new(reference: &[u8]) -> Result<Self, String> {
        Self::with_threshold(reference, THRESHOLD)
    }

    /// То же, но с другим порогом.
    ///
    /// Нужен `wake_speech`: подобрать порог можно, только измерив, сколько
    /// обращений теряется и сколько чужих фраз проходит на каждом значении.
    pub fn with_threshold(reference: &[u8], threshold: f32) -> Result<Self, String> {
        let wakeword = WakewordRef::load_from_buffer(reference)?;

        let mut config = RustpotterConfig::default();
        // Формат по умолчанию — ровно наш: 16 кГц, моно, f32.
        config.detector.threshold = threshold;
        config.detector.avg_threshold = AVG_THRESHOLD;
        config.detector.min_scores = MIN_SCORES;
        config.detector.score_mode = ScoreMode::Max;
        // Приводит громкость окна к громкости образцов: хозяин говорит то
        // ближе, то дальше от микрофона, и без этого тихое «Юки» не доходит.
        config.filters.gain_normalizer.enabled = true;
        // Собственная проверка на речь не включается: она молчит первые
        // полсекунды после тишины, а «Юки» столько и длится — первое же
        // обращение было бы пропущено.

        let mut detector = Rustpotter::new(&config)?;
        detector.add_wakeword_ref(WAKE_NAME, wakeword)?;
        let per_frame = detector.get_samples_per_frame();

        Ok(Self {
            detector,
            pending: Vec::with_capacity(per_frame * 2),
            per_frame,
            last_score: 0.0,
            threshold,
        })
    }

    /// Добавляет кусок звука и говорит, услышано ли обращение.
    pub fn push(&mut self, samples: &[f32]) -> bool {
        self.pending.extend_from_slice(samples);

        let mut heard = false;
        while self.pending.len() >= self.per_frame {
            let frame: Vec<f32> = self.pending.drain(..self.per_frame).collect();
            if let Some(detection) = self.detector.process_samples(frame) {
                self.last_score = detection.score;
                heard = true;
            }
        }
        heard
    }

    /// Совпадение последнего срабатывания — для диагностики.
    pub fn score(&self) -> f32 {
        self.last_score
    }

    /// Порог, с которым сравнивается совпадение.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Полсекунды «слова»: две гармоники с затуханием.
    fn word(seed: f32) -> Vec<f32> {
        let length = (SAMPLE_RATE * 0.5) as usize;
        (0..length)
            .map(|index| {
                let time = index as f32 / SAMPLE_RATE;
                let envelope = (time * 6.0).min(1.0) * (1.0 - time * 0.8);
                ((time * (220.0 + seed * 30.0) * std::f32::consts::TAU).sin() * 0.6
                    + (time * (660.0 + seed * 40.0) * std::f32::consts::TAU).sin() * 0.4)
                    * envelope
            })
            .collect()
    }

    fn silence(seconds: f32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE * seconds) as usize]
    }

    #[test]
    fn trimming_keeps_the_word_and_drops_the_silence() {
        let mut audio = silence(0.7);
        audio.extend_from_slice(&word(0.0));
        audio.extend_from_slice(&silence(0.8));

        let trimmed = trim(&audio);

        // Слово длится полсекунды, к нему добавляется по 80 мс с каждой
        // стороны — всё остальное лишнее.
        let expected = (SAMPLE_RATE * (0.5 + 0.16)) as usize;
        assert!(
            trimmed.len() < audio.len() / 2,
            "тишина осталась: {} из {}",
            trimmed.len(),
            audio.len()
        );
        assert!(
            trimmed.len() as f32 > expected as f32 * 0.8,
            "срезано само слово: {} при ожидаемых {expected}",
            trimmed.len()
        );
    }

    #[test]
    fn trimming_leaves_silence_alone() {
        // В тишине резать нечего: пусть вызывающий сам решает, что с ней делать.
        let audio = silence(1.0);
        assert_eq!(trim(&audio).len(), audio.len());
    }

    #[test]
    fn a_single_sample_is_not_enough() {
        assert!(build(&[word(0.0)]).is_err());
    }

    #[test]
    fn a_reference_survives_saving_and_loading() {
        let samples: Vec<Vec<f32>> = (0..3)
            .map(|index| {
                let mut audio = silence(0.3);
                audio.extend_from_slice(&word(index as f32 * 0.1));
                audio.extend_from_slice(&silence(0.3));
                audio
            })
            .collect();

        let reference = build(&samples).expect("эталон не собрался");
        assert!(!reference.is_empty());

        let spotter = Spotter::new(&reference).expect("эталон не развернулся");
        assert_eq!(spotter.threshold(), THRESHOLD);
    }

    #[test]
    fn silence_never_wakes_it() {
        let samples: Vec<Vec<f32>> = (0..3).map(|index| word(index as f32 * 0.1)).collect();
        let reference = build(&samples).expect("эталон не собрался");
        let mut spotter = Spotter::new(&reference).expect("эталон не развернулся");

        let quiet = vec![0.0f32; 320];
        for _ in 0..200 {
            assert!(!spotter.push(&quiet), "детектор услышал тишину");
        }
    }
}
