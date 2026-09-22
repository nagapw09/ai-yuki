//! Распознавание речи (ТЗ §10).
//!
//! Реализация ходит в эндпоинт `/v1/audio/transcriptions` — тот же протокол,
//! что у OpenAI, Groq и локальных серверов вроде `whisper.cpp --server`. Один
//! контракт покрывает и облако, и локальный режим из ТЗ §29: меняется только
//! адрес, а «локально» означает `http://localhost`, а не другой код.
//!
//! Локальный движок в самом процессе (`whisper-rs`) остаётся отдельной
//! реализацией [`SpeechToText`] — он требует скачивания модели на сотни
//! мегабайт, и это осмысленно делать по явному выбору пользователя.

use async_trait::async_trait;

use crate::error::{VoiceError, VoiceResult};

/// Распознаватель речи.
#[async_trait]
pub trait SpeechToText: Send + Sync {
    /// Принимает моно 16 кГц и возвращает текст.
    ///
    /// `language` — подсказка вида `ru`, `en`. Она заметно повышает точность на
    /// коротких командах, где по звуку язык угадывается плохо.
    async fn transcribe(&self, samples: &[f32], language: Option<&str>) -> VoiceResult<String>;
}

/// Кодирует моно-сигнал в WAV: именно его ждёт эндпоинт транскрипции.
pub fn encode_wav(samples: &[f32], sample_rate: u32) -> VoiceResult<Vec<u8>> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut buffer, spec)
            .map_err(|e| VoiceError::Encode(e.to_string()))?;
        for sample in samples {
            // Ограничение обязательно: значение за пределами -1.0…1.0 при
            // приведении к i16 переполнится и превратит громкий звук в треск.
            let clamped = sample.clamp(-1.0, 1.0);
            let value = (clamped * i16::MAX as f32) as i16;
            writer
                .write_sample(value)
                .map_err(|e| VoiceError::Encode(e.to_string()))?;
        }
        writer
            .finalize()
            .map_err(|e| VoiceError::Encode(e.to_string()))?;
    }

    Ok(buffer.into_inner())
}

/// Распознавание через HTTP-эндпоинт формата OpenAI.
pub struct HttpStt {
    base_url: String,
    api_key: Option<String>,
    model: String,
    http: reqwest::Client,
}

impl HttpStt {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
        http: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            http,
        }
    }
}

/// Куда именно отправлять запись.
///
/// У OpenAI, Groq и совместимых сервисов путь один — `/audio/transcriptions`.
/// У локального `whisper.cpp` он свой, `/inference`, и это единственное, чем
/// локальный движок отличается от облачного. Настройка хранит полный адрес, и
/// уже заданный путь мы не дописываем второй раз.
pub fn transcription_endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    if base.ends_with("/inference") || base.ends_with("/audio/transcriptions") {
        base.to_string()
    } else {
        format!("{base}/audio/transcriptions")
    }
}

#[async_trait]
impl SpeechToText for HttpStt {
    async fn transcribe(&self, samples: &[f32], language: Option<&str>) -> VoiceResult<String> {
        if samples.is_empty() {
            return Ok(String::new());
        }

        let wav = encode_wav(samples, crate::capture::TARGET_RATE)?;

        let part = reqwest::multipart::Part::bytes(wav)
            .file_name("speech.wav")
            .mime_str("audio/wav")
            .map_err(|e| VoiceError::Encode(e.to_string()))?;

        let mut form = reqwest::multipart::Form::new()
            .text("model", self.model.clone())
            // Подробный ответ нужен ради оценки языка: по ней отсеиваются
            // выдумки на шуме, см. `is_hallucination`. Облачные сервисы такой
            // оценки не дают — тогда фильтр просто не срабатывает.
            .text("response_format", "verbose_json")
            .part("file", part);

        if let Some(language) = language {
            form = form.text("language", language.to_string());
        }

        let mut request = self.http.post(transcription_endpoint(&self.base_url));
        if let Some(key) = self.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            request = request.bearer_auth(key);
        }

        let response = request
            .multipart(form)
            .send()
            .await
            .map_err(|e| VoiceError::Network(e.to_string()))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|e| VoiceError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(VoiceError::Stt(format!(
                "{}: {}",
                status.as_u16(),
                extract_error(&body)
            )));
        }

        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| VoiceError::Stt(e.to_string()))?;

        let text = parsed["text"].as_str().unwrap_or_default().trim();
        if is_hallucination(&parsed, language) {
            // Пустая строка означает «услышали тишину» — вызывающий уже умеет
            // молча вернуться к прослушиванию.
            return Ok(String::new());
        }
        Ok(text.to_string())
    }
}

/// Порог: ниже этой уверенности в языке запись не считается речью.
///
/// Замеры на `whisper.cpp` с русской моделью: настоящая речь даёт 0,57…0,98
/// даже на командах в одно слово, чистый шум — 0,02. Четверть лежит посередине
/// с запасом в обе стороны.
const LANGUAGE_FLOOR: f64 = 0.25;

