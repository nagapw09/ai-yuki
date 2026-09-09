//! Диагностика MCP-сервера:
//! `cargo run -p yuki-mcp --example mcp_check -- <команда> [аргументы…]`
//!
//! Подключается к серверу так же, как это делает Capability Hub: здоровается,
//! забирает список инструментов и вызывает первый из них. Нужен, чтобы отличить
//! «сервер не поднялся» от «сервер поднялся, но инструмент не работает» — из
//! интерфейса эти два случая выглядят одинаково.

use std::collections::HashMap;

use serde_json::json;
use yuki_mcp::{McpClient, Transport};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        eprintln!("укажите команду запуска сервера");
        std::process::exit(2);
    };
    let rest: Vec<String> = args.collect();

    println!("запускаю: {command} {}", rest.join(" "));

    let transport = Transport::Stdio {
        command,
        args: rest,
        env: HashMap::new(),
        cwd: None,
    };

    let http = reqwest::Client::new();
    let client = match McpClient::connect(transport, http).await {
        Ok(client) => client,
        Err(error) => {
            eprintln!("не подключился: {error}");
            std::process::exit(1);
        }
    };

    let info = client.info();
    println!(
        "сервер: {} {} (протокол {})",
        info.name, info.version, info.protocol_version
    );

    let tools = client.tools();
    println!("инструментов: {}", tools.len());
    for tool in tools {
        println!("  {} — {}", tool.name, tool.description);
    }

    let Some(first) = tools.first() else {
        println!("\nсервер не объявил ни одного инструмента");
        return;
    };

    // Аргументы собираем по схеме: строкам даём текст, числам — единицу.
    // Этого достаточно, чтобы проверить, что вызов вообще доходит.
    let mut arguments = serde_json::Map::new();
    if let Some(props) = first.input_schema["properties"].as_object() {
        for (name, spec) in props {
            let value = match spec["type"].as_str() {
                Some("number") | Some("integer") => json!(1),
                Some("boolean") => json!(false),
                _ => json!("проверка"),
            };
            arguments.insert(name.clone(), value);
        }
    }

    println!("\nвызываю {} с {}", first.name, json!(arguments));
    match client.call_tool(&first.name, json!(arguments)).await {
        Ok(result) if result.is_error => {
            println!("сервер сообщил об ошибке: {}", result.text);
        }
        Ok(result) => println!("ответ: {}", result.text),
        Err(error) => {
            eprintln!("вызов не удался: {error}");
            std::process::exit(1);
        }
    }
}
