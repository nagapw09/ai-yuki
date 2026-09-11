//! Детектор слова пробуждения (ТЗ §37: отзыв за 300 мс).
//!
//! # Почему сравнение с образцами, а не нейросеть
//!
//! Готовых моделей пробуждения для слова «Юки» не существует — открытые наборы
//! обучены на «Alexa», «Hey Jarvis» и подобном. Обучить свою — это отдельный
//! конвейер с тысячами записей, которых у одного человека нет.
//!
//! Зато у Yuki есть то, чего нет у массового ассистента: **она знает своего
//! пользователя**. Личный ассистент слушает один голос, и для одного голоса
//! сравнение с записанными им же образцами работает лучше, чем универсальная
//! модель — и не требует ни моделей в дистрибутиве, ни обучения.
//!
//! # Чего этот способ не умеет
//!
//! Он **зависит от голоса**: чужой человек, сказавший «Юки», скорее всего не
//! будет услышан, а сам пользователь с простудой — может не быть. Это цена
//! отказа от обучения, и называть её надо прямо.
//!
//! Он также требует записать слово при настройке. Без записанных образцов
//! детектор не притворяется работающим — он честно выключен, и голосовой режим
//! возвращается к прежнему пути (распознать фразу целиком и найти «Юки» в
//! тексте).
//!
//! # Откуда берутся 300 мс
//!
//! Окно анализа сдвигается каждые 96 мс (шесть кадров захвата по 16 мс), и
//! проверка одного окна против трёх образцов занимает единицы миллисекунд.
//! Отзыв наступает в пределах одного шага после конца слова, то есть заметно
//! быстрее, чем человек успевает договорить следующее.

use serde::{Deserialize, Serialize};

use crate::mfcc::{self, COEFFS, FRAME_HOP, FRAME_LEN, SAMPLE_RATE};

/// Сколько звука держать в окне анализа.
///
/// 1.2 секунды: слово «Юки» занимает около полусекунды, и запас нужен на то,
/// чтобы слово целиком попало в окно при любом выравнивании.
const WINDOW_SECONDS: f32 = 1.2;

/// Как часто проверять окно, в кадрах MFCC (10 мс каждый).
///
/// Реже — растёт задержка отзыва, чаще — процессор греется без пользы: за 50 мс
/// речь меняется незначительно.
const CHECK_EVERY_FRAMES: usize = 6;

/// Сколько образцов нужно записать.
///
/// Три: по одному нельзя оценить, насколько слово вообще похоже на себя, а
/// больше пяти человек записывать не станет.
pub const ENROLL_SAMPLES: usize = 3;

/// Насколько путь выравнивания может отходить от диагонали, в кадрах.
///
/// Двадцать кадров — это 200 мс перекоса на слове длиной полсекунды: с запасом
/// покрывает разницу темпа между «Юки» сказанным быстро и вразвалку.
const BAND: usize = 20;

/// Запас к порогу, вычисленному по образцам.
///
/// Порог берётся как худшее расстояние между собственными образцами,
/// умноженное на этот коэффициент. Меньше — детектор не узнаёт хозяина в
/// плохой день; больше — отзывается на постороннюю речь.
const THRESHOLD_MARGIN: f32 = 1.35;

/// Записанные образцы слова пробуждения.
///
/// Хранится в настройках: это не секрет, но и не то, что стоит показывать —
/// набор чисел, из которых звук не восстановить.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WakeModel {
    /// Последовательности признаков по одному на образец.
    pub templates: Vec<Vec<[f32; COEFFS]>>,
    /// Порог расстояния, посчитанный при записи.
    pub threshold: f32,
}

impl WakeModel {
    /// Собирает модель из записанных образцов.
    ///
    /// Порог вычисляется, а не задаётся константой: у разных людей слово звучит
    /// с разной устойчивостью, и единое число подошло бы не всем.
    pub fn from_samples(samples: &[Vec<f32>]) -> Option<Self> {
        // Образец — предмет сравнения целиком, поэтому его база считается
        // по нему самому.
        let templates: Vec<Vec<[f32; COEFFS]>> = samples
            .iter()
            .map(|audio| mfcc::extract_normalised(audio))
            .filter(|frames| !frames.is_empty())
            .collect();

        if templates.len() < 2 {
            // По одному образцу порог не оценить: сравнивать не с чем.
            return None;
        }

        // Худшее расстояние между собственными образцами — это разброс самого
        // человека. Всё, что дальше него с запасом, — уже другое слово.
        let mut worst = 0.0f32;
        for (index, first) in templates.iter().enumerate() {
            for second in templates.iter().skip(index + 1) {
                // Тем же способом, каким будет сравниваться поток: иначе порог
                // подошёл бы к другой мерке.
                worst = worst.max(best_alignment(first, second));
            }
        }

        Some(Self {
            templates,
            threshold: worst * THRESHOLD_MARGIN,
        })
    }
}

