//! Диагностика чтения интерфейса: `cargo run -p yuki-accessibility --example ui_check`.
//!
//! Читает дерево активного окна и печатает его в том же виде, в каком его увидит
//! модель. Нужен, чтобы проверить две вещи, которые из кода не видны: доступен ли
//! на этой платформе accessibility-слой вообще и укладывается ли реальное окно
//! в ограничения обхода.
//!
//! Окно берётся активное, поэтому у примера есть пауза: успейте переключиться
//! на то приложение, которое хотите прочитать.

use std::time::Duration;

const DELAY: Duration = Duration::from_secs(3);

fn main() {
    let provider = yuki_accessibility::provider();

    println!("разрешение на чтение интерфейса: {}", provider.is_permitted());
    println!("через {} с читаю активное окно…", DELAY.as_secs());
    std::thread::sleep(DELAY);

    match provider.tree(None) {
        Ok(tree) => {
            let text = yuki_accessibility::render(&tree);
            let lines = text.lines().count();
            println!("узлов в дереве: {lines}, символов: {}\n", text.len());

            // Печатаем начало: полное дерево крупного окна не влезет в терминал,
            // а для проверки достаточно увидеть, что роли и имена на месте.
            for line in text.lines().take(40) {
                println!("{line}");
            }
            if lines > 40 {
                println!("… ещё {} строк", lines - 40);
            }
        }
        Err(error) => {
            eprintln!("не удалось прочитать интерфейс: {error}");
            std::process::exit(1);
        }
    }
}
