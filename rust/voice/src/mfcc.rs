//! Признаки речи: MFCC (ТЗ §10, §37).
//!
//! # Зачем это здесь
//!
//! Слово пробуждения должно отрабатывать за 300 мс (ТЗ §37). Путь «дослушать
//! фразу → отправить её на распознавание → поискать в тексте „Юки“» этого не
//! даёт в принципе: отзыв наступает после того, как человек договорил, плюс
//! время сети.
//!
//! Значит, обращение надо узнавать локально и по звуку. MFCC — стандартный
//! набор признаков для этого: он описывает форму спектра так, как её слышит
//! человек, и почти не зависит от громкости.
//!
//! # Почему без готовой библиотеки
//!
//! Весь конвейер — это оконное преобразование Фурье, мел-фильтры, логарифм и
//! дискретное косинусное преобразование. Сто строк арифметики, каждая из
//! которых проверяется тестом против прямого определения. Зависимость ради
//! этого добавила бы к дистрибутиву больше, чем весит весь код голосового
//! слоя.

/// Частота дискретизации, на которой работает весь голосовой конвейер.
pub const SAMPLE_RATE: f32 = 16_000.0;

/// Длина окна анализа в отсчётах — 25 мс.
///
/// Классическое значение: короче — спектр шумный, длиннее — окно перестаёт быть
/// стационарным и звуки смазываются друг в друга.
pub const FRAME_LEN: usize = 400;

/// Шаг между окнами — 10 мс.
pub const FRAME_HOP: usize = 160;

/// Размер преобразования Фурье: ближайшая степень двойки не меньше окна.
const FFT_SIZE: usize = 512;

/// Число мел-фильтров.
const FILTERS: usize = 26;

/// Сколько кепстральных коэффициентов оставляем.
///
/// Тринадцать: дальше идут детали, которые описывают уже не звук речи, а
/// особенности записи.
pub const COEFFS: usize = 13;

/// Нижняя и верхняя границы полосы анализа.
///
/// Ниже 300 Гц живёт гул сети и стук по столу, выше 8000 Гц на 16 кГц ничего
/// нет по теореме Котельникова.
const LOW_HZ: f32 = 300.0;
const HIGH_HZ: f32 = 8_000.0;

/// Комплексное число: своё, чтобы не тянуть зависимость ради двух операций.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Complex {
    re: f32,
    im: f32,
}

impl Complex {
    const ZERO: Self = Self { re: 0.0, im: 0.0 };

    fn magnitude(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }
}

/// Быстрое преобразование Фурье на месте, основание 2.
///
/// Реализация итеративная (Cooley–Tukey с перестановкой по обращению бит):
/// рекурсивная короче, но выделяет память на каждом уровне, а эта функция
/// вызывается сто раз в секунду.
fn fft(buffer: &mut [Complex]) {
    let n = buffer.len();
    debug_assert!(n.is_power_of_two(), "длина должна быть степенью двойки");

    // Перестановка по обращению битов индекса.
    let mut target = 0usize;
    for position in 1..n {
        let mut bit = n >> 1;
        while target & bit != 0 {
            target ^= bit;
            bit >>= 1;
        }
        target |= bit;

        if position < target {
            buffer.swap(position, target);
        }
    }

    let mut length = 2;
    while length <= n {
        let angle = -2.0 * std::f32::consts::PI / length as f32;

        for start in (0..n).step_by(length) {
            for offset in 0..length / 2 {
                let phase = angle * offset as f32;
                let (sin, cos) = phase.sin_cos();

                let even = buffer[start + offset];
                let odd = buffer[start + offset + length / 2];

                let rotated = Complex {
                    re: odd.re * cos - odd.im * sin,
                    im: odd.re * sin + odd.im * cos,
                };

                buffer[start + offset] = Complex {
                    re: even.re + rotated.re,
                    im: even.im + rotated.im,
                };
                buffer[start + offset + length / 2] = Complex {
                    re: even.re - rotated.re,
                    im: even.im - rotated.im,
                };
            }
        }

        length <<= 1;
    }
}

/// Перевод частоты в мелы и обратно.
///
/// Мел-шкала описывает, как человек слышит высоту: разница между 200 и 300 Гц
/// заметна, между 8000 и 8100 — нет.
fn to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

