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
            .part("file", part);

        if let Some(language) = language {
            form = form.text("language", language.to_string());
        }

        let mut request = self
            .http
            .post(format!("{}/audio/transcriptions", self.base_url));
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

        Ok(parsed["text"].as_str().unwrap_or_default().trim().to_string())
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
    fn reads_provider_error_text_out_of_the_body() {
        let message = extract_error(r#"{"error":{"message":"неверный ключ"}}"#);
        assert_eq!(message, "неверный ключ");
        assert_eq!(extract_error(""), "сервис распознавания не вернул подробностей");
    }
}
