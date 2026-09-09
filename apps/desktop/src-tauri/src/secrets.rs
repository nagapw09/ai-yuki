//! Секреты в системном хранилище ОС (ТЗ §29, §31).
//!
//! API keys и токены не попадают ни в базу, ни в журнал активности (ТЗ §23) — только
//! в Keychain на macOS и Credential Manager на Windows. В таблицах остаётся `secret_ref`:
//! строковый идентификатор, по которому значение достаётся отсюда.

use keyring::Entry;

/// Имя сервиса в системном хранилище. Меняться не должно: по нему пользователь
/// находит записи Yuki в Keychain / Credential Manager.
const SERVICE: &str = "ai.yuki.desktop";

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("хранилище секретов недоступно: {0}")]
    Backend(String),

    #[error("пустая ссылка на секрет")]
    EmptyRef,
}

pub type SecretResult<T> = Result<T, SecretError>;

fn entry(secret_ref: &str) -> SecretResult<Entry> {
    if secret_ref.trim().is_empty() {
        return Err(SecretError::EmptyRef);
    }
    Entry::new(SERVICE, secret_ref).map_err(|e| SecretError::Backend(e.to_string()))
}

/// Сохраняет секрет под ссылкой `secret_ref`.
pub fn set(secret_ref: &str, value: &str) -> SecretResult<()> {
    entry(secret_ref)?
        .set_password(value)
        .map_err(|e| SecretError::Backend(e.to_string()))
}

/// Читает секрет. `Ok(None)` — записи нет; это штатная ситуация, а не ошибка.
pub fn get(secret_ref: &str) -> SecretResult<Option<String>> {
    match entry(secret_ref)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError::Backend(e.to_string())),
    }
}

/// Удаляет секрет. Отсутствие записи считается успехом: результат тот же.
pub fn delete(secret_ref: &str) -> SecretResult<()> {
    match entry(secret_ref)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Backend(e.to_string())),
    }
}

/// Есть ли значение по ссылке. Используется UI, чтобы показать «ключ задан»,
/// не вытаскивая сам ключ в интерфейс.
pub fn exists(secret_ref: &str) -> bool {
    matches!(get(secret_ref), Ok(Some(_)))
}

/// Ссылка на ключ провайдера: `provider:openai`, `provider:anthropic`, ...
pub fn provider_ref(provider_id: &str) -> String {
    format!("provider:{provider_id}")
}

/// Ссылка на учётные данные MCP-сервера (ТЗ §19).
pub fn mcp_ref(server_id: &str) -> String {
    format!("mcp:{server_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_reference() {
        assert!(matches!(set("", "x"), Err(SecretError::EmptyRef)));
        assert!(matches!(get("   "), Err(SecretError::EmptyRef)));
    }

    #[test]
    fn builds_namespaced_refs() {
        assert_eq!(provider_ref("openai"), "provider:openai");
        assert_eq!(mcp_ref("github"), "mcp:github");
    }
}
