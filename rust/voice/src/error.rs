//! Ошибки голосового конвейера (ТЗ §10).

#[derive(Debug, thiserror::Error)]
pub enum VoiceError {
    #[error("микрофон не найден")]
    NoInputDevice,

    #[error("нет разрешения на микрофон")]
    PermissionDenied,

    #[error("ошибка аудиоустройства: {0}")]
    Device(String),

    #[error("не удалось закодировать звук: {0}")]
    Encode(String),

    #[error("сеть недоступна: {0}")]
    Network(String),

    #[error("распознавание не удалось: {0}")]
    Stt(String),

    #[error("синтез речи не удался: {0}")]
    Tts(String),

    #[error("голосовой режим уже запущен")]
    AlreadyRunning,

    #[error("голосовой режим не запущен")]
    NotRunning,

    #[error("распознавание речи не настроено: укажите модель в настройках голоса")]
    SttNotConfigured,
}

pub type VoiceResult<T> = Result<T, VoiceError>;

impl VoiceError {
    /// Стоит ли повторить попытку (ТЗ §33).
    ///
    /// Сеть и сервис распознавания могут отвалиться на секунду; отсутствующий
    /// микрофон и отказ в разрешении повтором не лечатся.
    pub fn is_retryable(&self) -> bool {
        matches!(self, VoiceError::Network(_) | VoiceError::Stt(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_and_hardware_failures_are_not_retried() {
        assert!(!VoiceError::PermissionDenied.is_retryable());
        assert!(!VoiceError::NoInputDevice.is_retryable());
        assert!(VoiceError::Network("таймаут".into()).is_retryable());
    }
}
