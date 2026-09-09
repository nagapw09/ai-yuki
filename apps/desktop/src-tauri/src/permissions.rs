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

/// Проставляет `os_granted` по правилам текущей платформы.
///
/// Вызывается при каждом старте: пользователь мог выдать или отозвать разрешение
/// в системных настройках между запусками.
///
/// # Ограничение
/// Живой опрос статуса TCC на macOS (`AXIsProcessTrusted`, `CGPreflight…`) — это
/// фаза 2 вместе с мастером первого запуска. Пока категории, требующие явной
/// выдачи, остаются невыданными, и инструмент честно упирается в отказ, а не
/// делает вид, что сработал.
pub fn sync_os(storage: &Storage) -> StorageResult<()> {
    storage.with_conn(|conn| {
        conn.execute("UPDATE permissions SET os_granted = 0", [])?;

        let mut stmt = conn.prepare(
            "UPDATE permissions SET os_granted = 1, updated_at = unixepoch() WHERE category = ?1",
        )?;
        for category in OS_GRANTED {
            stmt.execute([category])?;
        }
        Ok(())
    })
}

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
