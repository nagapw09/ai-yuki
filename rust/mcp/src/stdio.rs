//! Транспорт stdio (ТЗ §19).
//!
//! Самый частый способ подключить MCP-сервер: он запускается как обычный процесс,
//! а JSON-RPC ходит построчно через его стандартный ввод-вывод.
//!
//! # Почему отдельный поток чтения
//!
//! Блокирующее чтение из stdout не имеет таймаута: зависший сервер повесил бы
//! вместе с собой и вызывающий код. Поэтому строки читает отдельный поток и
//! кладёт их в канал, а запрос ждёт ответа с крайним сроком. Зависший сервер
//! в худшем случае оставляет висеть свой поток чтения — он завершится, когда
//! процесс будет убит вместе с транспортом.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::protocol::{McpError, McpResult};

/// Сколько ждём ответа на один запрос.
///
/// Щедро: MCP-сервер может подниматься через `npx`, докачивая пакет на первом
/// запуске, и это законно занимает десятки секунд.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

struct Inner {
    child: Child,
    stdin: ChildStdin,
    incoming: Receiver<Value>,
    next_id: u64,
}

/// Сколько последних строк stderr держать для диагностики.
///
/// Сервер, упавший на старте, закрывает вывод — и клиент видит только это.
/// Причина при этом почти всегда написана в stderr, и без неё пользователь
/// получает «транспорт разорван» вместо «в скрипте ошибка на строке 12».
const STDERR_TAIL: usize = 10;

pub struct StdioTransport {
    inner: Mutex<Inner>,
    label: String,
    stderr_tail: std::sync::Arc<Mutex<std::collections::VecDeque<String>>>,
}

impl StdioTransport {
    /// Запускает сервер и подключается к его потокам.
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        cwd: Option<&str>,
    ) -> McpResult<Self> {
        let mut builder = Command::new(command);
        // Без явного каталога процесс наследует каталог Yuki — для плагина
        // это чужое место, где нет ни его файлов, ни его зависимостей.
        if let Some(directory) = cwd {
            builder.current_dir(directory);
        }

        let mut child = builder
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr сервера уходит в лог Yuki, а не смешивается с протоколом:
            // одна диагностическая строка в stdout сломала бы разбор JSON-RPC.
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| McpError::Spawn(format!("{command}: {e}")))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| McpError::Spawn("нет доступа к вводу процесса".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| McpError::Spawn("нет доступа к выводу процесса".into()))?;

        let (tx, rx) = mpsc::channel();
        let label = command.to_string();

        let reader_label = label.clone();
        std::thread::Builder::new()
            .name("yuki-mcp-read".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) => {
                            if tx.send(value).is_err() {
                                break;
                            }
                        }
                        // Сервер может писать в stdout не только протокол —
                        // такие строки пропускаем, но не молча.
                        Err(error) => tracing::debug!(
                            server = %reader_label,
                            %error,
                            "непротокольная строка в stdout"
                        ),
                    }
                }
            })
            .map_err(|e| McpError::Spawn(e.to_string()))?;

        let stderr_tail = std::sync::Arc::new(Mutex::new(
            std::collections::VecDeque::<String>::with_capacity(STDERR_TAIL),
        ));

        if let Some(stderr) = child.stderr.take() {
            let stderr_label = label.clone();
            let tail = stderr_tail.clone();
            std::thread::Builder::new()
                .name("yuki-mcp-err".into())
                .spawn(move || {
                    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                        tracing::debug!(server = %stderr_label, "{line}");
                        if let Ok(mut tail) = tail.lock() {
                            if tail.len() == STDERR_TAIL {
                                tail.pop_front();
                            }
                            tail.push_back(line);
                        }
                    }
                })
                .ok();
        }

        Ok(Self {
            inner: Mutex::new(Inner {
                child,
                stdin,
                incoming: rx,
                next_id: 1,
            }),
            label,
            stderr_tail,
        })
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// Последние строки stderr — то, что сервер успел сказать перед смертью.
    fn stderr_tail(&self) -> String {
        self.stderr_tail
            .lock()
            .map(|tail| tail.iter().cloned().collect::<Vec<_>>().join("\n"))
            .unwrap_or_default()
    }

    /// Дополняет причину обрыва тем, что сервер написал в stderr.
    fn with_stderr(&self, reason: &str) -> McpError {
        let tail = self.stderr_tail();
        McpError::Transport(if tail.is_empty() {
            reason.to_string()
        } else {
            format!("{reason}. Сервер сообщил:\n{tail}")
        })
    }

    fn write_line(stdin: &mut ChildStdin, value: &Value) -> McpResult<()> {
        let mut line = serde_json::to_string(value)
            .map_err(|e| McpError::Decode(e.to_string()))?;
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|_| stdin.flush())
            .map_err(|e| McpError::Transport(e.to_string()))
    }

    /// Отправляет запрос и ждёт ответ с нужным идентификатором.
    pub fn request(&self, method: &str, params: Value) -> McpResult<Value> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| McpError::Transport("состояние транспорта повреждено".into()))?;

        let id = inner.next_id;
        inner.next_id += 1;

        let body = crate::protocol::request(id, method, params);
        Self::write_line(&mut inner.stdin, &body)?;

        let deadline = Instant::now() + REQUEST_TIMEOUT;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(McpError::Timeout);
            }

            match inner.incoming.recv_timeout(left) {
                Ok(value) => {
                    // Пока ждём свой ответ, сервер может слать уведомления
                    // и ответы на другие запросы — они не наши.
                    if value.get("id").and_then(Value::as_u64) == Some(id) {
                        return crate::protocol::parse_response(&value);
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Err(McpError::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    // Освобождаем блокировку до чтения stderr: он живёт под
                    // своим мьютексом, но держать два разом незачем.
                    drop(inner);
                    return Err(self.with_stderr("сервер закрыл вывод"));
                }
            }
        }
    }

    /// Отправляет уведомление — ответа на него не будет.
    pub fn notify(&self, method: &str, params: Value) -> McpResult<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| McpError::Transport("состояние транспорта повреждено".into()))?;
        let body = crate::protocol::notification(method, params);
        Self::write_line(&mut inner.stdin, &body)
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Сервер запущен нами и нам его и закрывать: без этого отключённая
        // возможность оставляет за собой живой процесс.
        if let Ok(mut inner) = self.inner.lock() {
            let _ = inner.child.kill();
            let _ = inner.child.wait();
        }
    }
}
