//! Синтез речи чужим сервисом по HTTP (ТЗ §10).
//!
//! # Зачем второй синтезатор, если системный работает
//!
//! Системный синтез — SAPI на Windows, AVSpeechSynthesizer на macOS — не отдаёт
//! звук. Он сам берёт устройство вывода и сам говорит, а приложение узнаёт
//! только «говорю» или «замолчал». Из этого следуют две вещи, которые до сих
//! пор были записаны в долги:
//!
//! 1. Губы аватара двигались по правдоподобному ритму слогов, а не по звуку:
//!    амплитуды не было, потому что не было буфера.
//! 2. Голос был тот, что стоит в системе. Для существа, живущего в компьютере,
//!    это чужой голос.
//!
//! Сервис синтеза отдаёт WAV. Значит, буфер наш: мы его играем, считаем
//! громкость на каждом куске и отдаём наружу — рот открывается ровно на
//! звуке. И голос выбирает человек: GPT-SoVITS клонирует его по короткому
//! образцу.
//!
//! # Почему это не вытесняет системный синтез
//!
//! Сервис — отдельная программа с моделями на гигабайты и, как правило, с
//! видеокартой. Требовать её для того, чтобы Yuki могла сказать «готово»,
//! нельзя: системный синтез остаётся тем, что работает всегда и без спроса,
//! в том числе в режиме Local Only.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use crate::{TextToSpeech, VoiceError, VoiceResult};

/// Настройки сервиса.
#[derive(Debug, Clone)]
pub struct HttpTtsConfig {
    /// Адрес сервиса, например `http://127.0.0.1:9880`.
    pub base_url: String,
    /// Папка с образцами голоса: каждый `.wav` в ней — отдельный голос.
    pub reference_dir: String,
    /// Выбранный образец — имя файла без пути.
    pub reference: String,
    /// Что произнесено в образце: GPT-SoVITS сверяет текст со звуком.
    pub prompt_text: String,
    /// Язык произносимого текста.
    pub text_lang: String,
    /// Язык образца.
    pub prompt_lang: String,
}

impl Default for HttpTtsConfig {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:9880".into(),
            reference_dir: String::new(),
            reference: String::new(),
            prompt_text: String::new(),
            text_lang: "ru".into(),
            prompt_lang: "ru".into(),
        }
    }
}

/// Синтез через HTTP-сервис с воспроизведением своими силами.
pub struct HttpTts {
    config: Mutex<HttpTtsConfig>,
    /// Играет ли прямо сейчас.
    playing: Arc<AtomicBool>,
    /// Просьба замолчать: её проверяет поток воспроизведения.
    cancel: Arc<AtomicBool>,
    /// Текущая громкость 0…1, битами `f32`.
    ///
    /// Атомарно, потому что читают её из другого потока — того, что рисует
    /// аватар, — и блокировка ради одного числа на каждом кадре обошлась бы
    /// дороже самого числа.
    level: Arc<AtomicU32>,
}

impl HttpTts {
    pub fn new(config: HttpTtsConfig) -> Self {
        Self {
            config: Mutex::new(config),
            playing: Arc::new(AtomicBool::new(false)),
            cancel: Arc::new(AtomicBool::new(false)),
            level: Arc::new(AtomicU32::new(0)),
        }
    }

    /// Громкость речи прямо сейчас, 0…1.
    ///
    /// Ноль означает и тишину, и молчание — различать их по этому числу не
    /// нужно: рот всё равно закрыт.
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn configure(&self, config: HttpTtsConfig) {
        if let Ok(mut guard) = self.config.lock() {
            *guard = config;
        }
    }

    fn snapshot(&self) -> VoiceResult<HttpTtsConfig> {
        self.config
            .lock()
            .map(|guard| guard.clone())
            .map_err(|_| VoiceError::Tts("настройки синтеза повреждены".into()))
    }
}