/// Нормированное расстояние между двумя последовательностями признаков.
///
/// Динамическое выравнивание (DTW): одно и то же слово, сказанное быстрее или
/// медленнее, — это те же звуки в том же порядке, и сравнивать их надо с
/// растяжением по времени, а не кадр в кадр.
pub fn distance(a: &[[f32; COEFFS]], b: &[[f32; COEFFS]]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return f32::INFINITY;
    }

    // Две строки матрицы вместо всей: полная матрица для 120×120 кадров — это
    // 57 килобайт на каждую проверку сто раз в секунду.
    let mut previous = vec![f32::INFINITY; b.len() + 1];
    let mut current = vec![f32::INFINITY; b.len() + 1];
    previous[0] = 0.0;

    for (i, frame_a) in a.iter().enumerate() {
        current[0] = f32::INFINITY;

        for (j, frame_b) in b.iter().enumerate() {
            let cost = frame_distance(frame_a, frame_b);
            let best = previous[j].min(previous[j + 1]).min(current[j]);
            current[j + 1] = cost + best;
        }

        std::mem::swap(&mut previous, &mut current);
        let _ = i;
    }

    // Нормируем на длину пути: иначе длинные последовательности всегда
    // «дальше» коротких просто потому, что слагаемых больше.
    previous[b.len()] / (a.len() + b.len()) as f32
}

fn frame_distance(a: &[f32; COEFFS], b: &[f32; COEFFS]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<f32>()
        .sqrt()
}

/// То же расстояние, но из кадров окна на лету вычитается база `mean`.
///
/// Смысл в том, чтобы не копировать участок ради нормализации: участков в окне
/// два десятка, и каждое копирование с пересчётом среднего — это работа, которую
/// можно не делать.
fn distance_shifted(
    window: &[[f32; COEFFS]],
    mean: &[f32; COEFFS],
    template: &[[f32; COEFFS]],
) -> f32 {
    if window.is_empty() || template.is_empty() {
        return f32::INFINITY;
    }

    let mut previous = vec![f32::INFINITY; template.len() + 1];
    let mut current = vec![f32::INFINITY; template.len() + 1];
    previous[0] = 0.0;

    for (i, frame) in window.iter().enumerate() {
        current[0] = f32::INFINITY;

        // Полоса Сакоэ–Чибы: путь выравнивания не должен уходить от диагонали
        // дальше, чем на BAND кадров. Растяжение речи вдвое — это норма,
        // растяжение в пять раз — это уже не то же слово, и считать такие
        // клетки значит платить за пути, которые всё равно не выберут.
        let from = i.saturating_sub(BAND);
        let to = (i + BAND + 1).min(template.len());

        for (j, reference) in template.iter().enumerate().take(to).skip(from) {
            let cost = frame
                .iter()
                .zip(mean)
                .zip(reference)
                .map(|((value, base), target)| {
                    let shifted = value - base - target;
                    shifted * shifted
                })
                .sum::<f32>()
                .sqrt();

            let best = previous[j].min(previous[j + 1]).min(current[j]);
            current[j + 1] = cost + best;
        }

        // Клетки вне полосы должны остаться недостижимыми на следующем шаге.
        for slot in current.iter_mut().take(from + 1) {
            *slot = f32::INFINITY;
        }
        for slot in current.iter_mut().skip(to + 1) {
            *slot = f32::INFINITY;
        }
        current[0] = f32::INFINITY;

        std::mem::swap(&mut previous, &mut current);
    }

    previous[template.len()] / (window.len() + template.len()) as f32
}

/// Сколько кадров признаков держится в окне.
fn window_frames() -> usize {
    ((SAMPLE_RATE * WINDOW_SECONDS) as usize - FRAME_LEN) / FRAME_HOP + 1
}

