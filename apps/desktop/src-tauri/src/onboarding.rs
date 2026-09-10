//! Мастер первого запуска (`docs/GAPS.md` §1).
//!
//! # Зачем он обязателен, а не приятен
//!
//! ТЗ §21 перечисляет категории разрешений, но не описывает, как они
//! **выдаются**. На macOS это не мелочь: Accessibility и Screen Recording
//! выдаются только вручную в System Settings, приложение после выдачи нужно
//! перезапустить, а до выдачи соответствующий API возвращает не ошибку, а
//! пустоту. Без пошагового мастера человек получает ассистента, который
//! уверенно рассказывает про пустой экран, — и не понимает, почему.
//!
//! # Что мастер делает и чего не делает
//!
//! Делает: спрашивает язык, доводит до рабочего провайдера, показывает статус
//! каждого разрешения живьём и открывает нужную панель настроек.
//!
//! Не делает: не выдаёт разрешения за пользователя (этого не умеет ни одна ОС),
//! не пропускает шаги молча и не отмечает пройденным то, что не проверено.

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

/// Ключ настройки: пройден ли мастер.
pub const SETTING_COMPLETED: &str = "onboarding.completed";

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Разрешение в том виде, в каком его показывает мастер.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStep {
    pub category: String,
    /// Согласие внутри Yuki.
    pub user_granted: bool,
    /// Разрешение на уровне ОС.
    pub os_granted: bool,
    /// Где выдать вручную; `null` — на этой ОС выдавать нечего.
    pub hint: Option<String>,
    /// Можно ли открыть панель системных настроек прямо отсюда.
    pub can_open_settings: bool,
    /// Показывает ли ОС диалог выдачи по запросу приложения.
    pub can_request: bool,
    /// Обязательна ли категория для базовой работы.
    pub required: bool,
}

/// Состояние мастера целиком.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingStatus {
    pub completed: bool,
    pub platform: String,
    /// Настроен ли хотя бы один провайдер и выбран ли он основным.
    pub provider_ready: bool,
    pub provider_label: Option<String>,
    pub permissions: Vec<PermissionStep>,
    pub requirements: crate::requirements::RequirementsReport,
}

/// Категории, без которых Yuki не выполняет свою основную работу.
///
/// Список короткий намеренно: мастер, требующий десять разрешений подряд,
/// приучает нажимать «разрешить» не читая. Микрофон, камера и терминал
/// спрашиваются позже — тогда, когда человек попросит то, для чего они нужны.
const REQUIRED: &[&str] = &["files", "accessibility", "screen_recording"];

/// Порядок шагов. Сначала то, без чего Yuki бесполезна.
const ORDER: &[&str] = &[
    "files",
    "accessibility",
    "screen_recording",
    "microphone",
    "notifications",
    "browser",
];

// ── Команды ─────────────────────────────────────────────────────────────────────

/// Текущее состояние мастера.
///
/// Каждый вызов пересчитывает статус разрешений у системы: человек уходит
/// выдавать их в System Settings и возвращается в это же окно, и «обновить»
/// должно означать настоящий опрос, а не перечитывание вчерашней записи.
#[tauri::command]
pub fn onboarding_status(state: State<'_, AppState>) -> Result<OnboardingStatus, String> {
    crate::permissions::sync_os(&state.storage).map_err(err)?;

    let rows: Vec<(String, bool, bool)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT category, granted, os_granted FROM permissions")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? != 0,
                    r.get::<_, i64>(2)? != 0,
                ))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    let permissions = ORDER
        .iter()
        .filter_map(|category| {
            let (_, user_granted, os_granted) =
                rows.iter().find(|(name, _, _)| name == category)?;

            Some(PermissionStep {
                category: (*category).to_string(),
                user_granted: *user_granted,
                os_granted: *os_granted,
                hint: crate::permissions::how_to_grant(category).map(str::to_string),
                can_open_settings: crate::permissions::settings_url(category).is_some(),
                // Диалог выдачи существует ровно у одной категории и ровно на
                // одной ОС; на всех остальных кнопка «разрешить» была бы враньём.
                can_request: cfg!(target_os = "macos") && *category == "screen_recording",
                required: REQUIRED.contains(category),
            })
        })
        .collect();

    let provider: Option<String> = state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT label FROM providers WHERE is_default = 1 AND enabled = 1",
                [],
                |r| r.get::<_, String>(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })
        })
        .map_err(err)?;

    let info = state.adapters.system.system_info().map_err(err)?;

    Ok(OnboardingStatus {
        completed: completed(state.clone())?,
        platform: info.platform.clone(),
        provider_ready: provider.is_some(),
        provider_label: provider,
        permissions,
        requirements: crate::requirements::report(&info),
    })
}

