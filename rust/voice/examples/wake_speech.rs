//! Проверка слова пробуждения на настоящей речи, без микрофона:
//! `cargo run -p yuki-voice --release --example wake_speech`.
//!
//! # Зачем это отдельно от тестов
//!
//! Синтетические тесты в `wake.rs` говорят только о том, что арифметика верна:
//! две синусоиды детектор различает. Но жалоба звучала иначе — «во время
//! разговора с другим человеком оно взяло 1 слово и начало говорить». Про такое
//! синусоиды не скажут ничего: у речи есть форманты, шум смычек и длина фразы,
//! и ложное срабатывание рождается именно из них.
//!
//! Живой микрофон это проверил бы честнее всего, но требует человека у
//! компьютера. Системный синтезатор даёт настоящую речь без человека: обращение
//! записывается одним голосом, а проверяется и тем же, и чужим — то есть видно
//! отдельно качество детектора и его привязку к голосу.
//!
//! # Чего эта проверка не заменяет
//!
//! Синтезированная речь чище живой: в ней нет комнаты, микрофона и расстояния
//! до него. Поэтому здесь видно разницу между детекторами и поведение порога,
//! но не абсолютную долю ошибок в жизни — за ней по-прежнему `wake_check` с
//! живым голосом.

#![cfg(windows)]

use yuki_voice::mfcc::SAMPLE_RATE;
use yuki_voice::spotter::{self, Spotter};
use yuki_voice::{WakeDetector, WakeModel};

use windows::{
    core::HSTRING, Media::SpeechSynthesis::SpeechSynthesizer, Storage::Streams::DataReader,
};

/// Слово, которое записывается как обращение.
const WAKE: &str = "Юки";

/// Фразы, на которые детектор обязан отозваться.
const CALLS: &[&str] = &[
    "Юки",
    "Юки, открой браузер",
    "Юки, какая сегодня погода",
    "Слушай, Юки",
];

/// Чужой разговор и её собственные ответы: ни одна фраза не обращена к Yuki.
const CHATTER: &[&str] = &[
    "Привет, как дела",
    "Сегодня хорошая погода, пойдём гулять",
    "Я перезвоню тебе через полчаса, сейчас неудобно",
    "Юрий сказал, что документы уже готовы",
    "Смотри, какая штука интересная получилась",
    "Нет, я не думаю, что это хорошая идея",
    "Открой браузер и посмотри почту",
    "Ключи на столе в прихожей лежат",
    "Готово, открыла браузер",
    "Сейчас посмотрю, минутку",
    "Не поняла, повторите пожалуйста",
];

/// Пороги, на которых меряется детектор.
const THRESHOLDS: &[f32] = &[0.5, 0.45, 0.4, 0.35, 0.3, 0.25, 0.2];

