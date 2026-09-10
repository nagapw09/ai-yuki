//! Роль, тон и обращение (`docs/GAPS.md` §7).
//!
//! # Что здесь можно настраивать, а что нет
//!
//! ТЗ §11 фиксирует характер Yuki, а ТЗ §44 задаёт системную инструкцию
//! дословно. Ни то, ни другое отсюда не переопределяется: инструкция
//! идентичности всегда идёт первой и не может быть вытеснена (см.
//! [`crate::ai::chat_send`]). Настраивается **слой поверх** — кем Yuki
//! выступает в разговоре, насколько формально говорит, насколько подробно
//! отвечает и как обращается к человеку.
//!
//! Причина такой границы простая: «не рапортовать о невыполненном» — это не
//! черта характера, которую можно выключить ползунком, а обещание, на котором
//! держится доверие ко всему остальному.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Ключи настроек.
const KEY_ROLE: &str = "persona.role";
const KEY_CUSTOM: &str = "persona.custom";
const KEY_FORMALITY: &str = "persona.formality";
const KEY_VERBOSITY: &str = "persona.verbosity";
const KEY_ADDRESS: &str = "persona.address";

/// Итоговый текст, который дописывается к идентичности.
///
/// Хранится отдельно от составных частей: его читает агентный цикл на каждом
/// запросе, и пересобирать его там означало бы дублировать эту логику в
/// TypeScript.
pub const KEY_COMPOSED: &str = "persona.extra";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Persona {
    /// `assistant` · `coach` · `editor` · `developer` · `custom`
    pub role: String,
    /// Своя формулировка роли — только для `custom`.
    pub custom: String,
    /// 0 — на «ты», 1 — нейтрально, 2 — на «вы».
    pub formality: u8,
    /// 0 — кратко, 1 — обычно, 2 — подробно.
    pub verbosity: u8,
    /// Как обращаться к человеку. Пусто — никак.
    pub address: String,
}

impl Default for Persona {
    fn default() -> Self {
        Self {
            role: "assistant".into(),
            custom: String::new(),
            // Нейтральный тон и обычная подробность: настройка по умолчанию не
            // должна быть чьим-то вкусом.
            formality: 1,
            verbosity: 1,
            address: String::new(),
        }
    }
}

/// Описание роли для системной инструкции.
fn role_line(persona: &Persona) -> Option<String> {
    let text = match persona.role.as_str() {
        "assistant" => return None, // роль по умолчанию описана в §44
        "coach" => {
            "В этом разговоре ты выступаешь наставником: помогаешь разобраться и \
             довести до результата, задаёшь уточняющие вопросы, не решаешь за человека."
        }
        "editor" => {
            "В этом разговоре ты выступаешь редактором: правишь текст, объясняешь \
             правки коротко, сохраняешь авторский голос и не переписываешь без нужды."
        }
        "developer" => {
            "В этом разговоре ты выступаешь помощником разработчика: говоришь кодом \
             и точными командами, называешь файлы и пути, не пересказываешь очевидное."
        }
        "custom" => {
            let custom = persona.custom.trim();
            if custom.is_empty() {
                return None;
            }
            return Some(custom.to_string());
        }
        _ => return None,
    };

    Some(text.to_string())
}

fn formality_line(level: u8) -> Option<&'static str> {
    match level {
        0 => Some("Обращайся к человеку на «ты», говори просто и по-дружески."),
        2 => Some("Обращайся к человеку на «вы», держи деловой тон."),
        // Нейтральный уровень не описывается: лишняя строка в инструкции — это
        // лишний повод модели её отыгрывать.
        _ => None,
    }
}

fn verbosity_line(level: u8) -> Option<&'static str> {
    match level {
        0 => Some("Отвечай очень коротко: результат и, если нужно, одна строка пояснения."),
        2 => Some("Отвечай развёрнуто: поясняй ход мысли и упоминай важные оговорки."),
        _ => None,
    }
}

/// Собирает добавку к системной инструкции.
///
/// Пустая строка означает «ничего не добавлять» — и это нормальный, а не
/// вырожденный случай: настройки по умолчанию не должны ничего дописывать.
pub fn compose(persona: &Persona) -> String {
    let mut lines = Vec::new();

    if let Some(role) = role_line(persona) {
        lines.push(role);
    }
    if let Some(tone) = formality_line(persona.formality) {
        lines.push(tone.to_string());
    }
    if let Some(length) = verbosity_line(persona.verbosity) {
        lines.push(length.to_string());
    }

    let address = persona.address.trim();
    if !address.is_empty() {
        lines.push(format!("Обращайся к человеку по имени: {address}."));
    }

    lines.join("\n")
}

