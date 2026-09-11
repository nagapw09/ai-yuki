//! Живая проверка слова пробуждения: `cargo run -p yuki-voice --release --example wake_check`.
//!
//! Сначала записывает три образца, потом слушает и печатает задержку отзыва.
//! Именно задержку: ТЗ §37 отводит на неё 300 мс, и проверить это можно только
//! на живом голосе — синтетические тесты говорят лишь о том, что арифметика
//! верна.
//!
//! Запускать в release: в отладочной сборке арифметика идёт без оптимизаций, и
//! измеренное время скажет не о детекторе, а о сборке.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use yuki_voice::{WakeDetector, WakeModel, ENROLL_SAMPLES};

/// Сколько писать один образец.
const SAMPLE_SECONDS: u64 = 2;

/// Сколько слушать после записи.
const LISTEN_SECONDS: u64 = 20;

fn main() {
    println!("устройство: {:?}\n", yuki_voice::default_input_name());

    let mut samples = Vec::new();

    for index in 1..=ENROLL_SAMPLES {
        println!("образец {index} из {ENROLL_SAMPLES}: скажите «Юки» после сигнала…");
        std::thread::sleep(Duration::from_millis(600));
        println!("  говорите");

        match record(SAMPLE_SECONDS) {
            Ok(audio) => {
                let loudness = rms(&audio);
                println!("  записано {:.1} с, громкость {loudness:.4}", SAMPLE_SECONDS);
                if loudness < 0.005 {
                    eprintln!("  тихо — похоже, микрофон ничего не услышал");
                }
                samples.push(audio);
            }
            Err(error) => {
                eprintln!("  не удалось записать: {error}");
                std::process::exit(1);
            }
        }
    }

    let Some(model) = WakeModel::from_samples(&samples) else {
        eprintln!("\nмодель не собралась: образцы слишком короткие или пустые");
        std::process::exit(1);
    };

    println!(
        "\nмодель готова: образцов {}, порог {:.3}",
        model.templates.len(),
        model.threshold
    );
    println!("слушаю {LISTEN_SECONDS} секунд — говорите «Юки» и что-нибудь ещё\n");

    let detector = Arc::new(Mutex::new(WakeDetector::new(model)));
    // Момент, когда в потоке впервые появилась речь после тишины: от него и
    // считается задержка отзыва.
    let speech_started = Arc::new(Mutex::new(None::<Instant>));
    let stop = Arc::new(AtomicBool::new(false));

    let detector_in_stream = Arc::clone(&detector);
    let started_in_stream = Arc::clone(&speech_started);
    let stop_in_stream = Arc::clone(&stop);

    let handle = yuki_voice::capture::start(move |frame| {
        if stop_in_stream.load(Ordering::Relaxed) {
            return;
        }

        let loud = rms(frame) > 0.02;
        if let Ok(mut mark) = started_in_stream.lock() {
            if loud && mark.is_none() {
                *mark = Some(Instant::now());
            }
        }

        let heard = detector_in_stream
            .lock()
            .map(|mut detector| detector.push(frame))
            .unwrap_or(false);

        if heard {
            let since = started_in_stream
                .lock()
                .ok()
                .and_then(|mut mark| mark.take())
                .map(|start| start.elapsed());

            match since {
                Some(delay) => println!("  услышала обращение · через {delay:?} после начала речи"),
                None => println!("  услышала обращение"),
            }
        }
    });

    let handle = match handle {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("микрофон не открылся: {error}");
            std::process::exit(1);
        }
    };

    std::thread::sleep(Duration::from_secs(LISTEN_SECONDS));
    stop.store(true, Ordering::Relaxed);
    handle.stop();

    println!("\nготово");
    println!(
        "если обращение не услышано — попробуйте записать образцы заново, \
         ближе к микрофону и тем же голосом, каким будете обращаться"
    );
}

/// Пишет звук указанное время.
fn record(seconds: u64) -> yuki_voice::VoiceResult<Vec<f32>> {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&buffer);

    let handle = yuki_voice::capture::start(move |frame| {
        if let Ok(mut buffer) = sink.lock() {
            buffer.extend_from_slice(frame);
        }
    })?;

    std::thread::sleep(Duration::from_secs(seconds));
    handle.stop();

    let audio = buffer.lock().map(|buffer| buffer.clone()).unwrap_or_default();
    Ok(audio)
}

fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
}
