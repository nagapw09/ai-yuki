//! Клиент Model Context Protocol (ТЗ §19).
//!
//! MCP — способ подключить к Yuki чужие инструменты, не встраивая их в неё.
//! Сервер объявляет, что умеет, Yuki переносит это в свой реестр, и модель
//! видит чужие возможности рядом со своими.
//!
//! Поддержаны оба транспорта из ТЗ §19: локальный процесс через stdio и
//! удалённый сервер по HTTP. Из методов протокола реализованы рукопожатие,
//! список инструментов и вызов — ресурсы и промпты Capability Hub пока не
//! использует, и поддерживать неиспользуемый код незачем.

pub mod client;
pub mod http;
pub mod protocol;
pub mod stdio;

pub use client::{McpClient, Transport};
pub use protocol::{McpCallResult, McpError, McpResult, McpTool, ServerInfo};
