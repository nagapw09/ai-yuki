// Точка входа. На Windows в релизе прячем консольное окно: ассистент запускается
// автозагрузкой, и мигающая консоль при старте выглядит как чужеродный артефакт.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    yuki_desktop_lib::run();
}
