use serde::{Deserialize, Serialize};

/// Запущенное приложение (ТЗ §6).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// PID процесса.
    pub pid: u32,
    /// Имя исполняемого файла или bundle id на macOS.
    pub name: String,
    /// Полный путь к исполняемому файлу, если удалось определить.
    pub path: Option<String>,
}

/// Окно рабочего стола (ТЗ §6, §30 `listWindows`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    /// Платформенный идентификатор окна: HWND на Windows, window id на macOS.
    pub id: u64,
    pub title: String,
    pub app_name: String,
    pub pid: u32,
    pub bounds: Rect,
    pub is_focused: bool,
    pub is_minimized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Сводка о системе (ТЗ §30 `getSystemInfo`, §35 System Monitor).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub platform: String,
    pub os_version: String,
    pub arch: String,
    pub hostname: String,
    pub cpu_count: usize,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: u64,
}

/// Элемент файловой системы (ТЗ §8, §30 `FileAdapter`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    /// Unix-время последней модификации в секундах.
    pub modified_at: Option<i64>,
    pub extension: Option<String>,
}

/// Критерии поиска по файловой системе.
///
/// Сценарий ТЗ §41.2 «найди последний PDF в Downloads» ложится сюда целиком:
/// `root = Downloads`, `extensions = ["pdf"]`, `sort = ModifiedDesc`, `limit = 1`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileQuery {
    /// Каталог, с которого начинается обход.
    pub root: String,
    /// Подстрока в имени файла, регистронезависимо.
    pub name_contains: Option<String>,
    /// Допустимые расширения без точки.
    pub extensions: Vec<String>,
    /// Максимальная глубина обхода; `None` — без ограничения.
    pub max_depth: Option<usize>,
    pub sort: FileSort,
    pub limit: usize,
    pub include_hidden: bool,
}

impl FileQuery {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            name_contains: None,
            extensions: Vec::new(),
            max_depth: Some(8),
            sort: FileSort::ModifiedDesc,
            limit: 100,
            include_hidden: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSort {
    NameAsc,
    #[default]
    ModifiedDesc,
    SizeDesc,
}

/// Кнопка мыши (ТЗ §30 `InputAdapter`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Модификатор клавиатуры. `Meta` — Win на Windows, Cmd на macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

/// Снимок экрана (ТЗ §6, §30 `ScreenAdapter::capture`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCapture {
    pub width: u32,
    pub height: u32,
    /// PNG-данные в base64 — в таком виде их принимает vision-модель.
    pub png_base64: String,
    /// Индекс монитора, с которого сделан снимок.
    pub display_index: usize,
}

/// Распознанная строка текста на изображении (ТЗ §6).
///
/// Прямоугольник — в координатах изображения, а не экрана: снимок мог быть
/// кадрирован и уменьшен, и пересчёт в экранные делает тот, кто снимал.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextLine {
    pub text: String,
    pub rect: Rect,
}

/// Что и как снимать (ТЗ §6).
///
/// Отдельной структурой, а не тремя аргументами: снимок всего экрана в полном
/// разрешении стоит сотни килобайт на каждый шаг агента, и параметры, которые
/// это уменьшают, должны быть видны в одном месте, а не разъезжаться по
/// вызовам.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureOptions {
    /// Монитор; `None` — основной.
    pub display_index: Option<usize>,
    /// Прямоугольник в координатах монитора; `None` — весь экран.
    pub region: Option<Rect>,
    /// Ограничение ширины: кадр уменьшается пропорционально, если шире.
    pub max_width: Option<u32>,
}

/// Узел accessibility-дерева (ТЗ §6).
///
/// ТЗ прямо требует: нативные Accessibility/UI API — приоритет, computer vision и
/// симуляция мыши — только fallback. Этот тип и есть приоритетный путь.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessibilityNode {
    /// Роль элемента в терминах платформы: button, textfield, window, ...
    pub role: String,
    pub name: Option<String>,
    pub value: Option<String>,
    pub bounds: Option<Rect>,
    pub enabled: bool,
    pub focused: bool,
    /// Действия, которые элемент объявляет: press, focus, increment, ...
    pub actions: Vec<String>,
    pub children: Vec<AccessibilityNode>,
}
