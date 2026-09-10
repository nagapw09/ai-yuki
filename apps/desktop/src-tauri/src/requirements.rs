//! Минимальные системные требования и их проверка.
//!
//! ТЗ §37 задаёт цели по скорости, но не задаёт нижнюю планку железа — это
//! пропуск, найденный сверкой с Astra (`docs/GAPS.md`, пункт 4). Планка нужна
//! не ради строчки в описании: Yuki с локальной моделью и Yuki с облачной — это
//! требования, различающиеся вдвое, и человек должен узнать об этом до того,
//! как включит Local Only и получит ответ через минуту.
//!
//! # Почему проверка живая, а не абзац в README
//!
//! Список требований, который негде проверить, читают один раз и забывают.
//! Здесь он сравнивается с настоящей машиной, а результат показывается в
//! мастере первого запуска и в настройках. Требования и их обоснование —
//! `docs/REQUIREMENTS.md`.

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// Минимальная мажорная версия Windows.
///
/// Десятка, а не «любая»: Tauri рисует окно в WebView2, а он ставится только на
/// Windows 10 версии 1803 и новее. На более старой системе приложение не
/// запустится вовсе, и честнее сказать это заранее.
const MIN_WINDOWS: u32 = 10;

/// Минимальная мажорная версия macOS — из ТЗ §2.
const MIN_MACOS: u32 = 13;

/// Ядер процессора.
///
/// Четыре: одно уходит на WebView, одно на сам процесс, и как минимум два
/// должны остаться приложению, которым Yuki управляет. На двух ядрах интерфейс
/// начинает спотыкаться ровно тогда, когда идёт работа, — то есть всегда.
const MIN_CORES: usize = 4;

/// Оперативной памяти для работы с облачной моделью.
const MIN_MEMORY_GB: f64 = 8.0;

/// Оперативной памяти для режима Local Only.
///
/// Локальная модель уровня 7–8B в четырёхбитном кванте занимает 5–6 ГБ, и это
/// сверх всего остального, что уже открыто у человека.
const LOCAL_ONLY_MEMORY_GB: f64 = 16.0;

/// Памяти для окна аватара.
///
/// Отдельным порогом, потому что аватар — это ещё один WebView с WebGL и
/// сценой three.js: он стоит заметно дороже остального интерфейса.
const AVATAR_MEMORY_GB: f64 = 8.0;

const BYTES_IN_GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Одно требование и то, чем на него отвечает машина.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Requirement {
    pub key: String,
    pub label: String,
    /// Что требуется — человеческой строкой, а не числом.
    pub required: String,
    /// Что есть на самом деле.
    pub actual: String,
    pub ok: bool,
}

/// Итог проверки.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequirementsReport {
    /// Выполнены ли базовые требования.
    pub ok: bool,
    pub items: Vec<Requirement>,
    /// Хватит ли памяти на окно аватара (ТЗ §12).
    pub avatar_ok: bool,
    /// Хватит ли памяти на локальную модель (ТЗ §29).
    pub local_only_ok: bool,
}

/// Мажорная версия из строки вида `11`, `14.5`, `10.0.26200`.
///
/// Отдельной функцией ради теста: строку даёт `sysinfo`, и её вид отличается на
/// двух платформах и меняется между версиями библиотеки. Молча получить 0 из
/// неразобранной строки значит сказать человеку, что его система слишком стара.
pub fn major_version(raw: &str) -> Option<u32> {
    raw.trim()
        .split(['.', ' ', '-'])
        .find(|part| !part.is_empty())
        .and_then(|part| part.parse().ok())
}

