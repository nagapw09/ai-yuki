//! Клиент MCP: рукопожатие, список инструментов, вызов (ТЗ §19).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Value};

use crate::http::HttpTransport;
use crate::protocol::{
    self, McpCallResult, McpError, McpResult, McpTool, ServerInfo, PROTOCOL_VERSION,
};
use crate::stdio::StdioTransport;

/// Как подключаться к серверу (ТЗ §19: transport, URL/command, arguments, environment).
#[derive(Debug, Clone)]
pub enum Transport {
    /// Локальный процесс: JSON-RPC через его стандартный ввод-вывод.
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        /// Рабочий каталог процесса.
        ///
        /// Нужен плагинам (ТЗ §20): их сервер запускается из своей папки
        /// и ищет соседние файлы относительно неё, а не относительно того,
        /// где оказался запущен сам ассистент.
        cwd: Option<String>,
    },
    /// Удалённый сервер по HTTP.
    Http {
        url: String,
        /// Заголовок авторизации целиком; собирается из ключа в хранилище ОС.
        authorization: Option<String>,
    },
}

impl Transport {
    /// Разбирает описание транспорта из настроек.
    pub fn from_parts(
        kind: &str,
        command: Option<String>,
        args: Vec<String>,
        env: HashMap<String, String>,
        url: Option<String>,
        authorization: Option<String>,
    ) -> McpResult<Self> {
        match kind {
            "stdio" => Ok(Transport::Stdio {
                command: command.ok_or_else(|| {
                    McpError::Spawn("для stdio не задана команда запуска".into())
                })?,
                args,
                env,
                cwd: None,
            }),
            // sse и http различаются формой ответа, а не запросом: клиент
            // объявляет, что принимает оба, и разбирает то, что пришло.
            "http" | "sse" => Ok(Transport::Http {
                url: url.ok_or_else(|| McpError::Spawn("для http не задан адрес".into()))?,
                authorization,
            }),
            other => Err(McpError::UnknownTransport(other.to_string())),
        }
    }

    /// Задаёт рабочий каталог для stdio.
    ///
    /// Отдельным методом, а не ещё одним аргументом [`Transport::from_parts`]:
    /// каталог есть только у плагинов, а у остальных серверов его нет,
    /// и добавлять всем вызовам `None` ради одного случая — шум.
    pub fn with_cwd(self, directory: Option<String>) -> Self {
        match self {
            Transport::Stdio {
                command, args, env, ..
            } => Transport::Stdio {
                command,
                args,
                env,
                cwd: directory,
            },
            other => other,
        }
    }
}

enum Channel {
    // Под Arc, чтобы транспорт можно было отдать в блокирующую задачу, не
    // одалживая её из `&self` через сырой указатель.
    Stdio(Arc<StdioTransport>),
    Http(HttpTransport),
}

/// Подключённый MCP-сервер.
pub struct McpClient {
    channel: Channel,
    info: ServerInfo,
    tools: Vec<McpTool>,
}

impl McpClient {
    /// Подключается, здоровается и забирает список инструментов.
    ///
    /// Всё три шага вместе, потому что порознь они бессмысленны: сервер без
    /// рукопожатия отвергнет вызовы, а сервер без инструментов ничего не даёт
    /// Yuki. Полупроведённое подключение — это состояние, которое пришлось бы
    /// отображать в UI и объяснять пользователю.
    pub async fn connect(transport: Transport, http: reqwest::Client) -> McpResult<Self> {
        let channel = match transport {
            Transport::Stdio {
                command,
                args,
                env,
                cwd,
            } => {
                // Запуск процесса блокирует, поэтому уводим его с исполнителя
                // асинхронных задач.
                let spawned = tokio_spawn_blocking(move || {
                    StdioTransport::spawn(&command, &args, &env, cwd.as_deref())
                })
                .await?;
                Channel::Stdio(Arc::new(spawned?))
            }
            Transport::Http { url, authorization } => {
                Channel::Http(HttpTransport::new(url, authorization, http))
            }
        };

        let client = Self {
            channel,
            info: ServerInfo {
                name: String::new(),
                version: String::new(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
            tools: Vec::new(),
        };

        client.handshake().await
    }

    async fn handshake(mut self) -> McpResult<Self> {
        let result = self
            .call_raw("initialize", protocol::initialize_params())
            .await?;

        self.info = ServerInfo {
            name: result["serverInfo"]["name"]
                .as_str()
                .unwrap_or("сервер")
                .to_string(),
            version: result["serverInfo"]["version"]
                .as_str()
                .unwrap_or("?")
                .to_string(),
            protocol_version: result["protocolVersion"]
                .as_str()
                .unwrap_or(PROTOCOL_VERSION)
                .to_string(),
        };

        // Протокол требует подтвердить готовность до любых других вызовов.
        self.notify_raw("notifications/initialized", json!({})).await?;

        let listed = self.call_raw("tools/list", json!({})).await?;
        self.tools = protocol::parse_tools(&listed);

        Ok(self)
    }

    pub fn info(&self) -> &ServerInfo {
        &self.info
    }

    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    /// Перечитывает список инструментов: сервер мог их изменить.
    pub async fn refresh_tools(&mut self) -> McpResult<()> {
        let listed = self.call_raw("tools/list", json!({})).await?;
        self.tools = protocol::parse_tools(&listed);
        Ok(())
    }

    /// Вызывает инструмент сервера.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> McpResult<McpCallResult> {
        let result = self
            .call_raw(
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await?;
        Ok(protocol::parse_call_result(&result))
    }

    async fn call_raw(&self, method: &str, params: Value) -> McpResult<Value> {
        match &self.channel {
            Channel::Http(http) => http.request(method, params).await,
            Channel::Stdio(stdio) => {
                // Обмен по stdio блокирующий; уводим его с исполнителя задач,
                // иначе он застопорит весь асинхронный рантайм приложения.
                let stdio = stdio.clone();
                let method = method.to_string();
                tokio_spawn_blocking(move || stdio.request(&method, params)).await?
            }
        }
    }

    async fn notify_raw(&self, method: &str, params: Value) -> McpResult<()> {
        match &self.channel {
            Channel::Http(http) => http.notify(method, params).await,
            Channel::Stdio(stdio) => stdio.notify(method, params),
        }
    }
}

/// Выполняет блокирующую работу вне асинхронного исполнителя.
async fn tokio_spawn_blocking<T, F>(f: F) -> McpResult<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| McpError::Transport(format!("задача прервана: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stdio_needs_a_command() {
        let result = Transport::from_parts(
            "stdio",
            None,
            Vec::new(),
            HashMap::new(),
            None,
            None,
        );
        assert!(matches!(result, Err(McpError::Spawn(_))));
    }

    #[test]
    fn http_needs_a_url() {
        let result =
            Transport::from_parts("http", None, Vec::new(), HashMap::new(), None, None);
        assert!(matches!(result, Err(McpError::Spawn(_))));
    }

    #[test]
    fn sse_and_http_share_one_transport() {
        for kind in ["http", "sse"] {
            let transport = Transport::from_parts(
                kind,
                None,
                Vec::new(),
                HashMap::new(),
                Some("https://example.com/mcp".into()),
                None,
            )
            .expect("транспорт должен собраться");
            assert!(matches!(transport, Transport::Http { .. }));
        }
    }

    #[test]
    fn rejects_unknown_transports_by_name() {
        let result = Transport::from_parts(
            "carrier-pigeon",
            None,
            Vec::new(),
            HashMap::new(),
            None,
            None,
        );
        assert!(matches!(result, Err(McpError::UnknownTransport(_))));
    }
}
