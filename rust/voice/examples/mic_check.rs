//! Диагностика микрофона: `cargo run -p yuki-voice --example mic_check`.
//!
//! Открывает устройство по умолчанию на несколько секунд и печатает, что реально
//! приходит: сколько кадров, какая громкость, срабатывает ли VAD. Нужен, когда
//! пользователь говорит «Yuki меня не слышит» — по выводу сразу видно, где обрыв:
//! устройство не открылось, тишина в канале, или речь есть, а порог не берётся.

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use std::sync::Mutex;

use yuki_voice::{EnergyVad, SpeechDetector, VadEvent, FRAME_MS};

const SECONDS: u64 = 5;

fn main() {
    println!("устройство по умолчанию: {:?}", yuki_voice::default_input_name());
    println!("все входы: {:?}", yuki_voice::input_devices());
    println!("слушаю {SECONDS} секунд — скажите что-нибудь…\n");

    let frames = Arc::new(AtomicUsize::new(0));
    let speech_frames = Arc::new(AtomicUsize::new(0));
    let phrases = Arc::new(AtomicUsize::new(0));
    // f32 нет в атомиках, поэтому пик хранится в тысячных долях.
    let peak_milli = Arc::new(AtomicU32::new(0));

    let (f, s, p, pk) = (
        frames.clone(),
        speech_frames.clone(),
        phrases.clone(),
        peak_milli.clone(),
    );

    // Оценку шума и пороги показываем в конце — по ним видно, подстроился ли
    // детектор под комнату.
    let vad_state = Arc::new(Mutex::new(EnergyVad::default()));
    let vad_report = vad_state.clone();

    let capture = yuki_voice::capture::start(move |frame| {
        let Ok(mut vad) = vad_state.lock() else { return };
        f.fetch_add(1, Ordering::Relaxed);

        let level = yuki_voice::vad::rms(frame);
        let milli = (level * 1000.0) as u32;
        pk.fetch_max(milli, Ordering::Relaxed);

        match vad.push_frame(frame) {
            VadEvent::SpeechStart | VadEvent::Speech => {
                s.fetch_add(1, Ordering::Relaxed);
            }
            VadEvent::SpeechEnd => {
                s.fetch_add(1, Ordering::Relaxed);
                p.fetch_add(1, Ordering::Relaxed);
            }
            VadEvent::Silence => {}
        }
    });

    let capture = match capture {
        Ok(handle) => handle,
        Err(error) => {
            eprintln!("микрофон не открылся: {error}");
            std::process::exit(1);
        }
    };

    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(SECONDS) {
        std::thread::sleep(Duration::from_millis(200));
    }
    capture.stop();

    let total = frames.load(Ordering::Relaxed);
    let speech = speech_frames.load(Ordering::Relaxed);
    let peak = peak_milli.load(Ordering::Relaxed) as f32 / 1000.0;

    println!("кадров получено: {total} (ожидалось ~{})", SECONDS * 1000 / FRAME_MS as u64);
    println!("из них с речью:  {speech}");
    println!("фраз распознано VAD: {}", phrases.load(Ordering::Relaxed));
    println!("пиковая громкость: {peak:.3}");

    if let Ok(vad) = vad_report.lock() {
        println!(
            "уровень шума: {:.3}, порог начала: {:.3}, порог продолжения: {:.3}",
            vad.noise_floor(),
            vad.start_threshold(),
            vad.stop_threshold()
        );
    }

    if total == 0 {
        println!("\nустройство открылось, но не отдало ни одного кадра — проверьте, \
                  не занят ли микрофон другим приложением");
    } else if peak < 0.01 {
        println!("\nв канале тишина: проверьте, что выбран нужный микрофон и он не приглушён");
    } else if phrases.load(Ordering::Relaxed) == 0 {
        println!("\nзвук есть, но VAD не увидел законченной фразы — говорите громче \
                  или сделайте паузу в конце");
    } else {
        println!("\nконвейер работает: звук приходит, фразы выделяются");
    }
}