/// Пройден ли мастер.
#[tauri::command]
pub fn onboarding_completed(state: State<'_, AppState>) -> Result<bool, String> {
    completed(state)
}

fn completed(state: State<'_, AppState>) -> Result<bool, String> {
    state
        .storage
        .with_conn(|conn| {
            conn.query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [SETTING_COMPLETED],
                |r| r.get::<_, String>(0),
            )
            .map(|v| v == "on")
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(false),
                other => Err(other),
            })
        })
        .map_err(err)
}

/// Отмечает мастер пройденным.
///
/// Пройти можно и с невыданными разрешениями: заставлять человека выдавать
/// Screen Recording, чтобы просто поговорить с моделью, — это шантаж, а не
/// забота. Невыданное останется видно в настройках, а инструмент, которому
/// разрешения не хватило, скажет об этом в момент вызова.
#[tauri::command]
pub fn onboarding_finish(state: State<'_, AppState>) -> Result<(), String> {
    set(&state, SETTING_COMPLETED, "on")
}

/// Возвращает мастер: пункт «пройти настройку заново» в настройках.
#[tauri::command]
pub fn onboarding_reset(state: State<'_, AppState>) -> Result<(), String> {
    set(&state, SETTING_COMPLETED, "off")
}

/// Открывает панель системных настроек для категории.
///
/// Адрес берётся из закрытого списка в [`crate::permissions::settings_url`], а не
/// из аргумента: `ms-settings:` и `x-apple.systempreferences:` — это способ
/// запустить зарегистрированный в системе обработчик, и принимать сюда
/// произвольную строку значило бы открыть дыру ради удобства.
#[tauri::command]
pub fn permission_open_settings(category: String) -> Result<(), String> {
    let url = crate::permissions::settings_url(&category).ok_or_else(|| {
        format!("для «{category}» на этой системе нет отдельной панели настроек")
    })?;

    opener::open(url).map_err(err)
}

/// Просит ОС показать диалог выдачи, если он существует.
///
/// Возвращает статус **после** запроса — но не результат ответа пользователя:
/// системный API отвечает сразу, а человек нажимает кнопку потом. Поэтому
/// интерфейс всё равно обязан перечитать статус, а не верить этому ответу.
#[tauri::command]
pub fn permission_request_os(
    state: State<'_, AppState>,
    category: String,
) -> Result<bool, String> {
    let granted = crate::permissions::request_from_os(&category);
    crate::permissions::sync_os(&state.storage).map_err(err)?;
    Ok(granted)
}

fn set(state: &AppState, key: &str, value: &str) -> Result<(), String> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_required_category_is_part_of_the_walkthrough() {
        // Обязательная категория, которой нет в шагах, — это требование,
        // которое мастер не даёт выполнить.
        for category in REQUIRED {
            assert!(ORDER.contains(category), "«{category}» нет в шагах мастера");
        }
    }

    #[test]
    fn the_walkthrough_asks_for_less_than_the_spec_defines() {
        // Мастер, перечисляющий все десять категорий из ТЗ §21, приучает
        // нажимать «разрешить» не читая.
        assert!(ORDER.len() < 10);
        assert!(!ORDER.contains(&"shell"), "терминал не спрашивают заранее");
    }

    #[test]
    fn every_step_is_a_real_permission_category() {
        let schema = include_str!("storage/schema.sql");
        for category in ORDER {
            assert!(
                schema.contains(&format!("('{category}')")),
                "категории «{category}» нет в схеме"
            );
        }
    }
}
