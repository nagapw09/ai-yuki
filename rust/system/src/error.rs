use std::fmt;

/// Ошибка системного слоя.
///
/// ТЗ §5 и §44: Yuki не имеет права сообщить об успехе без подтверждения инструмента,
/// поэтому у операций нет «наверное получилось» — только `Ok` с полезным результатом
/// или явная причина отказа.
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    /// Возможность не реализована на текущей платформе.
    #[error("не поддерживается на этой платформе: {0}")]
    Unsupported(&'static str),

    /// Возможность объявлена, но ещё не реализована в этой фазе.
    #[error("ещё не реализовано: {0}")]
    NotImplemented(&'static str),

    /// Нет системного разрешения ОС (ТЗ §21).
    #[error("нет разрешения: {0}")]
    PermissionDenied(String),

    /// Цель операции не найдена: приложение, окно, файл.
    #[error("не найдено: {0}")]
    NotFound(String),

    /// Ошибка ввода-вывода.
    #[error("ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),

    /// Платформенный вызов вернул ошибку.
    #[error("сбой платформенного вызова: {0}")]
    Platform(String),

    /// Аргументы операции некорректны.
    #[error("некорректный аргумент: {0}")]
    InvalidArgument(String),
}

pub type SystemResult<T> = Result<T, SystemError>;

/// Идентификатор платформы, на которой собран бинарь (ТЗ §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    MacOS,
}

impl Platform {
    pub const fn current() -> Option<Self> {
        #[cfg(target_os = "windows")]
        {
            Some(Platform::Windows)
        }
        #[cfg(target_os = "macos")]
        {
            Some(Platform::MacOS)
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            None
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Platform::Windows => f.write_str("windows"),
            Platform::MacOS => f.write_str("macos"),
        }
    }
}
