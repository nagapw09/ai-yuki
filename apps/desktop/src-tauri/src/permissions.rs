//! Синхронизация системных разрешений (ТЗ §21).
//!
//! Категория из ТЗ §21 имеет две независимые стороны: согласие пользователя внутри
//! Yuki и разрешение на уровне ОС. Вторая сторона устроена на двух платформах
//! принципиально по-разному, и смешивать их нельзя — иначе на macOS получится
//! приложение, которое считает себя вправе, а система молча возвращает пустоту.

use yuki_system::Platform;

use crate::storage::{Storage, StorageResult};

/// Категории, которые Windows не выдаёт отдельно: доступ есть у любого процесса
/// пользователя, а отказ, если он возможен, приходит уже на вызове.
#[cfg(windows)]
const OS_GRANTED: &[&str] = &[
    "files",
    "network",
    "shell",
    "notifications",
    "browser",
    "external_services",
    // Windows не спрашивает разрешения на симуляцию ввода и снимок экрана.
    "accessibility",
    "screen_recording",
    // Микрофон и камера имеют системный переключатель приватности, но он не
    // виден приложению заранее: отказ приходит при попытке захвата.
    "microphone",
    "camera",
];

/// На macOS часть категорий выдаётся системой явно, через TCC, и до выдачи
/// соответствующий API возвращает пустой результат без ошибки. Такие категории
/// нельзя считать выданными по умолчанию — иначе Yuki будет уверять, что
/// прочитала интерфейс, которого она не видела.
#[cfg(target_os = "macos")]
const OS_GRANTED: &[&str] = &[
    "files",
    "network",
    "shell",
    "notifications",
    "browser",
    "external_services",
];

/// Категории, статус которых на macOS спрашивается у системы в момент проверки.
///
/// Их нельзя внести в [`OS_GRANTED`]: там список постоянный, а эти две меняются
/// в System Settings без ведома приложения — и меняются именно тогда, когда
/// пользователь идёт их выдавать.
#[cfg(target_os = "macos")]
fn probe_live(category: &str) -> Option<bool> {
    match category {
        "accessibility" => Some(yuki_macos::tcc::accessibility_granted()),
        "screen_recording" => Some(yuki_macos::tcc::screen_recording_granted()),
        _ => None,
    }
}

#[cfg(not(target_os = "macos"))]
fn probe_live(_category: &str) -> Option<bool> {
    None
}

/// Проставляет `os_granted` по правилам текущей платформы.
///
/// Вызывается при каждом старте и из мастера первого запуска: пользователь мог
/// выдать или отозвать разрешение в системных настройках между запусками — а на
/// macOS ещё и прямо сейчас, не закрывая мастер.
///
/// На macOS Accessibility и Screen Recording спрашиваются у системы, а не
/// предполагаются: до выдачи соответствующий API возвращает не ошибку, а пустоту,
/// и приложение, не спросившее статус, уверенно рапортует ерунду.
pub fn sync_os(storage: &Storage) -> StorageResult<()> {
    storage.with_conn(|conn| {
        conn.execute("UPDATE permissions SET os_granted = 0", [])?;

        let mut stmt = conn.prepare(
            "UPDATE permissions SET os_granted = 1, updated_at = unixepoch() WHERE category = ?1",
        )?;
        for category in OS_GRANTED {
            stmt.execute([category])?;
        }

        for category in LIVE_CATEGORIES {
            if probe_live(category) == Some(true) {
                stmt.execute([category])?;
            }
        }
        Ok(())
    })
}

/// Категории, которые опрашиваются вживую там, где это возможно.
pub const LIVE_CATEGORIES: &[&str] = &["accessibility", "screen_recording"];