/// Скользящий детектор.
///
/// Держит окно **признаков**, а не звука: поток приходит кусками по 20 мс, и
/// за один шаг проверки окно меняется на десятую часть. Считать признаки всего
/// окна заново на каждой проверке — это в десять раз больше преобразований
/// Фурье, чем нужно, и именно на них уходило бы всё процессорное время.
pub struct WakeDetector {
    model: WakeModel,
    extractor: mfcc::FrameExtractor,
    /// Хвост звука, из которого ещё не собран кадр.
    tail: Vec<f32>,
    /// Готовые кадры признаков, старые в начале.
    frames: std::collections::VecDeque<[f32; COEFFS]>,
    /// Сколько кадров добавилось с прошлой проверки.
    since_check: usize,
    /// Сколько кадров игнорировать после срабатывания.
    cooldown: usize,
}

impl WakeDetector {
    pub fn new(model: WakeModel) -> Self {
        Self {
            model,
            extractor: mfcc::FrameExtractor::new(),
            tail: Vec::with_capacity(FRAME_LEN + FRAME_HOP),
            frames: std::collections::VecDeque::with_capacity(window_frames() + 1),
            since_check: 0,
            cooldown: 0,
        }
    }

    /// Добавляет кусок звука и говорит, услышано ли обращение.
    ///
    /// Возвращает `true` ровно один раз на одно произнесение: после
    /// срабатывания детектор молчит, пока окно не сменится целиком. Иначе одно
    /// слово дало бы десяток срабатываний подряд — оно остаётся в окне ещё
    /// секунду.
    pub fn push(&mut self, samples: &[f32]) -> bool {
        self.tail.extend_from_slice(samples);

        let capacity = window_frames();
        let mut added = 0;

        // Из хвоста собираем столько кадров, сколько в нём поместилось. Кадр
        // длиной 25 мс, шаг 10 мс — значит, перекрытие остаётся в хвосте.
        while self.tail.len() >= FRAME_LEN {
            let frame = self.extractor.frame(&self.tail[..FRAME_LEN]);
            self.tail.drain(..FRAME_HOP);

            if self.frames.len() == capacity {
                self.frames.pop_front();
            }
            self.frames.push_back(frame);
            added += 1;
        }

        if self.cooldown > 0 {
            self.cooldown = self.cooldown.saturating_sub(added);
            return false;
        }

        self.since_check += added;
        if self.since_check < CHECK_EVERY_FRAMES {
            return false;
        }
        self.since_check = 0;

        // Пока окно не набралось, сравнивать не с чем: короткий кусок даст
        // маленькое расстояние просто потому, что в нём мало кадров.
        if self.frames.len() < capacity / 2 {
            return false;
        }

        let window: Vec<[f32; COEFFS]> = self.frames.iter().copied().collect();

        let heard = self
            .model
            .templates
            .iter()
            .any(|template| best_alignment(&window, template) <= self.model.threshold);

        if heard {
            self.cooldown = capacity;
            self.frames.clear();
            self.tail.clear();
        }

        heard
    }

    /// Порог модели — для диагностики.
    pub fn threshold(&self) -> f32 {
        self.model.threshold
    }
}

