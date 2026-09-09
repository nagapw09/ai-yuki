//! Транспорт HTTP (ТЗ §19).
//!
//! Для удалённых MCP-серверов. Streamable HTTP допускает два вида ответа на один
//! и тот же POST: обычный JSON или поток SSE. Различать их нужно по заголовку
//! `Content-Type`, а не по содержимому — сервер вправе прислать SSE и на запрос,
//! ответ на который умещается в одно событие.

use std::sync::Mutex;

use serde_json::Value;

use crate::protocol::{McpError, McpResult};

pub struct HttpTransport {
    url: String,
    /// Заголовок авторизации целиком, например `Bearer …`.
    authorization: Option<String>,
    /// Идентификатор сессии, выданный сервером при рукопожатии.
    ///
    /// Streamable HTTP требует возвращать его во всех последующих запросах;
    /// без этого сервер считает каждый запрос новой сессией и отвергает вызовы
    /// инструментов как невыполненное рукопожатие.
    session: Mutex<Option<String>>,
    next_id: Mutex<u64>,
    http: reqwest::Client,
}

impl HttpTransport {
    pub fn new(url: impl Into<String>, authorization: Option<String>, http: reqwest::Client) -> Self {
        Self {
            url: url.into(),
            authorization,
            session: Mutex::new(None),
            next_id: Mutex::new(1),
            http,
        }
    }

    pub fn label(&self) -> &str {
        &self.url
    }

    fn take_id(&self) -> u64 {
        let mut guard = self.next_id.lock().unwrap_or_else(|e| e.into_inner());
        let id = *guard;
        *guard += 1;
        id
    }

    async fn send(&self, body: Value) -> McpResult<Option<Value>> {
        let mut request = self
            .http
            .post(&self.url)
            .header("content-type", "application/json")
            // Обязателен для Streamable HTTP: им клиент сообщает, что готов
            // принять и обычный JSON, и поток событий.
            .header("accept", "application/json, text/event-stream");

        if let Some(auth) = &self.authorization {
            request = request.header("authorization", auth);
        }
        if let Some(session) = self.session.lock().ok().and_then(|s| s.clone()) {
            request = request.header("mcp-session-id", session);
        }

        let response = request
            .json(&body)
            .send()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        if let Some(id) = response
            .headers()
            .get("mcp-session-id")
            .and_then(|v| v.to_str().ok())
        {
            if let Ok(mut guard) = self.session.lock() {
                *guard = Some(id.to_string());
            }
        }

        let status = response.status();
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();

        let text = response
            .text()
            .await
            .map_err(|e| McpError::Transport(e.to_string()))?;

        if !status.is_success() {
            return Err(McpError::Transport(format!(
                "{}: {}",
                status.as_u16(),
                if text.is_empty() { "сервер не вернул подробностей" } else { &text }
            )));
        }

        // 202 без тела — нормальный ответ на уведомление.
        if text.trim().is_empty() {
            return Ok(None);
        }

        let payload = if content_type.contains("text/event-stream") {
            first_sse_payload(&text).ok_or_else(|| {
                McpError::Decode("поток событий не содержит данных".into())
            })?
        } else {
            text
        };

        serde_json::from_str::<Value>(&payload)
            .map(Some)
            .map_err(|e| McpError::Decode(format!("{e}: {payload}")))
    }

    pub async fn request(&self, method: &str, params: Value) -> McpResult<Value> {
        let id = self.take_id();
        let body = crate::protocol::request(id, method, params);

        let response = self
            .send(body)
            .await?
            .ok_or_else(|| McpError::Decode("сервер ответил пустым телом".into()))?;

        crate::protocol::parse_response(&response)
    }

    pub async fn notify(&self, method: &str, params: Value) -> McpResult<()> {
        self.send(crate::protocol::notification(method, params))
            .await
            .map(|_| ())
    }
}

/// Достаёт полезную нагрузку первого события SSE.
fn first_sse_payload(body: &str) -> Option<String> {
    let mut data = String::new();

    for line in body.lines() {
        if let Some(chunk) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(chunk.trim_start());
        } else if line.trim().is_empty() && !data.is_empty() {
            // Пустая строка закрывает событие.
            return Some(data);
        }
    }

    (!data.is_empty()).then_some(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_event_of_a_stream() {
        let body = "event: message\ndata: {\"id\":1}\n\ndata: {\"id\":2}\n\n";
        assert_eq!(first_sse_payload(body).as_deref(), Some("{\"id\":1}"));
    }

    #[test]
    fn joins_multiline_event_data() {
        let body = "data: {\ndata: \"a\": 1}\n\n";
        assert_eq!(first_sse_payload(body).as_deref(), Some("{\n\"a\": 1}"));
    }

    #[test]
    fn returns_nothing_for_a_stream_without_data() {
        assert_eq!(first_sse_payload(": пинг\n\n"), None);
    }

    #[test]
    fn accepts_a_stream_that_ends_without_a_blank_line() {
        assert_eq!(first_sse_payload("data: {}").as_deref(), Some("{}"));
    }
}