/// Выдумка ли это на шуме.
///
/// Распознаватель почти никогда не молчит: на шуме вентилятора он уверенно
/// выдаёт связную фразу вроде «Возьмите и не забывайте». Поле `no_speech_prob`
/// в этом не помогает — на чистом шуме оно равно 0,000016, то есть «это точно
/// речь». Зато оценка языка честная: на шуме распознаватель «слышит» нюнорск и
/// даёт русскому две сотых.
///
/// Если оценок нет (так отвечают облачные сервисы), запись не отбрасывается:
/// лучше лишняя фраза, чем потерянная команда.
fn is_hallucination(parsed: &serde_json::Value, language: Option<&str>) -> bool {
    let Some(expected) = language else {
        return false;
    };
    let Some(scores) = parsed["language_probabilities"].as_object() else {
        return false;
    };
    match scores.get(expected).and_then(serde_json::Value::as_f64) {
        Some(score) => score < LANGUAGE_FLOOR,
        // Языка нет в списке вовсе — судить не по чему.
        None => false,
    }
}

fn extract_error(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| {
            if body.is_empty() {
                "сервис распознавания не вернул подробностей".into()
            } else {
                body.to_string()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_has_a_correct_header_and_expected_length() {
        let samples = vec![0.0_f32; 1600]; // 100 мс при 16 кГц
        let wav = encode_wav(&samples, 16_000).expect("кодирование должно пройти");

        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        // 44 байта заголовка + по два байта на отсчёт.
        assert_eq!(wav.len(), 44 + samples.len() * 2);
    }

    #[test]
    fn clamps_samples_instead_of_letting_them_wrap() {
        // Без ограничения 2.0 превратилось бы в отрицательное число, и громкий
        // звук зазвучал бы как треск.
        let wav = encode_wav(&[2.0, -2.0], 16_000).expect("кодирование должно пройти");
        let first = i16::from_le_bytes([wav[44], wav[45]]);
        let second = i16::from_le_bytes([wav[46], wav[47]]);

        assert_eq!(first, i16::MAX);
        assert_eq!(second, -i16::MAX);
    }

    #[test]
    fn local_whisper_keeps_its_own_path_and_the_cloud_gets_the_openai_one() {
        assert_eq!(
            transcription_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(
            transcription_endpoint("http://127.0.0.1:8756/inference"),
            "http://127.0.0.1:8756/inference"
        );
        // Лишняя косая черта в настройке не должна порождать двойную в адресе.
        assert_eq!(
            transcription_endpoint("https://api.groq.com/openai/v1/"),
            "https://api.groq.com/openai/v1/audio/transcriptions"
        );
        // Полный путь, введённый руками, не дописывается второй раз.
        assert_eq!(
            transcription_endpoint("https://example.com/v1/audio/transcriptions"),
            "https://example.com/v1/audio/transcriptions"
        );
    }

    #[test]
    fn noise_that_sounds_like_a_sentence_is_dropped() {
        // Настоящие ответы whisper.cpp: слева шум вентилятора, справа речь.
        let noise = serde_json::json!({
            "text": " Возьмите и не забывайте.",
            "detected_language": "nynorsk",
            "language_probabilities": {"ru": 0.0224, "en": 0.1168, "nn": 0.5706},
        });
        let speech = serde_json::json!({
            "text": " Юки, открой браузер",
            "detected_language": "russian",
            "language_probabilities": {"ru": 0.9847, "en": 0.004},
        });
        assert!(is_hallucination(&noise, Some("ru")));
        assert!(!is_hallucination(&speech, Some("ru")));
    }

    #[test]
    fn a_one_word_command_is_not_mistaken_for_noise() {
        // Замер: «Юки» — 0,723, «Юки, стоп» — 0,567. Короткая команда должна
        // проходить, иначе слово пробуждения перестанет работать.
        for score in [0.723, 0.567] {
            let short = serde_json::json!({"text": "Юки", "language_probabilities": {"ru": score}});
            assert!(!is_hallucination(&short, Some("ru")), "порог отсёк {score}");
        }
    }

    #[test]
    fn without_language_scores_nothing_is_dropped() {
        // Облачные сервисы оценок не присылают — фильтр обязан молчать.
        let cloud = serde_json::json!({"text": "привет"});
        assert!(!is_hallucination(&cloud, Some("ru")));
        // И без заданного языка судить тоже не по чему.
        let scored = serde_json::json!({"language_probabilities": {"ru": 0.01}});
        assert!(!is_hallucination(&scored, None));
    }

    #[test]
    fn reads_provider_error_text_out_of_the_body() {
        let message = extract_error(r#"{"error":{"message":"неверный ключ"}}"#);
        assert_eq!(message, "неверный ключ");
        assert_eq!(
            extract_error(""),
            "сервис распознавания не вернул подробностей"
        );
    }
}
