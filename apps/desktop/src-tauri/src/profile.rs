//! Профили персонажа (ТЗ §11, `docs/GAPS.md` §7).
//!
//! # Зачем отдельный слой над настройками
//!
//! Модель, папка с анимациями, характер, обращение, слово пробуждения и имя —
//! это не шесть независимых переключателей, а один облик. Пока они лежали
//! отдельно, смена персонажа складывалась из шести походов в разные части
//! настроек, и половина забывалась: новая модель говорила прежним голосом и
//! откликалась на прежнее имя.
//!
//! # Профиль — это снимок, а не ссылки
//!
//! Внутри лежит объект «ключ настройки → значение» целиком. Хранить список
//! ключей и читать по нему текущие значения было бы дешевле, но тогда профиль,
//! сделанный полгода назад, менялся бы сам: появилась новая настройка облика —
//! и в старом профиле её нет, убрали старую — и она тихо осталась от прежнего
//! персонажа.
//!
//! # Что в профиль не входит
//!
//! Настройки окна — где оно стоит, поверх ли всех, пропускает ли клики — это
//! про рабочее место человека, а не про персонажа: перенося облик, никто не
//! ждёт, что окно уедет в другой угол. Язык и тема интерфейса, провайдер
//! распознавания и ключи тоже остаются на месте.

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Ключ настройки, помнящей применённый профиль.
const KEY_CURRENT: &str = "profile.current";

/// Имя ассистента: пустое значение означает «как в поставке».
pub const KEY_NAME: &str = "persona.name";

/// Настройки, из которых складывается облик.
///
/// Список явный, а не «всё, что начинается с persona и avatar»: под тот же
/// префикс попадают положение окна и признак «окно открыто», и перенос облика
/// таскал бы их за собой.
const PROFILE_KEYS: &[&str] = &[
    KEY_NAME,
    "persona.role",
    "persona.custom",
    "persona.formality",
    "persona.verbosity",
    "persona.address",
    // Собранный текст поверх идентичности. Он выводится из предыдущих полей,
    // но хранится отдельно и читается агентным циклом на каждом запросе —
    // пересобирать его при применении профиля значило бы повторить здесь всю
    // логику сборки из persona.rs.
    "persona.extra",
    "avatar.model",
    "avatar.animations",
    "avatar.pose",
    // Слово пробуждения — обученная на голосе человека модель, и она привязана
    // к имени: переименовав Yuki в Джарвиса, откликаться на «Юки» она не должна.
    "voice.wake.model",
    "voice.language",
];

/// Профиль в списке.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub name: String,
    /// Применён ли он прямо сейчас.
    pub active: bool,
    /// Выбрана ли в нём модель — по этому видно, полон ли облик.
    pub has_model: bool,
    pub updated_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot(std::collections::BTreeMap<String, String>);

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

fn set_setting(state: &AppState, key: &str, value: &str) -> Result<(), String> {
    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = unixepoch()",
                rusqlite::params![key, value],
            )
        })
        .map(|_| ())
        .map_err(err)
}

/// Имя ассистента для интерфейса.
///
/// Пустая строка означает «как в поставке»: подставлять сюда «Yuki» в момент
/// установки нельзя, иначе будущее переименование приложения прошло бы мимо
/// всех, кто ничего не менял.
#[tauri::command]
pub fn persona_name(state: State<'_, AppState>) -> String {
    setting(&state, KEY_NAME).unwrap_or_default()
}

/// Переименовывает ассистента.
#[tauri::command]
pub fn persona_set_name(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let trimmed = name.trim();

    // Длина ограничена: имя стоит в строке заголовка и на главном экране, и
    // строка на двести знаков сломала бы и то, и другое.
    if trimmed.chars().count() > 32 {
        return Err("имя длиннее тридцати двух знаков".into());
    }

    set_setting(&state, KEY_NAME, trimmed)
}

/// Список профилей, новые сверху.
#[tauri::command]
pub fn profile_list(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    let current = setting(&state, KEY_CURRENT).unwrap_or_default();

    state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, data, updated_at FROM profiles ORDER BY updated_at DESC",
            )?;

            let rows = stmt.query_map([], |row| {
                let id: String = row.get(0)?;
                let data: String = row.get(2)?;

                Ok(Profile {
                    active: id == current,
                    id,
                    name: row.get(1)?,
                    has_model: model_of(&data).is_some_and(|path| !path.is_empty()),
                    updated_at: row.get(3)?,
                })
            })?;

            rows.collect()
        })
        .map_err(err)
}

/// Достаёт путь к модели из снимка, не разбирая его целиком в структуру.
fn model_of(data: &str) -> Option<String> {
    serde_json::from_str::<Snapshot>(data)
        .ok()?
        .0
        .get("avatar.model")
        .cloned()
}

