//! Живая проверка синтеза по HTTP: играет фразу и печатает громкость.
//!
//! Проверяется то, что нельзя проверить юнит-тестом: звук действительно
//! доходит до устройства вывода, а громкость меняется по ходу речи — именно
//! она открывает рот аватару. Числа должны расти на словах и падать в паузах;
//! ровный ноль означает, что играет тишина, а ровная единица — что где-то
//! потерялось нормирование.
//!
//! ```text
//! cargo run -p yuki-voice --release --example tts_check -- <папка> <образец> [текст]
//! ```
//!
//! Сервис синтеза должен слушать на `http://127.0.0.1:9880`.

use std::time::{Duration, Instant};

use yuki_voice::{HttpTts, HttpTtsConfig, TextToSpeech};

fn main() {
    let mut args = std::env::args().skip(1);

    let reference_dir = args.next().unwrap_or_else(|| {
        eprintln!("нужна папка с образцами голоса");
        std::process::exit(2);
    });

    let reference = args.next().unwrap_or_else(|| {
        eprintln!("нужно имя файла образца");
        std::process::exit(2);
    });

    let text = args
        .next()
        .unwrap_or_else(|| "Проверка синтеза речи. Слушаю и говорю.".to_string());

    let tts = HttpTts::new(HttpTtsConfig {
        base_url: "http://127.0.0.1:9880".into(),
        reference_dir: reference_dir.clone(),
        reference: reference.clone(),
        prompt_text: "образец".into(),
        text_lang: "ru".into(),
        prompt_lang: "ru".into(),
    });

    println!("образцы в папке: {:?}", tts.voices());

    if let Err(error) = tts.speak(&text) {
        eprintln!("не получилось: {error}");
        std::process::exit(1);
    }

    let started = Instant::now();
    let mut peak = 0f32;
    let mut ticks = 0u32;
    let mut heard = 0u32;

    // Ждём начала: запрос к сервису занимает время, и мерить громкость до
    // того, как звук пошёл, значит записать ноль и решить, что всё плохо.
    while !tts.is_speaking() && started.elapsed() < Duration::from_secs(35) {
        std::thread::sleep(Duration::from_millis(20));
    }

    if !tts.is_speaking() {
        eprintln!("речь так и не началась за 35 секунд");
        std::process::exit(1);
    }

    println!("пошла речь через {:?}", started.elapsed());

    while tts.is_speaking() {
        let level = tts.level();
        peak = peak.max(level);
        ticks += 1;
        if level > 0.01 {
            heard += 1;
        }

        // Печатаем полоской: по ней видно ритм речи, а не только числа.
        let bars = (level * 60.0).round() as usize;
        println!("{:>5.3} {}", level, "#".repeat(bars.min(60)));

        std::thread::sleep(Duration::from_millis(100));
    }

    println!();
    println!("длительность: {:?}", started.elapsed());
    println!("замеров: {ticks}, из них со звуком: {heard}");
    println!("пик громкости: {peak:.3}");

    if heard == 0 {
        eprintln!("громкость всё время была нулевой — играла тишина");
        std::process::exit(1);
    }

    if peak > 0.999 {
        eprintln!("громкость упиралась в единицу — похоже на клиппинг");
        std::process::exit(1);
    }
}
