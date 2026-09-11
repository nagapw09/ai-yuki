//! Сколько стоит непрерывное слушание: `cargo run -p yuki-voice --release --example wake_bench`.
//!
//! Детектор слова пробуждения — единственная часть Yuki, которая работает всё
//! время, пока включён голосовой режим. Значит, его цена — это цена самого
//! режима, и знать её надо числом, а не на ощупь.
//!
//! Измеряется доля одного ядра: сколько процессорного времени уходит на секунду
//! звука. Запускать в release — в отладочной сборке цифра скажет о сборке, а не
//! о детекторе.

use std::time::Instant;

use yuki_voice::capture::FRAME_SAMPLES;
use yuki_voice::mfcc::SAMPLE_RATE;
use yuki_voice::{WakeDetector, WakeModel};

/// Сколько звука прогнать.
const MINUTES: f32 = 5.0;

fn main() {
    // Образцы синтетические: детектору всё равно, что в них, — работа зависит
    // от длины окна и числа образцов, а не от их содержания.
    let samples: Vec<Vec<f32>> = (0..3).map(|index| word(index as f32 * 0.3)).collect();

    let Some(model) = WakeModel::from_samples(&samples) else {
        eprintln!("модель не собралась");
        std::process::exit(1);
    };

    let template_frames = model.templates[0].len();
    let mut detector = WakeDetector::new(model);

    let seconds = MINUTES * 60.0;
    let total_frames = (SAMPLE_RATE * seconds / FRAME_SAMPLES as f32) as usize;

    // Шум, а не тишина: на тишине арифметика та же, но пусть вход будет похож
    // на настоящий.
    let frame: Vec<f32> = (0..FRAME_SAMPLES)
        .map(|index| ((index * 7919) % 1000) as f32 / 1000.0 - 0.5)
        .map(|value| value * 0.05)
        .collect();

    println!("окно детектора: {template_frames} кадров признаков на образец");
    println!("прогоняю {MINUTES:.0} минут звука кусками по {FRAME_SAMPLES} отсчётов…\n");

    let started = Instant::now();
    for _ in 0..total_frames {
        detector.push(&frame);
    }
    let elapsed = started.elapsed();

    let share = elapsed.as_secs_f32() / seconds;
    let per_second = elapsed.as_secs_f32() / seconds * 1000.0;

    println!("звука прогнано:     {seconds:.0} с");
    println!("процессорное время: {:.2} с", elapsed.as_secs_f32());
    println!("на секунду звука:   {per_second:.2} мс");
    println!("доля одного ядра:   {:.2} %", share * 100.0);

    // Ориентир: на четырёхъядерной машине из docs/REQUIREMENTS.md это доля
    // одного ядра из четырёх, то есть в четыре раза меньше от всего процессора.
    println!(
        "\nна машине из минимальных требований (4 ядра) это {:.2} % всего процессора",
        share * 100.0 / 4.0
    );
}

/// Синтетическое «слово» длиной полсекунды.
fn word(seed: f32) -> Vec<f32> {
    let length = (SAMPLE_RATE * 0.5) as usize;
    (0..length)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE;
            let frequency = if index < length / 2 { 400.0 } else { 900.0 };
            (2.0 * std::f32::consts::PI * frequency * time + seed).sin() * 0.3
        })
        .collect()
}