fn from_mel(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

/// Границы мел-фильтров в номерах частотных отсчётов.
fn filter_bounds() -> Vec<usize> {
    let low = to_mel(LOW_HZ);
    let high = to_mel(HIGH_HZ);

    (0..FILTERS + 2)
        .map(|index| {
            let mel = low + (high - low) * index as f32 / (FILTERS + 1) as f32;
            let hz = from_mel(mel);
            ((hz / SAMPLE_RATE) * FFT_SIZE as f32).round() as usize
        })
        .collect()
}

/// Счётчик признаков с заранее посчитанными границами фильтров.
///
/// Границы зависят только от частоты дискретизации, и пересчитывать их на
/// каждый кадр — работа впустую сто раз в секунду.
pub struct FrameExtractor {
    bounds: Vec<usize>,
}

impl Default for FrameExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameExtractor {
    pub fn new() -> Self {
        Self {
            bounds: filter_bounds(),
        }
    }

    /// Признаки одного окна. Нужно не меньше [`FRAME_LEN`] отсчётов.
    pub fn frame(&self, samples: &[f32]) -> [f32; COEFFS] {
        frame_coeffs(samples, &self.bounds)
    }
}

/// Считает MFCC одного окна.
fn frame_coeffs(samples: &[f32], bounds: &[usize]) -> [f32; COEFFS] {
    let mut buffer = [Complex::ZERO; FFT_SIZE];

    // Окно Хэмминга: обрыв сигнала на границе окна даёт в спектре ложные
    // высокие частоты, которых в звуке не было.
    for (index, sample) in samples.iter().take(FRAME_LEN).enumerate() {
        let window = 0.54
            - 0.46 * (2.0 * std::f32::consts::PI * index as f32 / (FRAME_LEN - 1) as f32).cos();
        buffer[index] = Complex {
            re: sample * window,
            im: 0.0,
        };
    }

    fft(&mut buffer);

    // Энергия в каждом мел-фильтре.
    let mut energies = [0.0f32; FILTERS];
    for (filter, energy) in energies.iter_mut().enumerate() {
        let left = bounds[filter];
        let center = bounds[filter + 1];
        let right = bounds[filter + 2];

        let mut sum = 0.0;
        for bin in left..=right.min(FFT_SIZE / 2) {
            // Треугольный вес: в центре фильтра 1, на краях 0.
            let weight = if bin <= center {
                if center == left {
                    1.0
                } else {
                    (bin - left) as f32 / (center - left) as f32
                }
            } else if right == center {
                1.0
            } else {
                (right - bin) as f32 / (right - center) as f32
            };

            sum += buffer[bin].magnitude() * weight;
        }

        // Логарифм с полом: тишина даёт ноль, а логарифм нуля — минус
        // бесконечность, которая портит всё дальше по конвейеру.
        *energy = (sum.max(1e-10)).ln();
    }

    // Дискретное косинусное преобразование: убирает корреляцию между
    // соседними фильтрами и оставляет форму спектра в первых коэффициентах.
    let mut coeffs = [0.0f32; COEFFS];
    for (index, coeff) in coeffs.iter_mut().enumerate() {
        let mut sum = 0.0;
        for (filter, energy) in energies.iter().enumerate() {
            sum += energy
                * (std::f32::consts::PI * index as f32 * (filter as f32 + 0.5) / FILTERS as f32)
                    .cos();
        }
        *coeff = sum;
    }

    coeffs
}

/// Считает последовательность MFCC для куска звука.
///
/// Возвращает по вектору на каждые 10 мс. Кусок короче одного окна даёт пустую
/// последовательность — это не ошибка, а «сказать пока нечего».
///
/// Коэффициенты **не нормализованы**: нормализация вычитает средний вектор, а
/// средний зависит от того, что именно попало в кусок. Окно с секундой тишины
/// вокруг слова и сам образец слова дают разную базу, и нормализовать их вместе
/// значит сравнивать разное. Поэтому базу выбирает тот, кто сравнивает, —
/// см. [`normalise`].
pub fn extract(samples: &[f32]) -> Vec<[f32; COEFFS]> {
    if samples.len() < FRAME_LEN {
        return Vec::new();
    }

    let bounds = filter_bounds();
    let mut frames = Vec::with_capacity((samples.len() - FRAME_LEN) / FRAME_HOP + 1);

    let mut start = 0;
    while start + FRAME_LEN <= samples.len() {
        frames.push(frame_coeffs(&samples[start..], &bounds));
        start += FRAME_HOP;
    }

    frames
}

/// Считает признаки и сразу приводит их к единой базе.
///
/// Годится там, где кусок и есть предмет сравнения целиком — например, для
/// записанного образца слова.
pub fn extract_normalised(samples: &[f32]) -> Vec<[f32; COEFFS]> {
    let mut frames = extract(samples);
    normalise(&mut frames);
    frames
}

/// Вычитает средний вектор по всей последовательности.
///
/// Микрофон, комната и расстояние до рта добавляют к спектру постоянную
/// составляющую. Без её вычитания образец, записанный вплотную к микрофону,
/// не совпадёт с тем же словом, сказанным с метра.
///
/// Вычитается средний **по переданному куску**: сравнивать две
/// последовательности можно только после приведения обеих к своей базе.
pub fn normalise(frames: &mut [[f32; COEFFS]]) {
    if frames.is_empty() {
        return;
    }

    let mut mean = [0.0f32; COEFFS];
    for frame in frames.iter() {
        for (sum, value) in mean.iter_mut().zip(frame) {
            *sum += value;
        }
    }
    for sum in mean.iter_mut() {
        *sum /= frames.len() as f32;
    }

    for frame in frames.iter_mut() {
        for (value, average) in frame.iter_mut().zip(&mean) {
            *value -= average;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Прямое определение преобразования Фурье — эталон для быстрого.
    fn naive_dft(input: &[Complex]) -> Vec<Complex> {
        let n = input.len();
        (0..n)
            .map(|k| {
                let mut sum = Complex::ZERO;
                for (index, value) in input.iter().enumerate() {
                    let angle = -2.0 * std::f32::consts::PI * (k * index) as f32 / n as f32;
                    let (sin, cos) = angle.sin_cos();
                    sum.re += value.re * cos - value.im * sin;
                    sum.im += value.re * sin + value.im * cos;
                }
                sum
            })
            .collect()
    }

    #[test]
    fn fast_transform_matches_the_definition() {
        // Цена ошибки здесь — молча неверные признаки, поэтому сверяем с
        // прямым определением, а не с ожиданиями автора.
        let mut input: Vec<Complex> = (0..64)
            .map(|index| Complex {
                re: (index as f32 * 0.37).sin() + (index as f32 * 1.11).cos(),
                im: 0.0,
            })
            .collect();

        let expected = naive_dft(&input);
        fft(&mut input);

        for (fast, slow) in input.iter().zip(&expected) {
            assert!((fast.re - slow.re).abs() < 1e-2, "{fast:?} vs {slow:?}");
            assert!((fast.im - slow.im).abs() < 1e-2, "{fast:?} vs {slow:?}");
        }
    }

    #[test]
    fn a_pure_tone_shows_up_in_its_own_bin() {
        let frequency = 1000.0;
        let mut buffer = [Complex::ZERO; FFT_SIZE];
        for (index, slot) in buffer.iter_mut().enumerate() {
            slot.re = (2.0 * std::f32::consts::PI * frequency * index as f32 / SAMPLE_RATE).sin();
        }

        fft(&mut buffer);

        let expected_bin = (frequency / SAMPLE_RATE * FFT_SIZE as f32).round() as usize;
        let loudest = (1..FFT_SIZE / 2)
            .max_by(|a, b| {
                buffer[*a]
                    .magnitude()
                    .partial_cmp(&buffer[*b].magnitude())
                    .unwrap()
            })
            .expect("спектр не пуст");

        assert!(
            loudest.abs_diff(expected_bin) <= 1,
            "тон 1000 Гц оказался в отсчёте {loudest}, ожидался {expected_bin}"
        );
    }

    #[test]
    fn mel_scale_round_trips() {
        for hz in [100.0, 440.0, 1000.0, 4000.0, 8000.0] {
            let back = from_mel(to_mel(hz));
            assert!((back - hz).abs() < 0.5, "{hz} → {back}");
        }
    }

    #[test]
    fn mel_filters_are_ordered_and_inside_the_spectrum() {
        let bounds = filter_bounds();
        assert_eq!(bounds.len(), FILTERS + 2);

        for pair in bounds.windows(2) {
            assert!(pair[0] <= pair[1], "границы фильтров идут не по порядку");
        }
        assert!(*bounds.last().expect("верхняя граница") <= FFT_SIZE / 2);
    }

    #[test]
    fn a_short_chunk_yields_nothing_instead_of_panicking() {
        assert!(extract(&[0.0; 100]).is_empty());
        assert!(extract(&[]).is_empty());
    }

    #[test]
    fn frame_count_follows_the_hop() {
        // Секунда звука при шаге 10 мс — около ста окон.
        let samples = vec![0.1f32; SAMPLE_RATE as usize];
        let frames = extract(&samples);
        assert!((96..=100).contains(&frames.len()), "окон: {}", frames.len());
    }

    #[test]
    fn two_different_sounds_give_different_features() {
        let tone = |frequency: f32| -> Vec<f32> {
            (0..8000)
                .map(|index| {
                    (2.0 * std::f32::consts::PI * frequency * index as f32 / SAMPLE_RATE).sin()
                })
                .collect()
        };

        let low = extract(&tone(300.0));
        let high = extract(&tone(3000.0));
        assert!(!low.is_empty() && !high.is_empty());

        let distance: f32 = low[10]
            .iter()
            .zip(&high[10])
            .map(|(a, b)| (a - b).abs())
            .sum();

        assert!(distance > 1.0, "признаки двух разных тонов совпали");
    }

    #[test]
    fn loudness_does_not_change_the_features_much() {
        // Кепстральная нормализация для этого и нужна: одно и то же слово,
        // сказанное тише, должно остаться тем же словом.
        let quiet: Vec<f32> = (0..8000)
            .map(|index| 0.05 * (index as f32 * 0.21).sin())
            .collect();
        let loud: Vec<f32> = quiet.iter().map(|value| value * 8.0).collect();

        let a = extract_normalised(&quiet);
        let b = extract_normalised(&loud);

        let distance: f32 = a[20]
            .iter()
            .zip(&b[20])
            .map(|(x, y)| (x - y).abs())
            .sum();

        assert!(distance < 0.5, "громкость изменила признаки на {distance}");
    }
}