impl TextToSpeech for HttpTts {
    fn speak(&self, text: &str) -> VoiceResult<()> {
        let text = text.trim().to_string();
        if text.is_empty() {
            return Ok(());
        }

        let config = self.snapshot()?;

        if config.reference.trim().is_empty() {
            return Err(VoiceError::Tts(
                "не выбран образец голоса: сервис синтезирует по образцу, без него говорить нечем"
                    .into(),
            ));
        }

        // Предыдущая фраза обрывается: две фразы разом — это каша, а ждать
        // конца прошлой значит отвечать с задержкой в предыдущий ответ.
        self.cancel.store(true, Ordering::Relaxed);

        let playing = Arc::clone(&self.playing);
        let cancel = Arc::clone(&self.cancel);
        let level = Arc::clone(&self.level);

        // Отдельный поток, потому что запрос блокирующий, а `speak` по
        // договору возвращается сразу. Блокирующий клиент создаётся здесь же:
        // внутри асинхронного рантайма его создавать нельзя.
        std::thread::spawn(move || {
            // Дать предыдущему воспроизведению заметить отмену.
            while playing.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }

            cancel.store(false, Ordering::Relaxed);
            playing.store(true, Ordering::Relaxed);

            let outcome = fetch(&config, &text).and_then(|wav| {
                let (samples, rate) = decode_wav(&wav)?;
                play(samples, rate, &cancel, &level)
            });

            if let Err(error) = outcome {
                tracing::warn!("синтез речи: {error}");
            }

            level.store(0f32.to_bits(), Ordering::Relaxed);
            playing.store(false, Ordering::Relaxed);
        });

        Ok(())
    }

    fn stop(&self) -> VoiceResult<()> {
        self.cancel.store(true, Ordering::Relaxed);
        Ok(())
    }

    fn is_speaking(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    /// Голоса — это образцы в папке.
    ///
    /// У сервиса нет списка голосов: голос задаётся образцом звука. Поэтому
    /// список голосов и список файлов — одно и то же, и заводить рядом
    /// отдельный перечень имён значило бы держать его в согласии руками.
    fn voices(&self) -> Vec<String> {
        let Ok(config) = self.snapshot() else {
            return Vec::new();
        };

        references(&config.reference_dir)
    }

    fn set_voice(&self, name: &str) -> VoiceResult<()> {
        let mut config = self.snapshot()?;

        if !references(&config.reference_dir).iter().any(|item| item == name) {
            return Err(VoiceError::Tts(format!("образца «{name}» нет в папке")));
        }

        config.reference = name.to_string();
        self.configure(config);
        Ok(())
    }

    fn set_rate(&self, _rate: f32) -> VoiceResult<()> {
        // Скорость задаётся образцом, а не параметром: сервис воспроизводит
        // манеру речи из него. Молча принять число и не применить его было бы
        // хуже, чем честно отказать.
        Err(VoiceError::Tts(
            "скорость речи задаётся образцом голоса, а не настройкой".into(),
        ))
    }
}

/// Список образцов в папке, по алфавиту.
///
/// Только файлы и только `.wav`: вложенные папки не обходятся, потому что
/// рекурсия по чужой папке — это чтение того, о чём не просили.
pub fn references(dir: &str) -> Vec<String> {
    if dir.trim().is_empty() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        })
        .filter_map(|path| path.file_name().and_then(|name| name.to_str()).map(String::from))
        .collect();

    found.sort_by_key(|name| name.to_lowercase());
    found
}

/// Тело запроса к сервису.
///
/// Формат взят у GPT-SoVITS: `POST /tts` с этими полями возвращает WAV
/// целиком. Отдельной функцией, чтобы состав полей можно было проверить
/// тестом, не поднимая сервис.
pub fn request_body(config: &HttpTtsConfig, text: &str) -> serde_json::Value {
    let reference = std::path::Path::new(&config.reference_dir).join(&config.reference);

    serde_json::json!({
        "text": text,
        "text_lang": config.text_lang,
        "ref_audio_path": reference.to_string_lossy(),
        "prompt_text": config.prompt_text,
        "prompt_lang": config.prompt_lang,
    })
}

fn fetch(config: &HttpTtsConfig, text: &str) -> VoiceResult<Vec<u8>> {
    let url = format!("{}/tts", config.base_url.trim_end_matches('/'));

    let client = reqwest::blocking::Client::builder()
        // Синтез длинной фразы занимает секунды; полминуты хватает, а
        // бесконечное ожидание оставило бы поток висеть навсегда.
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| VoiceError::Tts(e.to_string()))?;

    let response = client
        .post(&url)
        .json(&request_body(config, text))
        .send()
        .map_err(|e| VoiceError::Tts(format!("сервис синтеза недоступен: {e}")))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(VoiceError::Tts(format!(
            "сервис синтеза ответил {status}: {}",
            body.chars().take(200).collect::<String>()
        )));
    }

    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|e| VoiceError::Tts(e.to_string()))
}

