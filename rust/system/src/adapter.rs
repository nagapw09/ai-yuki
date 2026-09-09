use crate::error::SystemResult;
use crate::types::*;

/// Управление приложениями, окнами и системой (ТЗ §30).
///
/// Реализации: [`yuki_windows::WindowsAdapter`] и [`yuki_macos::MacOsAdapter`].
/// Слой выше по стеку не имеет права знать, какая из них подставлена.
pub trait SystemAdapter: Send + Sync {
    /// Запустить приложение по имени, bundle id или пути.
    ///
    /// Возвращает информацию о запущенном процессе — без неё агент не имеет права
    /// сообщать об успехе (ТЗ §5).
    fn open_app(&self, app: &str) -> SystemResult<AppInfo>;

    /// Завершить приложение. `force` — жёсткое завершение вместо запроса на закрытие.
    fn close_app(&self, app: &str, force: bool) -> SystemResult<()>;

    /// Список приложений с видимыми окнами.
    fn list_apps(&self) -> SystemResult<Vec<AppInfo>>;

    /// Все окна верхнего уровня.
    fn list_windows(&self) -> SystemResult<Vec<WindowInfo>>;

    /// Вывести окно на передний план и передать ему фокус.
    fn focus_window(&self, window_id: u64) -> SystemResult<()>;

    /// Активное окно, если оно есть.
    fn active_window(&self) -> SystemResult<Option<WindowInfo>>;

    /// Громкость системного вывода, 0.0–1.0.
    fn set_volume(&self, level: f32) -> SystemResult<()>;

    /// Текущая громкость системного вывода, 0.0–1.0.
    fn volume(&self) -> SystemResult<f32>;

    /// Сводка о системе.
    fn system_info(&self) -> SystemResult<SystemInfo>;
}

/// Файловые операции (ТЗ §8, §30).
pub trait FileAdapter: Send + Sync {
    fn search(&self, query: &FileQuery) -> SystemResult<Vec<FileEntry>>;
    fn read(&self, path: &str) -> SystemResult<Vec<u8>>;
    fn write(&self, path: &str, contents: &[u8]) -> SystemResult<()>;
    fn move_to(&self, from: &str, to: &str) -> SystemResult<()>;
    fn copy(&self, from: &str, to: &str) -> SystemResult<()>;
    /// Удалить файл или каталог.
    ///
    /// ТЗ §22 относит удаление к HIGH risk, поэтому вызов сюда обязан приходить
    /// уже после подтверждения пользователя. `to_trash` по умолчанию — корзина,
    /// а не безвозвратное удаление.
    fn delete(&self, path: &str, to_trash: bool) -> SystemResult<()>;
    fn stat(&self, path: &str) -> SystemResult<FileEntry>;
    /// Открыть файл в приложении по умолчанию.
    fn open(&self, path: &str) -> SystemResult<()>;
}

/// Клавиатура и мышь (ТЗ §6, §30).
pub trait InputAdapter: Send + Sync {
    /// Напечатать текст как последовательность нажатий.
    fn type_text(&self, text: &str) -> SystemResult<()>;

    /// Нажать клавишу с модификаторами. Имя клавиши — в нотации `key_name`:
    /// `a`, `enter`, `escape`, `f5`, `left`, ...
    fn press_key(&self, key: &str, modifiers: &[Modifier]) -> SystemResult<()>;

    fn mouse_move(&self, x: i32, y: i32) -> SystemResult<()>;
    fn mouse_click(&self, button: MouseButton) -> SystemResult<()>;
    fn mouse_scroll(&self, dx: i32, dy: i32) -> SystemResult<()>;
    fn cursor_position(&self) -> SystemResult<(i32, i32)>;
}

/// Экран: снимок и accessibility-дерево (ТЗ §6, §30).
pub trait ScreenAdapter: Send + Sync {
    /// Снимок монитора. `display_index = None` — основной монитор.
    fn capture(&self, display_index: Option<usize>) -> SystemResult<ScreenCapture> {
        self.capture_with(&CaptureOptions {
            display_index,
            ..Default::default()
        })
    }

    /// Снимок с областью и ограничением размера (ТЗ §6).
    fn capture_with(&self, options: &CaptureOptions) -> SystemResult<ScreenCapture>;

    /// Снимок конкретного окна.
    fn capture_window(&self, window_id: u64) -> SystemResult<ScreenCapture>;

    /// Accessibility-дерево окна. `window_id = None` — активное окно.
    ///
    /// Приоритетный способ понять интерфейс: он структурный, точный и не требует
    /// vision-модели. CV подключается, только когда дерево недоступно (ТЗ §6).
    fn accessibility_tree(&self, window_id: Option<u64>) -> SystemResult<AccessibilityNode>;

    fn display_count(&self) -> SystemResult<usize>;
}

/// Буфер обмена (ТЗ §16 actions, §24 context awareness).
pub trait ClipboardAdapter: Send + Sync {
    fn read_text(&self) -> SystemResult<Option<String>>;
    fn write_text(&self, text: &str) -> SystemResult<()>;
}

/// Полный набор адаптеров платформы.
///
/// Собирается один раз при старте и раздаётся как `&'static`, чтобы Tauri-команды
/// не пересоздавали платформенные объекты на каждый вызов.
pub struct PlatformAdapters {
    pub system: Box<dyn SystemAdapter>,
    pub files: Box<dyn FileAdapter>,
    pub input: Box<dyn InputAdapter>,
    pub screen: Box<dyn ScreenAdapter>,
    pub clipboard: Box<dyn ClipboardAdapter>,
}
