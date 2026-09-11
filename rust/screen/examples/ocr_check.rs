//! Проверка распознавания текста на живом экране.
//!
//! ```text
//! cargo run -p yuki-screen --example ocr_check
//! cargo run -p yuki-screen --example ocr_check -- 0 400 300 800 200
//! ```
//!
//! Второй вариант — область: монитор, x, y, ширина, высота. Область полезнее
//! целого экрана: распознавание тем точнее, чем меньше лишнего вокруг текста.
//!
//! Утилита существует по той же причине, что `mic_check` и `ui_check`: движок
//! распознавания зависит от установленных в системе языков, и единственный
//! способ узнать, что он видит на этой машине, — посмотреть.

use yuki_screen::DesktopScreenAdapter;
use yuki_system::{CaptureOptions, Rect, ScreenAdapter};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let options = match args.len() {
        0 => CaptureOptions::default(),
        5 => CaptureOptions {
            display_index: args[0].parse().ok(),
            region: Some(Rect {
                x: args[1].parse().unwrap_or(0),
                y: args[2].parse().unwrap_or(0),
                width: args[3].parse().unwrap_or(0),
                height: args[4].parse().unwrap_or(0),
            }),
            // Без уменьшения: мелкий текст после уменьшения перестаёт
            // распознаваться, а ради этого область и берут.
            max_width: None,
        },
        _ => {
            eprintln!("нужно либо ничего, либо пять чисел: монитор x y ширина высота");
            std::process::exit(2);
        }
    };

    let adapter = DesktopScreenAdapter::new();

    let started = std::time::Instant::now();
    match adapter.recognize_text(&options) {
        Ok(lines) if lines.is_empty() => {
            println!("текст не найден за {:?}", started.elapsed());
            println!(
                "если на экране текст есть — проверьте, установлен ли в системе \
                 языковой пакет с распознаванием"
            );
        }
        Ok(lines) => {
            println!("строк: {} за {:?}\n", lines.len(), started.elapsed());
            for line in lines {
                println!(
                    "[{:>5},{:>5} {:>4}x{:<4}] {}",
                    line.rect.x, line.rect.y, line.rect.width, line.rect.height, line.text
                );
            }
        }
        Err(error) => {
            eprintln!("не получилось: {error}");
            std::process::exit(1);
        }
    }
}