/// Разбирает WAV в моно `f32` и частоту.
///
/// Каналы сводятся в один: аватар открывает рот по одной громкости, а
/// устройство вывода всё равно получит копию в каждый канал.
pub fn decode_wav(bytes: &[u8]) -> VoiceResult<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::new(Cursor::new(bytes))
        .map_err(|e| VoiceError::Tts(format!("сервис вернул не WAV: {e}")))?;

    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
        hound::SampleFormat::Int => {
            // Нормируем по разрядности, а не по 16 битам всегда: сервис может
            // отдать 24 или 32 бита, и деление на 32768 превратило бы такой
            // звук в клиппинг.
            let scale = 2f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .filter_map(Result::ok)
                .map(|sample| sample as f32 / scale)
                .collect()
        }
    };

    if raw.is_empty() {
        return Err(VoiceError::Tts("сервис вернул пустой звук".into()));
    }

    let mono = if channels == 1 {
        raw
    } else {
        raw.chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };

    Ok((mono, spec.sample_rate))
}

/// Приводит сигнал к другой частоте линейной интерполяцией.
///
/// Линейной, а не честным фильтром: разница слышна на музыке, а на речи из
/// 32 кГц в 48 кГц — нет, а полноценный ресемплер это отдельная зависимость.
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || samples.is_empty() {
        return samples.to_vec();
    }

    let ratio = to as f64 / from as f64;
    let length = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(length);

    for index in 0..length {
        let position = index as f64 / ratio;
        let left = position.floor() as usize;
        let right = (left + 1).min(samples.len() - 1);
        let fraction = (position - left as f64) as f32;

        let a = samples.get(left).copied().unwrap_or(0.0);
        let b = samples.get(right).copied().unwrap_or(a);
        out.push(a + (b - a) * fraction);
    }

    out
}

/// Громкость куска: среднеквадратичное, а не пик.
///
/// Пик подскакивает на каждом щелчке и делает рот дёрганым; RMS следует за
/// слышимой громкостью, то есть за тем, насколько открыт рот у человека.
pub fn rms(chunk: &[f32]) -> f32 {
    if chunk.is_empty() {
        return 0.0;
    }

    let sum: f32 = chunk.iter().map(|sample| sample * sample).sum();
    (sum / chunk.len() as f32).sqrt().clamp(0.0, 1.0)
}

/// Играет сигнал на устройстве вывода, обновляя громкость.
fn play(
    samples: Vec<f32>,
    rate: u32,
    cancel: &Arc<AtomicBool>,
    level: &Arc<AtomicU32>,
) -> VoiceResult<()> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| VoiceError::Tts("нет устройства вывода звука".into()))?;

    let supported = device
        .default_output_config()
        .map_err(|e| VoiceError::Tts(e.to_string()))?;

    let config: cpal::StreamConfig = supported.config();
    let channels = config.channels.max(1) as usize;

    let ready = Arc::new(resample(&samples, rate, config.sample_rate.0));
    let total = ready.len();
    let cursor = Arc::new(AtomicUsize::new(0));

    let feed = Arc::clone(&ready);
    let position = Arc::clone(&cursor);
    let meter = Arc::clone(level);
    let stop = Arc::clone(cancel);

    let fill = move |output: &mut [f32]| {
        let frames = output.len() / channels;
        let at = position.load(Ordering::Relaxed);

        if stop.load(Ordering::Relaxed) {
            output.fill(0.0);
            position.store(total, Ordering::Relaxed);
            meter.store(0f32.to_bits(), Ordering::Relaxed);
            return;
        }

        let end = (at + frames).min(total);
        let chunk = &feed[at.min(total)..end];

        for (frame, sample) in output.chunks_mut(channels).zip(chunk.iter()) {
            // Один и тот же отсчёт во все каналы: моно, растянутое на стерео.
            frame.fill(*sample);
        }

        // Хвост буфера, на который отсчётов не хватило, обязателен к очистке:
        // иначе устройство повторит предыдущий кусок и речь закончится
        // жужжанием.
        let written = chunk.len() * channels;
        if written < output.len() {
            output[written..].fill(0.0);
        }

        position.store(end, Ordering::Relaxed);
        meter.store(rms(chunk).to_bits(), Ordering::Relaxed);
    };

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            &config,
            move |output: &mut [f32], _| fill(output),
            report,
            None,
        ),
        cpal::SampleFormat::I16 => {
            let mut scratch: Vec<f32> = Vec::new();
            device.build_output_stream(
                &config,
                move |output: &mut [i16], _| {
                    scratch.resize(output.len(), 0.0);
                    fill(&mut scratch);
                    for (target, source) in output.iter_mut().zip(scratch.iter()) {
                        *target = (source.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                    }
                },
                report,
                None,
            )
        }
        other => {
            return Err(VoiceError::Tts(format!(
                "устройство вывода просит формат {other:?}, который мы не умеем"
            )))
        }
    }
    .map_err(|e| VoiceError::Tts(e.to_string()))?;

    stream.play().map_err(|e| VoiceError::Tts(e.to_string()))?;

    // Ждём, пока курсор доедет до конца. Поток должен жить, пока живёт
    // `stream`: уронив его раньше, мы оборвём звук на середине фразы.
    while cursor.load(Ordering::Relaxed) < total && !cancel.load(Ordering::Relaxed) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    Ok(())
}

