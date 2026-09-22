//! Сколько стоит непрерывное слушание: `cargo run -p yuki-voice --release --example wake_bench`.
//!
//! Детектор слова пробуждения — единственная часть Yuki, которая работает всё
//! время, пока включён голосовой режим. Значит, его цена — это цена самого
//! режима, и знать её надо числом, а не на ощупь.
//!
//! Меряются оба пути: rustpotter (то, чем слово записывается сейчас) и прежнее
//! сравнение с образцами (то, что осталось у людей с предыдущих версий). Одно
//! число само по себе ничего не говорит — важно, во что обошёлся переход.
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

    let seconds = MINUTES * 60.0;
    let total_frames = (SAMPLE_RATE * seconds / FRAME_SAMPLES as f32) as usize;

    // Шум, а не тишина: на тишине арифметика та же, но пусть вход будет похож
    // на настоящий.
    let frame: Vec<f32> = (0..FRAME_SAMPLES)
        .map(|index| ((index * 7919) % 1000) as f32 / 1000.0 - 0.5)
        .map(|value| value * 0.05)
        .collect();

    println!("прогоняю {MINUTES:.0} минут звука кусками по {FRAME_SAMPLES} отсчётов…\n");

    let now = WakeModel::from_samples(&samples).expect("модель rustpotter не собралась");
    let before = WakeModel::from_templates(&samples).expect("прежняя модель не собралась");
    println!(
        "прежний путь: окно {} кадров признаков на образец\n",
        before.templates[0].len()
    );

    let now = measure("rustpotter", now, &frame, total_frames, seconds);
    let before = measure(
        "сравнение с образцами",
        before,
        &frame,
        total_frames,
        seconds,
    );

    println!();
    if now < before {
        println!(
            "переход на rustpotter дешевле прежнего в {:.1} раза",
            before / now
        );
    } else {
        println!(
            "переход на rustpotter дороже прежнего в {:.1} раза",
            now / before
        );
    }

    // Ориентир: на четырёхъядерной машине из docs/REQUIREMENTS.md это доля
    // одного ядра из четырёх, то есть в четыре раза меньше от всего процессора.
    println!(
        "на машине из минимальных требований (4 ядра) нынешний путь это {:.2} % всего процессора",
        now * 100.0 / 4.0
    );
}

/// Прогоняет звук через детектор и возвращает долю одного ядра.
fn measure(name: &str, model: WakeModel, frame: &[f32], frames: usize, seconds: f32) -> f32 {
    let Some(mut detector) = WakeDetector::new(model) else {
        eprintln!("{name}: детектор не собрался");
        std::process::exit(1);
    };

    let started = Instant::now();
    for _ in 0..frames {
        detector.push(frame);
    }
    let elapsed = started.elapsed();

    let share = elapsed.as_secs_f32() / seconds;
    println!(
        "{name:<22} {:>6.2} с процессора · {:>5.2} мс на секунду звука · {:>5.2} % ядра",
        elapsed.as_secs_f32(),
        share * 1000.0,
        share * 100.0
    );
    share
}

/// Синтетическое «слово» длиной полсекунды.
fn word(seed: f32) -> Vec<f32> {
    let length = (SAMPLE_RATE * 0.5) as usize;
    (0..length)
        .map(|index| {
            let time = index as f32 / SAMPLE_RATE;
            let envelope = (time * 6.0).min(1.0) * (1.0 - time * 0.8);
            ((time * (400.0 + seed * 40.0) * std::f32::consts::TAU).sin() * 0.6
                + (time * (900.0 + seed * 60.0) * std::f32::consts::TAU).sin() * 0.4)
                * envelope
        })
        .collect()
}
