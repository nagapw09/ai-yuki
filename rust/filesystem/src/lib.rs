//! Файловые операции (ТЗ §8, §30 `FileAdapter`).
//!
//! Реализация общая для обеих ОС: платформенных различий здесь нет, кроме корзины
//! и открытия файла в приложении по умолчанию — их берут на себя `trash` и `opener`.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use walkdir::WalkDir;
use yuki_system::{FileAdapter, FileEntry, FileQuery, FileSort, SystemError, SystemResult};

pub struct CrossPlatformFileAdapter;

impl CrossPlatformFileAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CrossPlatformFileAdapter {
    fn default() -> Self {
        Self::new()
    }
}

fn entry_from_path(path: &Path) -> SystemResult<FileEntry> {
    let meta = fs::metadata(path)?;
    Ok(FileEntry {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        is_dir: meta.is_dir(),
        size_bytes: meta.len(),
        modified_at: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64),
        extension: path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase()),
    })
}

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| s.starts_with('.'))
        .unwrap_or(false)
}

impl FileAdapter for CrossPlatformFileAdapter {
    fn search(&self, query: &FileQuery) -> SystemResult<Vec<FileEntry>> {
        let root = Path::new(&query.root);
        if !root.exists() {
            return Err(SystemError::NotFound(query.root.clone()));
        }

        let needle = query.name_contains.as_ref().map(|s| s.to_lowercase());
        let wanted_ext: Vec<String> = query.extensions.iter().map(|e| e.to_lowercase()).collect();

        let mut walker = WalkDir::new(root).follow_links(false);
        if let Some(depth) = query.max_depth {
            walker = walker.max_depth(depth);
        }

        let mut found = Vec::new();
        for entry in walker.into_iter().filter_entry(|e| {
            // Скрытые каталоги отсекаем целиком, чтобы не проваливаться в .git и подобные.
            query.include_hidden || e.depth() == 0 || !is_hidden(e)
        }) {
            // Недоступный элемент — не повод валить весь поиск: пропускаем и идём дальше.
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_file() {
                continue;
            }

            let path = entry.path();

            if !wanted_ext.is_empty() {
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !wanted_ext.contains(&ext) {
                    continue;
                }
            }

            if let Some(needle) = &needle {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !name.contains(needle) {
                    continue;
                }
            }

            if let Ok(file) = entry_from_path(path) {
                found.push(file);
            }
        }

        match query.sort {
            FileSort::NameAsc => found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
            FileSort::ModifiedDesc => found.sort_by(|a, b| b.modified_at.cmp(&a.modified_at)),
            FileSort::SizeDesc => found.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes)),
        }
        found.truncate(query.limit);
        Ok(found)
    }

    fn read(&self, path: &str) -> SystemResult<Vec<u8>> {
        fs::read(path).map_err(Into::into)
    }

    fn write(&self, path: &str, contents: &[u8]) -> SystemResult<()> {
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents).map_err(Into::into)
    }

    fn move_to(&self, from: &str, to: &str) -> SystemResult<()> {
        if let Some(parent) = Path::new(to).parent() {
            fs::create_dir_all(parent)?;
        }
        // rename не работает между томами — в этом случае копируем и удаляем источник.
        match fs::rename(from, to) {
            Ok(()) => Ok(()),
            Err(_) => {
                fs::copy(from, to)?;
                fs::remove_file(from)?;
                Ok(())
            }
        }
    }

    fn copy(&self, from: &str, to: &str) -> SystemResult<()> {
        if let Some(parent) = Path::new(to).parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(from, to)?;
        Ok(())
    }

    fn delete(&self, path: &str, to_trash: bool) -> SystemResult<()> {
        let target = Path::new(path);
        if !target.exists() {
            return Err(SystemError::NotFound(path.to_string()));
        }
        if to_trash {
            // Корзина — поведение по умолчанию: удаление относится к HIGH risk (ТЗ §22),
            // и даже после подтверждения пользователя действие должно быть обратимым.
            return trash::delete(target).map_err(|e| SystemError::Platform(e.to_string()));
        }
        if target.is_dir() {
            fs::remove_dir_all(target)?;
        } else {
            fs::remove_file(target)?;
        }
        Ok(())
    }

    fn stat(&self, path: &str) -> SystemResult<FileEntry> {
        let p = Path::new(path);
        if !p.exists() {
            return Err(SystemError::NotFound(path.to_string()));
        }
        entry_from_path(p)
    }

    fn open(&self, path: &str) -> SystemResult<()> {
        if !Path::new(path).exists() {
            return Err(SystemError::NotFound(path.to_string()));
        }
        opener::open(path).map_err(|e| SystemError::Platform(e.to_string()))
    }
}