/// Собирает отчёт по живым данным о системе.
pub fn report(info: &yuki_system::SystemInfo) -> RequirementsReport {
    let memory_gb = info.total_memory_bytes as f64 / BYTES_IN_GB;

    let (os_label, min_os) = match info.platform.as_str() {
        "macos" => ("macOS", MIN_MACOS),
        _ => ("Windows", MIN_WINDOWS),
    };

    // Неразобранная версия — это «не знаю», а не «слишком старая». Пугать
    // человека из-за того, что библиотека поменяла формат строки, нельзя.
    let detected = major_version(&info.os_version);
    let os_ok = detected.map(|v| v >= min_os).unwrap_or(true);

    let items = vec![
        Requirement {
            key: "os".into(),
            label: "Операционная система".into(),
            required: format!("{os_label} {min_os} или новее"),
            actual: format!("{os_label} {}", info.os_version),
            ok: os_ok,
        },
        Requirement {
            key: "arch".into(),
            label: "Архитектура".into(),
            required: "x86-64 или ARM64".into(),
            actual: info.arch.clone(),
            // 32-битных сборок нет и не планируется: WebView2 и локальные
            // модели в 32 бита не помещаются по адресному пространству.
            ok: matches!(info.arch.as_str(), "x86_64" | "aarch64" | "arm64"),
        },
        Requirement {
            key: "cpu".into(),
            label: "Ядер процессора".into(),
            required: format!("{MIN_CORES}"),
            actual: format!("{}", info.cpu_count),
            ok: info.cpu_count >= MIN_CORES,
        },
        Requirement {
            key: "memory".into(),
            label: "Оперативная память".into(),
            required: format!("{MIN_MEMORY_GB:.0} ГБ"),
            actual: format!("{memory_gb:.1} ГБ"),
            ok: memory_gb + 0.5 >= MIN_MEMORY_GB,
        },
    ];

    RequirementsReport {
        ok: items.iter().all(|item| item.ok),
        items,
        // Запас в полгигабайта: производитель считает гигабайты по 10^9, часть
        // памяти забирает видеоядро, и машина «на 8 ГБ» показывает 7.6.
        avatar_ok: memory_gb + 0.5 >= AVATAR_MEMORY_GB,
        local_only_ok: memory_gb + 0.5 >= LOCAL_ONLY_MEMORY_GB,
    }
}

#[tauri::command]
pub fn system_requirements(state: State<'_, AppState>) -> Result<RequirementsReport, String> {
    let info = state
        .adapters
        .system
        .system_info()
        .map_err(|e| e.to_string())?;
    Ok(report(&info))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(platform: &str, version: &str, cores: usize, gb: f64) -> yuki_system::SystemInfo {
        yuki_system::SystemInfo {
            platform: platform.into(),
            os_version: version.into(),
            arch: "x86_64".into(),
            hostname: "test".into(),
            cpu_count: cores,
            total_memory_bytes: (gb * BYTES_IN_GB) as u64,
            available_memory_bytes: (gb * BYTES_IN_GB / 2.0) as u64,
        }
    }

    #[test]
    fn reads_the_major_version_out_of_every_shape_these_systems_report() {
        assert_eq!(major_version("11"), Some(11));
        assert_eq!(major_version("10.0.26200"), Some(10));
        assert_eq!(major_version("14.5"), Some(14));
        assert_eq!(major_version(" 13 "), Some(13));
    }

    #[test]
    fn an_unreadable_version_is_not_treated_as_too_old() {
        // «unknown» приходит, когда sysinfo не смог определить версию. Это не
        // повод сказать человеку, что его система не подходит.
        assert_eq!(major_version("unknown"), None);
        let report = report(&info("windows", "unknown", 8, 16.0));
        assert!(report.ok, "{:?}", report.items);
    }

    #[test]
    fn a_modern_machine_passes_everything() {
        let report = report(&info("windows", "11", 16, 32.0));
        assert!(report.ok);
        assert!(report.avatar_ok);
        assert!(report.local_only_ok);
    }

    #[test]
    fn eight_gigabytes_is_enough_for_the_base_but_not_for_a_local_model() {
        let report = report(&info("windows", "11", 8, 8.0));
        assert!(report.ok);
        assert!(report.avatar_ok);
        assert!(!report.local_only_ok, "8 ГБ мало для локальной модели");
    }

    #[test]
    fn a_machine_that_reports_slightly_under_eight_still_counts_as_eight() {
        // Производитель считает гигабайты по 10^9, видеоядро откусывает своё —
        // «восьмигигабайтная» машина показывает 7.6, и отказывать ей нельзя.
        let report = report(&info("windows", "11", 4, 7.6));
        assert!(report.ok, "{:?}", report.items);
    }

    #[test]
    fn windows_older_than_ten_fails_on_the_operating_system_line() {
        let report = report(&info("windows", "8.1", 8, 16.0));
        assert!(!report.ok);
        let os = report.items.iter().find(|i| i.key == "os").expect("строка ОС");
        assert!(!os.ok);
    }

    #[test]
    fn macos_uses_its_own_floor_from_the_spec() {
        assert!(report(&info("macos", "13.0", 8, 16.0)).ok);
        assert!(!report(&info("macos", "12.7", 8, 16.0)).ok);
    }

    #[test]
    fn a_weak_machine_says_exactly_what_is_missing() {
        let report = report(&info("windows", "11", 2, 4.0));
        assert!(!report.ok);

        let failed: Vec<&str> = report
            .items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| i.key.as_str())
            .collect();
        assert_eq!(failed, vec!["cpu", "memory"]);
    }
}
