//! Эмбеддинги для семантического поиска по памяти (ТЗ §9).
//!
//! # Почему только диалект OpenAI
//!
//! У Anthropic собственного эндпоинта эмбеддингов нет — их документация
//! отсылает к стороннему сервису. Делать вид, что он есть, значит обещать
//! семантический поиск и падать на первом же запросе. Поэтому эмбеддинги
//! доступны там, где есть `/v1/embeddings`: OpenAI, OpenRouter, Ollama,
//! LM Studio и любой совместимый сервер, — а если такого подключения нет,
//! поиск честно остаётся текстовым.
//!
//! Это же делает семантическую память совместимой с режимом Local Only
//! (ТЗ §29): локальная модель эмбеддингов ничем не хуже облачной, а данные
//! не покидают машину.

use serde_json::json;

use crate::types::{AiError, AiResult};

/// Модель по умолчанию.
///
/// Небольшая намеренно: память — это десятки коротких записей, и точность
/// крупной модели здесь не окупает ни времени, ни денег.
pub const DEFAULT_MODEL: &str = "text-embedding-3-small";

/// Запрашивает эмбеддинги для набора текстов.
///
/// Пакетом, а не по одному: индексация всей памяти — это один запрос вместо
/// сотни, а сетевая задержка на каждой записи была бы заметна.
pub async fn embed(
    http: &reqwest::Client,
    base_url: &str,
    api_key: Option<&str>,
    model: &str,
    texts: &[String],
) -> AiResult<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let url = format!("{}/embeddings", base_url.trim_end_matches('/'));
    let mut request = http.post(url).json(&json!({
        "model": model,
        "input": texts,
    }));

    if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        request = request.bearer_auth(key);
    }

    let response = request
        .send()
        .await
        .map_err(|e| AiError::Network(e.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AiError::Network(e.to_string()))?;

    if !status.is_success() {
        return Err(AiError::Api {
            status: status.as_u16(),
            message: text,
        });
    }

    parse(&text, texts.len())
}

/// Разбирает ответ и проверяет, что векторов столько же, сколько текстов.
///
/// Проверка не формальность: сервис возвращает вектора в поле `index`, и
/// молчаливая потеря одного сдвинула бы соответствие «запись → вектор» —
/// поиск после этого выдавал бы уверенно неправильные ответы.
pub fn parse(body: &str, expected: usize) -> AiResult<Vec<Vec<f32>>> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| AiError::Decode(e.to_string()))?;

    let data = value["data"]
        .as_array()
        .ok_or_else(|| AiError::Decode("в ответе нет поля data".into()))?;

    if data.len() != expected {
        return Err(AiError::Decode(format!(
            "запрошено {expected} эмбеддингов, получено {}",
            data.len()
        )));
    }

    let mut result = vec![Vec::new(); expected];

    for item in data {
        let index = item["index"].as_u64().unwrap_or(0) as usize;
        if index >= expected {
            return Err(AiError::Decode(format!("индекс {index} вне запроса")));
        }

        let vector: Vec<f32> = item["embedding"]
            .as_array()
            .ok_or_else(|| AiError::Decode("у элемента нет поля embedding".into()))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        if vector.is_empty() {
            return Err(AiError::Decode("пустой вектор в ответе".into()));
        }

        result[index] = vector;
    }

    if result.iter().any(Vec::is_empty) {
        return Err(AiError::Decode("не все эмбеддинги пришли".into()));
    }

    Ok(result)
}

/// Косинусная близость двух векторов, −1…1.
///
/// Векторы разной длины — это векторы разных моделей: сравнивать их нельзя,
/// и результат 0 здесь честнее любого числа, полученного обрезанием.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let denominator = (norm_a.sqrt()) * (norm_b.sqrt());
    if denominator == 0.0 {
        return 0.0;
    }

    dot / denominator
}

/// Упаковывает вектор в байты для хранения в BLOB.
///
/// Little-endian явно, а не «как получится»: база переживает переезд между
/// машинами, и порядок байтов должен быть один и тот же везде.
pub fn to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Читает вектор из байтов. Повреждённая длина даёт `None`, а не мусор.
pub fn from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }

    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_vectors_in_the_order_the_service_declares() {
        // Сервис вправе вернуть элементы не по порядку — важен index, а не
        // позиция в массиве.
        let body = r#"{"data":[
            {"index":1,"embedding":[0.0,1.0]},
            {"index":0,"embedding":[1.0,0.0]}
        ]}"#;

        let vectors = parse(body, 2).expect("должно разобраться");
        assert_eq!(vectors[0], vec![1.0, 0.0]);
        assert_eq!(vectors[1], vec![0.0, 1.0]);
    }

    #[test]
    fn a_missing_vector_is_a_failure_not_a_silent_gap() {
        let body = r#"{"data":[{"index":0,"embedding":[1.0]}]}"#;
        assert!(parse(body, 2).is_err());
    }

    #[test]
    fn cosine_recognises_the_same_direction_regardless_of_length() {
        let a = [1.0, 2.0, 3.0];
        let b = [2.0, 4.0, 6.0];
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_of_perpendicular_vectors_is_zero() {
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn vectors_of_different_models_do_not_pretend_to_be_comparable() {
        // Разная размерность — разные модели. Обрезать и сравнить значило бы
        // выдать уверенную чушь.
        assert_eq!(cosine(&[1.0, 0.0], &[1.0, 0.0, 0.0]), 0.0);
        assert_eq!(cosine(&[], &[]), 0.0);
    }

    #[test]
    fn a_vector_survives_a_round_trip_through_the_database() {
        let vector = vec![0.5f32, -0.25, 1.0e-3];
        let bytes = to_bytes(&vector);
        assert_eq!(bytes.len(), 12);
        assert_eq!(from_bytes(&bytes), Some(vector));
    }

    #[test]
    fn a_truncated_blob_reads_as_nothing_rather_than_as_a_short_vector() {
        assert_eq!(from_bytes(&[0, 1, 2]), None);
        assert_eq!(from_bytes(&[]), None);
    }
}