fn report(error: cpal::StreamError) {
    tracing::warn!("вывод звука: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> HttpTtsConfig {
        HttpTtsConfig {
            base_url: "http://127.0.0.1:9880/".into(),
            reference_dir: "D:/voices".into(),
            reference: "yuki.wav".into(),
            prompt_text: "Это образец голоса".into(),
            text_lang: "ru".into(),
            prompt_lang: "ru".into(),
        }
    }

    /// Запрос содержит все поля, которые ждёт сервис.
    ///
    /// Пропущенное поле обнаружилось бы только тем, что сервис отвечает
    /// ошибкой в чужой формулировке, а искать причину человек пошёл бы в
    /// настройки сети.
    #[test]
    fn the_request_carries_every_field_the_service_needs() {
        let body = request_body(&config(), "привет");

        assert_eq!(body["text"], "привет");
        assert_eq!(body["text_lang"], "ru");
        assert_eq!(body["prompt_lang"], "ru");
        assert_eq!(body["prompt_text"], "Это образец голоса");

        let path = body["ref_audio_path"].as_str().expect("нет пути к образцу");
        assert!(path.contains("yuki.wav"), "путь без имени образца: {path}");
        assert!(path.contains("voices"), "путь без папки: {path}");
    }

    /// Разбор WAV: целые отсчёты нормируются по разрядности.
    #[test]
    fn a_wav_is_decoded_into_mono_floats() {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 24_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut bytes = Vec::new();
        {
            let mut writer =
                hound::WavWriter::new(Cursor::new(&mut bytes), spec).expect("писатель");
            for sample in [0i16, i16::MAX, i16::MIN, 0] {
                writer.write_sample(sample).expect("отсчёт");
            }
            writer.finalize().expect("закрытие");
        }

        let (samples, rate) = decode_wav(&bytes).expect("должно разобраться");

        assert_eq!(rate, 24_000);
        assert_eq!(samples.len(), 4);
        assert!((samples[1] - 1.0).abs() < 0.001, "максимум стал {}", samples[1]);
        assert!((samples[2] + 1.0).abs() < 0.001, "минимум стал {}", samples[2]);
    }

    /// Стерео сводится в моно.
    #[test]
    fn stereo_is_mixed_down() {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut bytes = Vec::new();
        {
            let mut writer =
                hound::WavWriter::new(Cursor::new(&mut bytes), spec).expect("писатель");
            // Левый канал в максимуме, правый в нуле — в моно должно выйти пол.
            writer.write_sample(i16::MAX).expect("л");
            writer.write_sample(0i16).expect("п");
            writer.finalize().expect("закрытие");
        }

        let (samples, _) = decode_wav(&bytes).expect("должно разобраться");

        assert_eq!(samples.len(), 1, "два канала должны стать одним отсчётом");
        assert!((samples[0] - 0.5).abs() < 0.01, "смешалось в {}", samples[0]);
    }

    #[test]
    fn garbage_is_not_mistaken_for_sound() {
        assert!(decode_wav(b"not a wav at all").is_err());
        assert!(decode_wav(&[]).is_err());
    }

    /// Пересчёт частоты сохраняет длительность.
    #[test]
    fn resampling_keeps_the_duration() {
        let signal: Vec<f32> = (0..1000).map(|i| (i as f32 / 50.0).sin()).collect();

        let up = resample(&signal, 24_000, 48_000);
        assert_eq!(up.len(), 2000, "вдвое большая частота — вдвое больше отсчётов");

        let down = resample(&signal, 48_000, 24_000);
        assert_eq!(down.len(), 500);

        // Та же частота — тот же сигнал, без лишней работы.
        assert_eq!(resample(&signal, 44_100, 44_100), signal);
    }

    /// Пересчёт не портит форму сигнала.
    #[test]
    fn resampling_follows_the_signal() {
        // Линейный рост: при любой частоте он остаётся линейным, и по концам
        // видно, что интерполяция не съехала.
        let ramp: Vec<f32> = (0..100).map(|i| i as f32 / 99.0).collect();
        let out = resample(&ramp, 100, 300);

        assert!((out[0] - 0.0).abs() < 0.01);
        assert!((out[out.len() - 1] - 1.0).abs() < 0.02, "конец стал {}", out[out.len() - 1]);

        // Середина должна быть серединой.
        let middle = out[out.len() / 2];
        assert!((middle - 0.5).abs() < 0.02, "середина стала {middle}");
    }

    #[test]
    fn resampling_survives_an_empty_signal() {
        assert!(resample(&[], 24_000, 48_000).is_empty());
        assert!(resample(&[0.5], 0, 48_000).len() == 1);
    }

    /// Громкость считается по среднеквадратичному.
    #[test]
    fn loudness_follows_the_signal_and_not_its_peaks() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0, 0.0, 0.0]), 0.0);
        assert!((rms(&[1.0, 1.0]) - 1.0).abs() < 0.001);

        // Один щелчок среди тишины не должен открывать рот нараспашку.
        let click = rms(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
        assert!(click < 0.4, "щелчок дал громкость {click}");

        // Ровный сигнал вполовину — половина и есть.
        let steady = rms(&[0.5, 0.5, 0.5, 0.5]);
        assert!((steady - 0.5).abs() < 0.001, "ровный сигнал дал {steady}");
    }

    /// Образцы читаются из папки, только `.wav`, по алфавиту.
    #[test]
    fn voices_are_the_wav_files_in_the_folder() {
        let dir = std::env::temp_dir().join(format!("yuki-voices-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("папка");

        for name in ["Бета.wav", "альфа.WAV", "readme.txt", "нота.mp3"] {
            std::fs::write(dir.join(name), b"x").expect("файл");
        }
        std::fs::create_dir_all(dir.join("вложенная.wav")).expect("папка-обманка");

        let found = references(dir.to_str().expect("путь"));

        assert_eq!(found, vec!["альфа.WAV", "Бета.wav"], "нашлось: {found:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_folder_gives_no_voices() {
        assert!(references("").is_empty());
        assert!(references("D:/такой/папки/нет").is_empty());
    }

    /// Без образца синтез отказывается говорить, а не молчит.
    #[test]
    fn without_a_sample_it_says_so() {
        let tts = HttpTts::new(HttpTtsConfig::default());
        let error = tts.speak("привет").expect_err("должно отказать");

        assert!(
            error.to_string().contains("образец"),
            "непонятная причина: {error}"
        );
    }

    /// Пустой текст — не ошибка: ассистенту бывает нечего сказать вслух.
    #[test]
    fn empty_text_is_silence_and_not_a_failure() {
        let tts = HttpTts::new(HttpTtsConfig::default());
        assert!(tts.speak("   ").is_ok());
        assert!(!tts.is_speaking());
    }

    /// Скорость речи задаётся образцом — настройка честно отказывает.
    #[test]
    fn the_rate_setting_refuses_instead_of_lying() {
        let tts = HttpTts::new(config());
        assert!(tts.set_rate(0.5).is_err());
    }

    /// Несуществующий голос не выбирается.
    #[test]
    fn an_unknown_voice_is_refused() {
        let tts = HttpTts::new(config());
        assert!(tts.set_voice("такого-нет.wav").is_err());
    }
}
