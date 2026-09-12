//! Локальное хранилище на SQLite (ТЗ §31) и миграции.
//!
//! Local-first по ТЗ §29: файл базы лежит в app data dir пользователя и никуда не
//! синхронизируется. Секретов в базе нет — см. [`crate::secrets`].

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

/// Версия схемы. Инкрементируется вместе с добавлением шага в [`MIGRATIONS`].
const SCHEMA_VERSION: i64 = 7;

/// Шаги миграции. Индекс в массиве + 1 = версия, до которой шаг поднимает базу.
const MIGRATIONS: &[&str] = &[
    include_str!("schema.sql"),
    include_str!("002_providers.sql"),
    include_str!("003_permission_defaults.sql"),
    include_str!("004_commands.sql"),
    include_str!("005_calendar.sql"),
    include_str!("006_profiles.sql"),
    include_str!("007_remote.sql"),
];

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("ошибка базы данных: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("не удалось создать каталог данных: {0}")]
    Io(#[from] std::io::Error),

    #[error("состояние базы повреждено конкурентным доступом")]
    Poisoned,

    #[error("база новее приложения: версия схемы {found}, поддерживается {supported}")]
    TooNew { found: i64, supported: i64 },
}

pub type StorageResult<T> = Result<T, StorageError>;

/// Владелец соединения с базой.
///
/// SQLite в режиме WAL допускает конкурентное чтение, но `rusqlite::Connection`
/// не `Sync`, поэтому доступ сериализуется мьютексом. Для десктопного приложения
/// с единственным пользователем этого достаточно, и это дешевле пула соединений.
pub struct Storage {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Storage {
    /// Открывает базу по пути, создавая каталог и применяя миграции.
    pub fn open(path: impl AsRef<Path>) -> StorageResult<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        // Компромисс между сохранностью и числом fsync: при падении процесса данные
        // целы, теряется только незакоммиченный хвост при отключении питания.
        conn.pragma_update(None, "synchronous", "NORMAL")?;

        let storage = Self {
            conn: Mutex::new(conn),
            path,
        };
        storage.migrate()?;
        Ok(storage)
    }

    /// База в памяти — для тестов.
    pub fn in_memory() -> StorageResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let storage = Self {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        };
        storage.migrate()?;
        Ok(storage)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Выполняет операцию с соединением.
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> StorageResult<T> {
        let guard = self.conn.lock().map_err(|_| StorageError::Poisoned)?;
        f(&guard).map_err(Into::into)
    }

    /// Применяет недостающие миграции по `PRAGMA user_version`.
    fn migrate(&self) -> StorageResult<()> {
        let guard = self.conn.lock().map_err(|_| StorageError::Poisoned)?;

        let current: i64 =
            guard.query_row("PRAGMA user_version", [], |row| row.get(0))?;

        if current > SCHEMA_VERSION {
            // Откатывать схему вслепую нельзя: пользователь мог поставить более
            // новую версию Yuki, и её данные мы бы испортили.
            return Err(StorageError::TooNew {
                found: current,
                supported: SCHEMA_VERSION,
            });
        }

        for (index, step) in MIGRATIONS.iter().enumerate() {
            let target = index as i64 + 1;
            if target <= current {
                continue;
            }
            guard.execute_batch(step)?;
            guard.pragma_update(None, "user_version", target)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_fresh_database_to_current_version() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let version: i64 = storage
            .with_conn(|c| c.query_row("PRAGMA user_version", [], |r| r.get(0)))
            .expect("версия должна читаться");
        assert_eq!(version, SCHEMA_VERSION);
    }

    #[test]
    fn creates_every_table_required_by_spec_section_31() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let required = [
            "users",
            "settings",
            "providers",
            "conversations",
            "messages",
            "memories",
            "tools",
            "permissions",
            "capabilities",
            "plugins",
            "mcp_servers",
            "commands",
            "command_nodes",
            "automations",
            "automation_runs",
            "activity_logs",
            "notifications",
            "tasks",
        ];

        for table in required {
            let found: i64 = storage
                .with_conn(|c| {
                    c.query_row(
                        "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                        [table],
                        |r| r.get(0),
                    )
                })
                .expect("запрос должен выполниться");
            assert_eq!(found, 1, "таблица {table} из ТЗ §31 отсутствует");
        }
    }

    #[test]
    fn seeds_permission_categories_from_spec_section_21() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let count: i64 = storage
            .with_conn(|c| c.query_row("SELECT count(*) FROM permissions", [], |r| r.get(0)))
            .expect("запрос должен выполниться");
        assert_eq!(count, 10);
    }

    #[test]
    fn seeds_provider_presets_from_spec_section_4() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let count: i64 = storage
            .with_conn(|c| c.query_row("SELECT count(*) FROM providers", [], |r| r.get(0)))
            .expect("запрос должен выполниться");
        assert_eq!(count, 7);

        // Заготовки обязаны быть выключены: включение — осознанное действие
        // пользователя после ввода ключа.
        let enabled: i64 = storage
            .with_conn(|c| {
                c.query_row("SELECT count(*) FROM providers WHERE enabled = 1", [], |r| r.get(0))
            })
            .expect("запрос должен выполниться");
        assert_eq!(enabled, 0);
    }

    #[test]
    fn grants_only_the_non_invasive_permission_categories_by_default() {
        let storage = Storage::in_memory().expect("база должна открыться");
        let granted: Vec<String> = storage
            .with_conn(|c| {
                let mut stmt =
                    c.prepare("SELECT category FROM permissions WHERE granted = 1 ORDER BY category")?;
                let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
                rows.collect()
            })
            .expect("запрос должен выполниться");

        assert_eq!(granted, ["browser", "files", "network", "notifications"]);

        // Категории, дающие качественно новый доступ, обязаны остаться выключенными.
        for invasive in ["shell", "accessibility", "screen_recording", "microphone", "camera"] {
            assert!(
                !granted.iter().any(|c| c == invasive),
                "категория {invasive} не должна быть выдана без участия пользователя"
            );
        }
    }

    #[test]
    fn migration_is_idempotent() {
        let storage = Storage::in_memory().expect("база должна открыться");
        storage.migrate().expect("повторная миграция не должна падать");
    }
}