/// Лучшее совпадение образца с любым участком окна.
///
/// Образец короче окна, и сравнивать его с окном целиком нельзя: тишина вокруг
/// слова добавила бы к расстоянию столько, что совпадение утонуло бы. Поэтому
/// окно просматривается участками длины образца.
///
/// Каждый участок приводится к своей базе перед сравнением — это и есть причина
/// брать участки, а не окно: у окна с тишиной вокруг слова база другая, и
/// нормализованный по ней фрагмент перестаёт совпадать с образцом.
pub fn best_alignment(window: &[[f32; COEFFS]], template: &[[f32; COEFFS]]) -> f32 {
    if template.is_empty() || window.is_empty() {
        return f32::INFINITY;
    }

    if window.len() <= template.len() {
        let mut piece = window.to_vec();
        mfcc::normalise(&mut piece);
        return distance(&piece, template);
    }

    // Накопленные суммы по кадрам: среднее любого участка считается по двум
    // точкам вместо обхода участка целиком.
    let mut prefix = vec![[0.0f32; COEFFS]; window.len() + 1];
    for (index, frame) in window.iter().enumerate() {
        // Две соседние строки одновременно: split_at_mut разделяет заимствование,
        // иначе компилятор справедливо видит одновременное чтение и запись.
        let (head, tail) = prefix.split_at_mut(index + 1);
        let previous = &head[index];
        for (slot, (sum, value)) in tail[0].iter_mut().zip(previous.iter().zip(frame)) {
            *slot = sum + value;
        }
    }

    // Шаг в три кадра (30 мс): DTW и так терпима к сдвигу, а проверять каждый
    // кадр — втрое больше работы без выигрыша в точности.
    let step = 3;
    let length = template.len();
    let mut best = f32::INFINITY;
    let mut start = 0;

    while start + length <= window.len() {
        let mut mean = [0.0f32; COEFFS];
        for (slot, (end, begin)) in mean
            .iter_mut()
            .zip(prefix[start + length].iter().zip(&prefix[start]))
        {
            *slot = (end - begin) / length as f32;
        }

        best = best.min(distance_shifted(
            &window[start..start + length],
            &mean,
            template,
        ));
        start += step;
    }

    best
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Синтетическое «слово»: два тона подряд, как гласная и согласная.
    fn word(first: f32, second: f32, seed: f32) -> Vec<f32> {
        let length = (SAMPLE_RATE * 0.5) as usize;
        (0..length)
            .map(|index| {
                let time = index as f32 / SAMPLE_RATE;
                let frequency = if index < length / 2 { first } else { second };
                (2.0 * std::f32::consts::PI * frequency * time + seed).sin() * 0.3
            })
            .collect()
    }

    #[test]
    fn a_sequence_is_closest_to_itself() {
        let frames = mfcc::extract_normalised(&word(400.0, 900.0, 0.0));
        assert!(distance(&frames, &frames) < 1e-3);
    }

    #[test]
    fn the_same_word_said_again_is_closer_than_a_different_one() {
        let first = mfcc::extract_normalised(&word(400.0, 900.0, 0.0));
        let again = mfcc::extract_normalised(&word(400.0, 900.0, 0.7));
        let other = mfcc::extract_normalised(&word(1200.0, 300.0, 0.0));

        let same = distance(&first, &again);
        let different = distance(&first, &other);

        assert!(
            same < different,
            "повтор дальше чужого слова: {same} против {different}"
        );
    }

    #[test]
    fn alignment_survives_a_change_of_tempo() {
        // Растяжение по времени — это то, ради чего здесь DTW, а не сравнение
        // кадр в кадр.
        let normal = mfcc::extract_normalised(&word(400.0, 900.0, 0.0));

        let slow: Vec<f32> = word(400.0, 900.0, 0.0)
            .iter()
            .flat_map(|sample| [*sample, *sample])
            .collect();
        let stretched = mfcc::extract_normalised(&slow);

        let same = distance(&normal, &stretched);
        let other = distance(&normal, &mfcc::extract_normalised(&word(1200.0, 300.0, 0.0)));

        assert!(same < other, "растянутое слово дальше чужого: {same} / {other}");
    }

    #[test]
    fn an_empty_sequence_is_infinitely_far() {
        let frames = mfcc::extract_normalised(&word(400.0, 900.0, 0.0));
        assert_eq!(distance(&frames, &[]), f32::INFINITY);
        assert_eq!(distance(&[], &frames), f32::INFINITY);
    }

    #[test]
    fn a_single_sample_is_not_enough_to_build_a_model() {
        // По одному образцу порог не оценить — сравнивать не с чем.
        assert!(WakeModel::from_samples(&[word(400.0, 900.0, 0.0)]).is_none());
        assert!(WakeModel::from_samples(&[]).is_none());
    }

    #[test]
    fn a_model_from_similar_samples_gets_a_tight_threshold() {
        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.3),
            word(400.0, 900.0, 0.6),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");

        assert_eq!(model.templates.len(), 3);
        assert!(model.threshold > 0.0);
        assert!(model.threshold.is_finite());
    }

    #[test]
    fn finds_the_word_inside_a_longer_window() {
        // Слово окружено тишиной: сравнение с окном целиком утопило бы его,
        // поэтому окно просматривается участками.
        let template = mfcc::extract_normalised(&word(400.0, 900.0, 0.0));

        let mut audio = vec![0.0f32; (SAMPLE_RATE * 0.3) as usize];
        audio.extend(word(400.0, 900.0, 0.2));
        audio.extend(vec![0.0f32; (SAMPLE_RATE * 0.3) as usize]);
        let window = mfcc::extract(&audio);

        let aligned = best_alignment(&window, &template);
        let whole = distance(&window, &template);

        assert!(
            aligned < whole,
            "участок не лучше целого окна: {aligned} против {whole}"
        );
    }

    #[test]
    fn the_detector_hears_the_enrolled_word() {
        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.25),
            word(400.0, 900.0, 0.5),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");
        let mut detector = WakeDetector::new(model);

        // Кормим так, как приходит с микрофона: кусками по 16 мс.
        let mut audio = vec![0.0f32; (SAMPLE_RATE * 0.4) as usize];
        audio.extend(word(400.0, 900.0, 0.1));
        audio.extend(vec![0.0f32; (SAMPLE_RATE * 0.4) as usize]);

        let chunk = (SAMPLE_RATE * 0.016) as usize;
        let heard = audio.chunks(chunk).any(|piece| detector.push(piece));

        assert!(heard, "детектор не узнал записанное слово");
    }

    #[test]
    fn the_detector_stays_silent_on_a_different_word() {
        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.25),
            word(400.0, 900.0, 0.5),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");
        let mut detector = WakeDetector::new(model);

        let mut audio = vec![0.0f32; (SAMPLE_RATE * 0.4) as usize];
        audio.extend(word(1500.0, 250.0, 0.0));
        audio.extend(vec![0.0f32; (SAMPLE_RATE * 0.4) as usize]);

        let chunk = (SAMPLE_RATE * 0.016) as usize;
        let heard = audio.chunks(chunk).any(|piece| detector.push(piece));

        assert!(!heard, "детектор отозвался на чужое слово");
    }

    #[test]
    fn silence_never_wakes_it() {
        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.25),
            word(400.0, 900.0, 0.5),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");
        let mut detector = WakeDetector::new(model);

        let silence = vec![0.0f32; (SAMPLE_RATE * 3.0) as usize];
        let chunk = (SAMPLE_RATE * 0.016) as usize;

        assert!(!silence.chunks(chunk).any(|piece| detector.push(piece)));
    }

    #[test]
    fn one_utterance_fires_once() {
        // Слово остаётся в окне ещё секунду: без выдержки оно дало бы десяток
        // срабатываний подряд.
        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.25),
            word(400.0, 900.0, 0.5),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");
        let mut detector = WakeDetector::new(model);

        let mut audio = vec![0.0f32; (SAMPLE_RATE * 0.4) as usize];
        audio.extend(word(400.0, 900.0, 0.1));
        audio.extend(vec![0.0f32; (SAMPLE_RATE * 1.5) as usize]);

        let chunk = (SAMPLE_RATE * 0.016) as usize;
        let fired = audio
            .chunks(chunk)
            .filter(|piece| detector.push(piece))
            .count();

        assert_eq!(fired, 1, "сработало {fired} раз вместо одного");
    }

    #[test]
    fn checking_a_window_is_fast_enough_for_the_budget() {
        // ТЗ §37 отводит на отзыв 300 мс, и проверка обязана быть дешевле шага
        // в 96 мс — иначе детектор не успевает за потоком.
        //
        // В отладочной сборке арифметика идёт без оптимизаций и медленнее в
        // разы, поэтому здесь бюджет свой: тест сторожит не абсолютное время, а
        // отсутствие катастрофы вроде копирования матрицы на каждый шаг.
        // Настоящее время меряется в release — им же и проверялось.
        let budget = if cfg!(debug_assertions) { 400 } else { 96 };

        let samples = vec![
            word(400.0, 900.0, 0.0),
            word(400.0, 900.0, 0.25),
            word(400.0, 900.0, 0.5),
        ];
        let model = WakeModel::from_samples(&samples).expect("модель должна собраться");

        let audio = vec![0.1f32; (SAMPLE_RATE * WINDOW_SECONDS) as usize];
        let frames = mfcc::extract(&audio);

        let started = std::time::Instant::now();
        for template in &model.templates {
            let _ = best_alignment(&frames, template);
        }
        let elapsed = started.elapsed();

        assert!(
            elapsed.as_millis() < budget,
            "проверка окна заняла {elapsed:?} при бюджете {budget} мс"
        );
    }
}
