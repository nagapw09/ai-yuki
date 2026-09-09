//! Разбор Server-Sent Events.
//!
//! Все три провайдера стримят ответ через SSE, поэтому декодер общий. Своя
//! реализация вместо библиотеки — потому что нужен ровно один сценарий: читать
//! поле `data`, склеивая многострочные события, и останавливаться по требованию
//! вызывающего.

use futures_util::StreamExt;

use crate::types::{AiError, AiResult};

/// Что делать после обработки события.
pub enum Flow {
    Continue,
    /// Поток закончен по существу — дочитывать остаток незачем.
    Stop,
}

/// Читает SSE-поток, вызывая `on_event` на каждое непустое поле `data`.
///
/// Комментарии SSE (строки, начинающиеся с `:`) и имена событий пропускаются:
/// у всех трёх провайдеров тип события продублирован внутри JSON, а полагаться
/// на заголовок `event:` — значит писать три разных разбора вместо одного.
pub async fn read_events<F>(response: reqwest::Response, mut on_event: F) -> AiResult<()>
where
    F: FnMut(&str) -> AiResult<Flow>,
{
    let mut stream = response.bytes_stream();
    // Буфер сырых байтов: чанк может оборваться посреди UTF-8 последовательности,
    // поэтому декодируем только завершённые строки.
    let mut raw: Vec<u8> = Vec::new();
    let mut data = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AiError::Network(e.to_string()))?;
        raw.extend_from_slice(&chunk);

        while let Some(pos) = raw.iter().position(|b| *b == b'\n') {
            let line_bytes: Vec<u8> = raw.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line_bytes);
            let line = line.trim_end_matches(['\n', '\r']);

            if line.is_empty() {
                if !data.is_empty() {
                    let flow = on_event(&data)?;
                    data.clear();
                    if matches!(flow, Flow::Stop) {
                        return Ok(());
                    }
                }
                continue;
            }

            if let Some(payload) = line.strip_prefix("data:") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(payload.trim_start());
            }
            // Прочие поля (`event:`, `id:`, `:` — комментарий) намеренно игнорируются.
        }
    }

    // Поток мог закончиться без завершающей пустой строки.
    if !data.is_empty() {
        on_event(&data)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Собирает события из готового тела, минуя сеть.
    fn collect(body: &'static str) -> Vec<String> {
        let response = http_response(body);
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = events.clone();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("рантайм должен подняться");

        rt.block_on(read_events(response, |data| {
            sink.lock().expect("мьютекс цел").push(data.to_string());
            Ok(Flow::Continue)
        }))
        .expect("разбор не должен падать");

        let guard = events.lock().expect("мьютекс цел");
        guard.clone()
    }

    fn http_response(body: &'static str) -> reqwest::Response {
        let raw = http::Response::builder()
            .status(200)
            .body(body)
            .expect("ответ должен собраться");
        reqwest::Response::from(raw)
    }

    #[test]
    fn splits_events_on_blank_lines() {
        let events = collect("data: one\n\ndata: two\n\n");
        assert_eq!(events, vec!["one", "two"]);
    }

    #[test]
    fn joins_multiline_data_fields() {
        let events = collect("data: {\ndata: \"a\": 1}\n\n");
        assert_eq!(events, vec!["{\n\"a\": 1}"]);
    }

    #[test]
    fn ignores_event_names_and_comments() {
        let events = collect(": ping\nevent: message_start\ndata: payload\n\n");
        assert_eq!(events, vec!["payload"]);
    }

    #[test]
    fn emits_trailing_event_without_blank_line() {
        let events = collect("data: last\n");
        assert_eq!(events, vec!["last"]);
    }
}