/// Сохраняет текущий облик как профиль.
///
/// Если профиль с таким именем уже есть, он перезаписывается: человек,
/// сохраняющий «Юки» второй раз, хочет обновить её, а не получить вторую
/// «Юки» в списке.
#[tauri::command]
pub fn profile_save(state: State<'_, AppState>, name: String) -> Result<Vec<Profile>, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("у профиля должно быть имя".into());
    }

    let mut snapshot = std::collections::BTreeMap::new();
    for key in PROFILE_KEYS {
        // Незаданная настройка сохраняется пустой строкой, а не пропускается:
        // «в этом профиле анимаций нет» — такое же решение, как «вот эти», и
        // применение профиля должно его воспроизводить.
        snapshot.insert((*key).to_string(), setting(&state, key).unwrap_or_default());
    }

    let data = serde_json::to_string(&Snapshot(snapshot)).map_err(err)?;

    let existing: Option<String> = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT id FROM profiles WHERE name = ?1",
                [trimmed],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(err)?;

    let id = existing.unwrap_or_else(|| format!("prof-{}", uuid()));

    state
        .storage
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO profiles (id, name, data) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET
                     name = excluded.name,
                     data = excluded.data,
                     updated_at = unixepoch()",
                rusqlite::params![id, trimmed, data],
            )
        })
        .map_err(err)?;

    set_setting(&state, KEY_CURRENT, &id)?;
    profile_list(state)
}

/// Применяет профиль: раскладывает снимок обратно по настройкам.
///
/// Ключи, которых в снимке нет, не трогаются. Это важно для профилей,
/// сделанных до появления новой настройки облика: сбрасывать её в пустоту
/// значило бы терять то, чего профиль никогда не обещал менять.
#[tauri::command]
pub fn profile_apply(state: State<'_, AppState>, id: String) -> Result<Vec<Profile>, String> {
    let data: String = state
        .storage
        .with_conn(|conn| {
            conn.query_row("SELECT data FROM profiles WHERE id = ?1", [&id], |r| r.get(0))
        })
        .map_err(|_| format!("профиля {id} нет"))?;

    let snapshot: Snapshot = serde_json::from_str(&data).map_err(err)?;

    for (key, value) in snapshot.0 {
        // Чужие ключи из снимка игнорируются: файл базы человек может
        // поправить руками, и применение профиля не должно быть способом
        // записать что угодно куда угодно.
        if PROFILE_KEYS.contains(&key.as_str()) {
            set_setting(&state, &key, &value)?;
        }
    }

    set_setting(&state, KEY_CURRENT, &id)?;
    profile_list(state)
}

#[tauri::command]
pub fn profile_delete(state: State<'_, AppState>, id: String) -> Result<Vec<Profile>, String> {
    state
        .storage
        .with_conn(|conn| conn.execute("DELETE FROM profiles WHERE id = ?1", [&id]))
        .map_err(err)?;

    // Удалённый профиль перестаёт быть текущим, но настройки остаются как
    // есть: удаление профиля — это забыть снимок, а не откатить облик.
    if setting(&state, KEY_CURRENT).as_deref() == Some(id.as_str()) {
        set_setting(&state, KEY_CURRENT, "")?;
    }

    profile_list(state)
}

/// Идентификатор без внешней зависимости: время плюс счётчик процесса.
fn uuid() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or_default();

    format!("{micros:x}{:x}", COUNTER.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Облик состоит из персонажа, а не из рабочего места.
    ///
    /// Ровно та ошибка, которую легко сделать «заодно»: положить в профиль всё,
    /// что начинается с `avatar.`, и вместе с моделью перенести положение окна.
    /// Человек, меняющий персонажа, не ждёт, что окно уедет в другой угол.
    #[test]
    fn the_profile_carries_the_character_and_not_the_window() {
        for key in ["persona.role", "avatar.model", "avatar.animations", "persona.name"] {
            assert!(PROFILE_KEYS.contains(&key), "в облике нет {key}");
        }

        for key in [
            "avatar.placement",
            "avatar.enabled",
            "avatar.click_through",
            "avatar.always_on_top",
            "ui.theme",
            "ui.language",
            "voice.stt.provider",
            "voice.stt.model",
        ] {
            assert!(!PROFILE_KEYS.contains(&key), "{key} не должен быть в облике");
        }
    }

    /// Слово пробуждения переносится вместе с именем.
    ///
    /// Иначе переименованный в Джарвиса ассистент продолжал бы откликаться на
    /// «Юки» — то есть имя было бы надписью, а не именем.
    #[test]
    fn renaming_carries_the_wake_word() {
        assert!(PROFILE_KEYS.contains(&"voice.wake.model"));
        assert!(PROFILE_KEYS.contains(&KEY_NAME));
    }

    /// Ключи в списке не повторяются.
    #[test]
    fn the_key_list_has_no_duplicates() {
        let unique: std::collections::BTreeSet<_> = PROFILE_KEYS.iter().collect();
        assert_eq!(unique.len(), PROFILE_KEYS.len(), "ключ в списке дважды");
    }

    /// Путь к модели читается из снимка без разбора остального.
    #[test]
    fn the_model_is_readable_from_a_snapshot() {
        let data = r#"{"avatar.model":"D:/models/arisa.vrm","persona.role":"assistant"}"#;
        assert_eq!(model_of(data).as_deref(), Some("D:/models/arisa.vrm"));

        assert_eq!(model_of(r#"{"persona.role":"assistant"}"#), None);
        assert_eq!(model_of("не json"), None);
    }

    /// Идентификаторы не повторяются даже в одну микросекунду.
    #[test]
    fn identifiers_do_not_collide() {
        let made: std::collections::BTreeSet<_> = (0..500).map(|_| uuid()).collect();
        assert_eq!(made.len(), 500);
    }
}