fn main() {
    let voices = SpeechSynthesizer::AllVoices()
        .map(|list| {
            list.into_iter()
                .filter_map(|v| v.DisplayName().ok().map(|n| n.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if voices.is_empty() {
        eprintln!("в системе нет голосов синтезатора — проверять нечем");
        std::process::exit(1);
    }
    println!("голоса системы: {}\n", voices.join(", "));

    // Записываем обращение одним голосом на трёх скоростях — так же, как
    // человек говорит его три раза подряд чуть по-разному.
    let own = &voices[0];
    let samples: Vec<Vec<f32>> = [0.9, 1.0, 1.15]
        .into_iter()
        .filter_map(|rate| say(WAKE, own, rate))
        .collect();

    if samples.len() < 3 {
        eprintln!("синтезатор не отдал три образца");
        std::process::exit(1);
    }
    println!(
        "обращение записано голосом «{own}»: {} · после обрезки тишины {}",
        lengths(&samples),
        lengths(
            &samples
                .iter()
                .map(|audio| spotter::trim(audio).to_vec())
                .collect::<Vec<_>>()
        )
    );

    let other = voices.get(1).cloned();
    match &other {
        Some(name) => println!("чужой голос для проверки: «{name}»\n"),
        None => println!("второго голоса в системе нет — привязку к голосу не проверить\n"),
    }

    // Фразы синтезируются по разу и переиспользуются на всех порогах: иначе
    // прогон занимает минуты, а разница между порогами тонет в разнице синтеза.
    let mut sets = vec![("свой голос", speak_all(own))];
    if let Some(name) = &other {
        sets.push(("чужой голос", speak_all(name)));
    }

    let reference = spotter::build(&samples).expect("эталон не собрался");

    println!("── rustpotter, по порогам ──");
    println!("   порог │ {:^38} │", "услышала обращений / ложных отзывов");
    for &threshold in THRESHOLDS {
        print!("    {threshold:.2} │");
        for (_, phrases) in &sets {
            let (hits, calls, alarms, total) = measure(
                || Spotter::with_threshold(&reference, threshold).expect("эталон не развернулся"),
                phrases,
            );
            print!(" {hits}/{calls} обращений, {alarms}/{total} ложных │");
        }
        println!();
    }

    println!("\n── сравнение с образцами (прежний путь) ──");
    let before = WakeModel::from_templates(&samples).expect("прежняя модель не собралась");
    for (name, phrases) in &sets {
        let (hits, calls, alarms, total) = measure(
            || WakeDetector::new(before.clone()).expect("детектор не собрался"),
            phrases,
        );
        println!("  {name:<12} {hits}/{calls} обращений, {alarms}/{total} ложных");
        for (phrase, fired) in phrases.iter().zip(fired_flags(&before, phrases)) {
            if fired && !CALLS.contains(&phrase.0.as_str()) {
                println!("      ложно отозвалась на: «{}»", phrase.0);
            }
        }
    }
}

/// Одна проверка: сколько обращений услышано и сколько чужих фраз прошло.
fn measure<D: Detector>(
    make: impl Fn() -> D,
    phrases: &[(String, Vec<f32>)],
) -> (usize, usize, usize, usize) {
    let mut hits = 0;
    let mut calls = 0;
    let mut alarms = 0;
    let mut chatter = 0;

    for (text, audio) in phrases {
        // Детектор для каждой фразы свой: после срабатывания у него выдержка,
        // и общий детектор занизил бы число ложных отзывов.
        let mut detector = make();
        let fired = feed(&mut detector, audio);

        if CALLS.contains(&text.as_str()) {
            calls += 1;
            hits += usize::from(fired);
        } else {
            chatter += 1;
            alarms += usize::from(fired);
        }
    }
    (hits, calls, alarms, chatter)
}

/// Кто из фраз прошёл через прежний детектор — чтобы назвать их поимённо.
fn fired_flags(model: &WakeModel, phrases: &[(String, Vec<f32>)]) -> Vec<bool> {
    phrases
        .iter()
        .map(|(_, audio)| {
            let mut detector = WakeDetector::new(model.clone()).expect("детектор не собрался");
            feed(&mut detector, audio)
        })
        .collect()
}

/// Общий вид двух детекторов: оба принимают куски звука и отвечают «да/нет».
trait Detector {
    fn push(&mut self, samples: &[f32]) -> bool;
}
impl Detector for Spotter {
    fn push(&mut self, samples: &[f32]) -> bool {
        Spotter::push(self, samples)
    }
}
impl Detector for WakeDetector {
    fn push(&mut self, samples: &[f32]) -> bool {
        WakeDetector::push(self, samples)
    }
}

/// Прогоняет фразу так, как она пришла бы с микрофона: кусками по 20 мс,
/// с полусекундной тишиной вокруг.
fn feed<D: Detector>(detector: &mut D, audio: &[f32]) -> bool {
    let quiet = vec![0.0f32; (SAMPLE_RATE * 0.5) as usize];
    let mut stream = quiet.clone();
    stream.extend_from_slice(audio);
    stream.extend_from_slice(&quiet);

    let chunk = (SAMPLE_RATE * 0.02) as usize;
    stream.chunks(chunk).any(|piece| detector.push(piece))
}

/// Синтезирует весь набор фраз одним голосом.
fn speak_all(voice: &str) -> Vec<(String, Vec<f32>)> {
    CALLS
        .iter()
        .chain(CHATTER.iter())
        .filter_map(|phrase| say(phrase, voice, 1.0).map(|audio| ((*phrase).to_string(), audio)))
        .collect()
}

fn lengths(samples: &[Vec<f32>]) -> String {
    samples
        .iter()
        .map(|audio| format!("{:.2} с", audio.len() as f32 / SAMPLE_RATE))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Синтезирует фразу и отдаёт её в формате микрофона: 16 кГц, моно.
fn say(text: &str, voice: &str, rate: f64) -> Option<Vec<f32>> {
    let synth = SpeechSynthesizer::new().ok()?;
    for candidate in SpeechSynthesizer::AllVoices().ok()? {
        if candidate.DisplayName().ok()? == voice {
            synth.SetVoice(&candidate).ok()?;
            break;
        }
    }
    synth.Options().ok()?.SetSpeakingRate(rate).ok()?;

    let stream = synth
        .SynthesizeTextToStreamAsync(&HSTRING::from(text))
        .ok()?
        .get()
        .ok()?;
    let size = stream.Size().ok()?;
    let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
    reader.LoadAsync(size as u32).ok()?.get().ok()?;
    let mut bytes = vec![0; size as usize];
    reader.ReadBytes(&mut bytes).ok()?;

    let (samples, source_rate) = yuki_voice::tts_http::decode_wav(&bytes).ok()?;
    Some(yuki_voice::tts_http::resample(
        &samples,
        source_rate,
        SAMPLE_RATE as u32,
    ))
}
