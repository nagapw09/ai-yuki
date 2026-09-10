//! Диагностика: журнал в памяти и отчёт для разбора (`docs/GAPS.md` §14).
//!
//! # Что это и чего это не делает
//!
//! Кнопка «собрать отчёт» складывает в один файл всё, что нужно, чтобы понять
//! чужую поломку: версии, систему, статусы разрешений, здоровье возможностей и
//! последние строки журнала.
//!
//! **Никуда ничего не отправляется.** Автоматическая отправка отчётов о
//! падениях — это фоновая выгрузка данных с чужой машины, и включать её без
//! спроса нельзя; спрашивать пока нечего, потому что принимающей стороны у
//! проекта нет. Файл появляется на диске, и что с ним делать, решает человек.
//!
//! # Почему журнал в памяти, а не в файле
//!
//! Разбирают всегда текущий запуск: «только что не сработало». Кольцевой буфер
//! на несколько сотен строк отвечает ровно на этот вопрос, не растёт без
//! присмотра и не оставляет на диске файл, о котором никто не помнит.

use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use tauri::State;

use crate::state::AppState;

/// Сколько строк журнала держать.
///
/// Пятисот хватает на весь запуск обычной сессии; больше — это уже не «что
/// произошло только что», а архив, которому место в файле.
const LOG_CAPACITY: usize = 500;

/// Кольцевой буфер строк журнала.
#[derive(Clone, Default)]
pub struct LogBuffer {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl LogBuffer {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(VecDeque::with_capacity(LOG_CAPACITY))),
        }
    }

    fn push(&self, line: String) {
        let Ok(mut lines) = self.lines.lock() else {
            return;
        };
        if lines.len() == LOG_CAPACITY {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    /// Последние строки, старые сверху.
    pub fn snapshot(&self) -> Vec<String> {
        self.lines
            .lock()
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Писатель, отдающий строки в буфер.
///
/// `tracing` пишет форматированную строку через `io::Write`; одна запись может
/// прийти несколькими вызовами, поэтому строки собираются по переводу строки.
pub struct BufferWriter {
    buffer: LogBuffer,
    pending: Vec<u8>,
}

impl io::Write for BufferWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(data);

        while let Some(index) = self.pending.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=index).collect();
            let text = String::from_utf8_lossy(&line).trim_end().to_string();
            if !text.is_empty() {
                self.buffer.push(text);
            }
        }

        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for BufferWriter {
    fn drop(&mut self) {
        // Хвост без перевода строки — тоже сообщение, и терять его нельзя.
        if !self.pending.is_empty() {
            let text = String::from_utf8_lossy(&self.pending).trim_end().to_string();
            if !text.is_empty() {
                self.buffer.push(text);
            }
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBuffer {
    type Writer = BufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        BufferWriter {
            buffer: self.clone(),
            pending: Vec::new(),
        }
    }
}

// ── Отчёт ───────────────────────────────────────────────────────────────────────

/// Настройки, значения которых входят в отчёт.
///
/// Всё остальное показывается только именем ключа. Секретов в настройках нет по
/// устройству (они в хранилище ОС), но там есть личное — город, имя, своя
/// формулировка роли, — а отчёт человек отдаёт постороннему.
const SAFE_SETTINGS: &[&str] = &[
    "ui.theme",
    "ui.language",
    "hotkey.summon",
    "tray.close_to_tray",
    "window.always_on_top",
    "avatar.enabled",
    "privacy.local_only",
    "persona.role",
    "persona.formality",
    "persona.verbosity",
    "onboarding.completed",
    "voice.stt.model",
];

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Собирает отчёт в виде Markdown.
///
/// Markdown, а не JSON: отчёт читает человек, а не программа, и читаемость тут
/// важнее машинной разборности.
#[tauri::command]
pub fn diagnostics_report(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let mut out = String::new();
    let info = state.adapters.system.system_info().map_err(err)?;

    out.push_str("# Диагностика Yuki\n\n");
    out.push_str("Секретов здесь нет: ключи и токены лежат в хранилище ОС и в отчёт\n");
    out.push_str("не попадают. Личные настройки скрыты, показаны только их имена.\n\n");

    out.push_str("## Версии\n\n");
    out.push_str(&format!("- Yuki: {}\n", app.package_info().version));
    out.push_str(&format!("- Система: {} {}\n", info.platform, info.os_version));
    out.push_str(&format!("- Архитектура: {}\n", info.arch));
    out.push_str(&format!("- Ядер: {}\n", info.cpu_count));
    out.push_str(&format!(
        "- Память: {:.1} ГБ всего, {:.1} ГБ свободно\n\n",
        info.total_memory_bytes as f64 / 1e9,
        info.available_memory_bytes as f64 / 1e9
    ));

    let requirements = crate::requirements::report(&info);
    out.push_str("## Требования\n\n");
    for item in &requirements.items {
        out.push_str(&format!(
            "- {} {}: {} (нужно {})\n",
            if item.ok { "✓" } else { "✗" },
            item.label,
            item.actual,
            item.required
        ));
    }
    out.push('\n');

    out.push_str("## Разрешения\n\n");
    let permissions: Vec<(String, i64, i64)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT category, granted, os_granted FROM permissions ORDER BY category")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect()
        })
        .map_err(err)?;

    for (category, granted, os_granted) in permissions {
        out.push_str(&format!(
            "- {category}: пользователь {}, система {}\n",
            if granted != 0 { "да" } else { "нет" },
            if os_granted != 0 { "да" } else { "нет" }
        ));
    }
    out.push('\n');

    out.push_str("## Провайдеры\n\n");
    let providers: Vec<(String, String, String, i64, i64, Option<String>)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, kind, base_url, enabled, is_default, secret_ref FROM providers ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    for (id, kind, base_url, enabled, is_default, secret_ref) in providers {
        // Адрес входит в отчёт, ключ — нет. По адресу видно, куда уходят
        // запросы; по ключу видно только то, чего никто не должен видеть.
        out.push_str(&format!(
            "- {id} ({kind}): {base_url} · включён {} · основной {} · ключ {}\n",
            yes_no(enabled),
            yes_no(is_default),
            if secret_ref.is_some() { "задан" } else { "нет" }
        ));
    }
    out.push('\n');

    out.push_str("## Возможности\n\n");
    let capabilities: Vec<(String, String, String, Option<String>, i64)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, source, health, health_note, enabled FROM capabilities ORDER BY id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })?;
            rows.collect()
        })
        .map_err(err)?;

    if capabilities.is_empty() {
        out.push_str("_нет_\n\n");
    } else {
        for (id, source, health, note, enabled) in capabilities {
            out.push_str(&format!(
                "- {id} ({source}): {health} · включена {}{}\n",
                yes_no(enabled),
                note.map(|n| format!(" · {n}")).unwrap_or_default()
            ));
        }
        out.push('\n');
    }

    out.push_str("## Настройки\n\n");
    let settings: Vec<(String, String)> = state
        .storage
        .with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT key, value FROM settings ORDER BY key")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
        .map_err(err)?;

    for (key, value) in settings {
        out.push_str(&format!("- {key}: {}\n", redact(&key, &value)));
    }
    out.push('\n');

    out.push_str("## Журнал\n\n");
    let lines = state.logs.snapshot();
    if lines.is_empty() {
        out.push_str("_пусто. Для подробного журнала запустите с YUKI_LOG=debug._\n");
    } else {
        out.push_str("```\n");
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("```\n");
    }

    Ok(out)
}