/// Что показать пользователю, чтобы он выдал разрешение вручную (ТЗ §21).
///
/// На macOS это единственный путь: программно выдать TCC-разрешение нельзя,
/// можно только открыть нужную панель System Settings.
pub fn how_to_grant(category: &str) -> Option<&'static str> {
    match Platform::current()? {
        Platform::MacOS => Some(match category {
            "accessibility" => "System Settings → Privacy & Security → Accessibility",
            "screen_recording" => "System Settings → Privacy & Security → Screen Recording",
            "microphone" => "System Settings → Privacy & Security → Microphone",
            "camera" => "System Settings → Privacy & Security → Camera",
            "files" => "System Settings → Privacy & Security → Files and Folders",
            _ => return None,
        }),
        Platform::Windows => Some(match category {
            "microphone" => "Параметры → Конфиденциальность и защита → Микрофон",
            "camera" => "Параметры → Конфиденциальность и защита → Камера",
            _ => return None,
        }),
    }
}

/// Адрес панели системных настроек для категории.
///
/// Список закрытый и константный: это единственная причина, по которой открывать
/// схемы вроде `ms-settings:` вообще допустимо. Принимать сюда произвольную
/// строку значило бы дать способ запустить что угодно через зарегистрированный
/// в системе протокол.
pub fn settings_url(category: &str) -> Option<&'static str> {
    match Platform::current()? {
        Platform::MacOS => Some(match category {
            "accessibility" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            "screen_recording" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            "microphone" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
            }
            "camera" => "x-apple.systempreferences:com.apple.preference.security?Privacy_Camera",
            "files" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
            }
            _ => return None,
        }),
        Platform::Windows => Some(match category {
            "microphone" => "ms-settings:privacy-microphone",
            "camera" => "ms-settings:privacy-webcam",
            _ => return None,
        }),
    }
}

/// Показывает системный диалог выдачи, если для категории он существует.
///
/// Существует он ровно один: Screen Recording на macOS. Accessibility диалога
/// не имеет вовсе — только панель настроек, — а на Windows обе категории
/// спрашиваются самой ОС в момент захвата.
pub fn request_from_os(category: &str) -> bool {
    #[cfg(target_os = "macos")]
    if category == "screen_recording" {
        return yuki_macos::tcc::request_screen_recording();
    }

    let _ = category;
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_only_platform_appropriate_categories_as_os_granted() {
        let storage = Storage::in_memory().expect("база должна открыться");
        sync_os(&storage).expect("синхронизация должна пройти");

        let granted: i64 = storage
            .with_conn(|c| {
                c.query_row(
                    "SELECT count(*) FROM permissions WHERE os_granted = 1",
                    [],
                    |r| r.get(0),
                )
            })
            .expect("запрос должен выполниться");

        assert_eq!(granted as usize, OS_GRANTED.len());
    }

    #[test]
    fn every_live_category_knows_where_it_is_granted() {
        // Опрашиваемая вживую категория без ссылки на панель настроек — это
        // мастер, который говорит «выдайте разрешение» и не может показать где.
        for category in LIVE_CATEGORIES {
            if Platform::current() == Some(Platform::MacOS) {
                assert!(
                    settings_url(category).is_some(),
                    "для «{category}» нет панели настроек"
                );
            }
        }
    }

    #[test]
    fn settings_urls_are_limited_to_known_panes() {
        // Произвольная категория не должна превращаться в открытие чего угодно.
        assert!(settings_url("shell").is_none());
        assert!(settings_url("../../evil").is_none());
    }

    #[test]
    fn revoking_in_the_os_is_picked_up_on_the_next_sync() {
        let storage = Storage::in_memory().expect("база должна открыться");

        // Имитируем состояние, оставшееся от прошлого запуска.
        storage
            .with_conn(|c| c.execute("UPDATE permissions SET os_granted = 1", []))
            .expect("подготовка должна пройти");

        sync_os(&storage).expect("синхронизация должна пройти");

        let stale: i64 = storage
            .with_conn(|c| {
                c.query_row(
                    "SELECT count(*) FROM permissions WHERE os_granted = 1",
                    [],
                    |r| r.get(0),
                )
            })
            .expect("запрос должен выполниться");

        assert_eq!(stale as usize, OS_GRANTED.len());
    }
}