fn setting(state: &AppState, key: &str) -> Option<String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .ok()
        .flatten()
}

fn level(state: &AppState, key: &str, default: u8) -> u8 {
    setting(state, key)
        .and_then(|v| v.parse::<u8>().ok())
        .filter(|v| *v <= 2)
        .unwrap_or(default)
}

// ── Команды ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn persona_get(state: State<'_, AppState>) -> Persona {
    let defaults = Persona::default();

    Persona {
        role: setting(&state, KEY_ROLE).unwrap_or(defaults.role),
        custom: setting(&state, KEY_CUSTOM).unwrap_or_default(),
        formality: level(&state, KEY_FORMALITY, defaults.formality),
        verbosity: level(&state, KEY_VERBOSITY, defaults.verbosity),
        address: setting(&state, KEY_ADDRESS).unwrap_or_default(),
    }
}

/// Сохраняет настройки и пересобирает добавку к инструкции.
#[tauri::command]
pub fn persona_set(state: State<'_, AppState>, persona: Persona) -> Result<String, String> {
    let composed = compose(&persona);

    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
            )?;

            stmt.execute(rusqlite::params![KEY_ROLE, persona.role])?;
            stmt.execute(rusqlite::params![KEY_CUSTOM, persona.custom.trim()])?;
            stmt.execute(rusqlite::params![KEY_FORMALITY, persona.formality.to_string()])?;
            stmt.execute(rusqlite::params![KEY_VERBOSITY, persona.verbosity.to_string()])?;
            stmt.execute(rusqlite::params![KEY_ADDRESS, persona.address.trim()])?;
            stmt.execute(rusqlite::params![KEY_COMPOSED, composed])?;
            Ok(())
        })
        .map_err(err)?;

    Ok(composed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_add_nothing_to_the_instruction() {
        // Каждая лишняя строка — это то, что модель начнёт отыгрывать.
        assert_eq!(compose(&Persona::default()), "");
    }

    #[test]
    fn a_role_changes_how_yuki_behaves_in_the_conversation() {
        let persona = Persona {
            role: "developer".into(),
            ..Persona::default()
        };
        assert!(compose(&persona).contains("разработчика"));
    }

    #[test]
    fn a_custom_role_is_taken_verbatim() {
        let persona = Persona {
            role: "custom".into(),
            custom: "  Ты сварливый библиотекарь.  ".into(),
            ..Persona::default()
        };
        assert_eq!(compose(&persona), "Ты сварливый библиотекарь.");
    }

    #[test]
    fn an_empty_custom_role_falls_back_to_saying_nothing() {
        let persona = Persona {
            role: "custom".into(),
            custom: "   ".into(),
            ..Persona::default()
        };
        assert_eq!(compose(&persona), "");
    }

    #[test]
    fn tone_and_length_stack_with_the_role() {
        let persona = Persona {
            role: "coach".into(),
            formality: 0,
            verbosity: 0,
            address: "Алексей".into(),
            ..Persona::default()
        };

        let text = compose(&persona);
        assert_eq!(text.lines().count(), 4);
        assert!(text.contains("наставником"));
        assert!(text.contains("«ты»"));
        assert!(text.contains("коротко"));
        assert!(text.contains("Алексей"));
    }

    #[test]
    fn an_unknown_role_is_ignored_rather_than_invented() {
        let persona = Persona {
            role: "звездочёт".into(),
            ..Persona::default()
        };
        assert_eq!(compose(&persona), "");
    }

    #[test]
    fn the_composed_text_never_touches_the_identity() {
        // Идентичность из ТЗ §44 дописывается перед этим текстом и не может
        // быть отменена настройкой: проверяем, что мы не сочиняем инструкций
        // вида «забудь предыдущее».
        for role in ["assistant", "coach", "editor", "developer"] {
            let persona = Persona {
                role: role.into(),
                formality: 0,
                verbosity: 2,
                ..Persona::default()
            };
            let text = compose(&persona).to_lowercase();
            assert!(!text.contains("забудь"));
            assert!(!text.contains("игнорируй"));
        }
    }
}