/// Значение настройки для отчёта.
///
/// Отдельной функцией ради теста: цена ошибки здесь — личные данные в файле,
/// который человек отправит постороннему.
pub fn redact(key: &str, value: &str) -> String {
    if SAFE_SETTINGS.contains(&key) {
        return value.to_string();
    }
    // Длину показываем: по ней видно, задано ли значение вообще, и этого
    // достаточно, чтобы отличить «не настроено» от «настроено неверно».
    format!("(скрыто, {} симв.)", value.chars().count())
}

fn yes_no(flag: i64) -> &'static str {
    if flag != 0 {
        "да"
    } else {
        "нет"
    }
}

/// Сохраняет отчёт в файл.
#[tauri::command]
pub fn diagnostics_save(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let report = diagnostics_report(app, state)?;
    std::fs::write(&path, &report).map_err(|e| format!("{path}: {e}"))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn keeps_the_last_lines_and_drops_the_oldest() {
        let buffer = LogBuffer::new();
        for index in 0..(LOG_CAPACITY + 10) {
            buffer.push(format!("строка {index}"));
        }

        let lines = buffer.snapshot();
        assert_eq!(lines.len(), LOG_CAPACITY);
        // Разбирают всегда последнее, поэтому теряется начало, а не конец.
        assert_eq!(lines.last().expect("последняя"), "строка 509");
        assert_eq!(lines.first().expect("первая"), "строка 10");
    }

    #[test]
    fn assembles_lines_from_partial_writes() {
        // tracing пишет одно сообщение несколькими вызовами write.
        let buffer = LogBuffer::new();
        let mut writer = BufferWriter {
            buffer: buffer.clone(),
            pending: Vec::new(),
        };

        writer.write_all("нача".as_bytes()).expect("запись");
        writer.write_all("ло строки\nвторая\n".as_bytes()).expect("запись");

        let lines = buffer.snapshot();
        assert_eq!(lines, vec!["начало строки", "вторая"]);
    }

    #[test]
    fn a_line_without_a_newline_is_not_lost() {
        let buffer = LogBuffer::new();
        {
            let mut writer = BufferWriter {
                buffer: buffer.clone(),
                pending: Vec::new(),
            };
            writer.write_all("хвост без перевода".as_bytes()).expect("запись");
        }

        assert_eq!(buffer.snapshot(), vec!["хвост без перевода"]);
    }

    #[test]
    fn shows_technical_settings_and_hides_personal_ones() {
        assert_eq!(redact("ui.theme", "light"), "light");
        assert_eq!(redact("persona.role", "coach"), "coach");

        // Город и имя человек не обязан показывать тому, кто разбирает поломку.
        assert!(redact("everyday.city", "Москва").starts_with("(скрыто"));
        assert!(redact("persona.address", "Алексей").starts_with("(скрыто"));
    }

    #[test]
    fn a_hidden_value_still_says_whether_it_is_set() {
        // «Не настроено» и «настроено неверно» — разные беды, и различать их
        // надо, не показывая само значение.
        assert_eq!(redact("persona.custom", ""), "(скрыто, 0 симв.)");
        assert_eq!(redact("persona.custom", "текст"), "(скрыто, 5 симв.)");
    }

    #[test]
    fn no_secret_reference_can_leak_a_value() {
        // Ссылка на секрет — это имя записи в хранилище ОС, а не сам секрет;
        // но и она не должна попадать в отчёт как значение настройки.
        assert!(redact("mcp.github.secret_env", "GITHUB_TOKEN").starts_with("(скрыто"));
    }
}
