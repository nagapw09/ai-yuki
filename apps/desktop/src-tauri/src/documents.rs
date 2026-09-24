//! Чтение документов: PDF, Word, Excel (ТЗ §8).
//!
//! Раньше Yuki читала только простой текст и на PDF отвечала ошибкой. Теперь
//! документ разбирается на части с понятными метками — «стр. 3», «лист
//! Продажи», «абзацы 41–80», — чтобы ответ можно было сослаться на место:
//! «на третьей странице написано…».
//!
//! Всё локально, без сети и без сторонних программ: разбор на чистом Rust.
//! Сканы без текстового слоя честно называются сканами — выдумывать их
//! содержимое нельзя.

use std::io::Read;
use std::path::Path;

use serde::Serialize;

/// Часть документа с меткой места.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Part {
    /// «стр. 3», «лист Продажи», «абзацы 1–40».
    pub label: String,
    pub text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPage {
    /// pdf, docx, xlsx, text.
    pub kind: String,
    /// Всего частей в документе.
    pub total: usize,
    /// С какой части начата выдача.
    pub from: usize,
    pub parts: Vec<Part>,
    /// Есть ли продолжение — тогда просить с `from = from + parts.len()`.
    pub more: bool,
    /// Пояснение, если текста нет (скан) или он обрезан.
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub label: String,
    /// Отрывок вокруг совпадения.
    pub snippet: String,
    pub score: usize,
}

/// Сколько текста отдавать за раз: столько модель читает без потери внимания.
const PAGE_CHARS: usize = 12_000;
/// Абзацев Word в одной части.
const PARAGRAPHS_PER_PART: usize = 40;
/// Строк таблицы в одной части.
const ROWS_PER_PART: usize = 200;
/// Больше не открываем: документ на сотни мегабайт — не то, что читают голосом.
const MAX_BYTES: u64 = 200 * 1024 * 1024;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn kind_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Разбирает документ на части.
pub fn parts(path: &Path) -> Result<(String, Vec<Part>, Option<String>), String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    if size > MAX_BYTES {
        return Err("документ больше 200 МБ — открывать его целиком не стоит".into());
    }
    let ext = kind_of(path);
    match ext.as_str() {
        "pdf" => pdf(path),
        "docx" => docx(path).map(|p| ("docx".into(), p, None)),
        "xlsx" | "xlsm" | "xls" | "ods" => sheets(path).map(|p| ("xlsx".into(), p, None)),
        "doc" => Err("старый формат .doc не поддерживается — сохраните файл как .docx".into()),
        _ => text(path).map(|p| ("text".into(), p, None)),
    }
}

fn pdf(path: &Path) -> Result<(String, Vec<Part>, Option<String>), String> {
    let pages = pdf_extract::extract_text_by_pages(path).map_err(|e| format!("не удалось прочитать PDF: {e}"))?;
    let parts: Vec<Part> = pages
        .into_iter()
        .enumerate()
        .map(|(i, text)| Part { label: format!("стр. {}", i + 1), text: tidy(&text) })
        .collect();
    let empty = parts.iter().all(|p| p.text.trim().is_empty());
    let note = empty.then(|| {
        "В PDF нет текстового слоя — похоже, это скан. Прочитать его можно, открыв файл и \
         распознав экран (read_screen_text)."
            .to_string()
    });
    Ok(("pdf".into(), parts, note))
}

/// Word: текст абзацев из `word/document.xml`, по сорок абзацев в части.
fn docx(path: &Path) -> Result<Vec<Part>, String> {
    let file = std::fs::File::open(path).map_err(err)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("это не документ Word: {e}"))?;
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .map_err(|_| "в файле нет текста Word".to_string())?
        .read_to_string(&mut xml)
        .map_err(err)?;

    let paragraphs = docx_paragraphs(&xml)?;
    Ok(paragraphs
        .chunks(PARAGRAPHS_PER_PART)
        .enumerate()
        .map(|(i, chunk)| {
            let first = i * PARAGRAPHS_PER_PART + 1;
            Part {
                label: format!("абзацы {}–{}", first, first + chunk.len() - 1),
                text: chunk.join("\n"),
            }
        })
        .collect())
}

/// Абзацы Word: `<w:p>` — абзац, `<w:t>` — текст, `<w:tab/>` и `<w:br/>` — пробел
/// и перенос. Пустые абзацы пропускаются.
fn docx_paragraphs(xml: &str) -> Result<Vec<String>, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut paragraphs = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader.read_event().map_err(err)? {
            Event::Start(e) if e.name().into_inner() == "w:t" => in_text = true,
            Event::End(e) if e.name().into_inner() == "w:t" => in_text = false,
            Event::Empty(e) if e.name().into_inner() == "w:tab" => current.push(' '),
            Event::Empty(e) if e.name().into_inner() == "w:br" => current.push('\n'),
            Event::Text(t) if in_text => current.push_str(AsRef::<str>::as_ref(&t)),
            Event::GeneralRef(r) if in_text => {
                // &amp; и прочие ссылки внутри текста.
                current.push_str(match AsRef::<str>::as_ref(&r) {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                });
            }
            Event::End(e) if e.name().into_inner() == "w:p" => {
                let paragraph = current.trim().to_string();
                if !paragraph.is_empty() {
                    paragraphs.push(paragraph);
                }
                current.clear();
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(paragraphs)
}

/// Таблицы: каждый лист отдельно, по двести строк в части, ячейки через « | ».
fn sheets(path: &Path) -> Result<Vec<Part>, String> {
    use calamine::{open_workbook_auto, Data, Reader};
    let mut book = open_workbook_auto(path).map_err(|e| format!("не удалось открыть таблицу: {e}"))?;
    let mut parts = Vec::new();
    for name in book.sheet_names().to_vec() {
        let Ok(range) = book.worksheet_range(&name) else {
            continue;
        };
        let rows: Vec<String> = range
            .rows()
            .map(|row| {
                row.iter()
                    .map(|cell| match cell {
                        Data::Empty => String::new(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join(" | ")
                    .trim_end_matches([' ', '|'])
                    .to_string()
            })
            .filter(|row| !row.is_empty())
            .collect();
        for (i, chunk) in rows.chunks(ROWS_PER_PART).enumerate() {
            let label = if rows.len() > ROWS_PER_PART {
                format!("лист {name}, строки {}–{}", i * ROWS_PER_PART + 1, i * ROWS_PER_PART + chunk.len())
            } else {
                format!("лист {name}")
            };
            parts.push(Part { label, text: chunk.join("\n") });
        }
    }
    Ok(parts)
}

/// Обычный текст: частями по размеру, метка — номера строк.
fn text(path: &Path) -> Result<Vec<Part>, String> {
    let bytes = std::fs::read(path).map_err(err)?;
    let content = String::from_utf8(bytes).map_err(|_| "файл не текстовый и не документ, который я умею читать".to_string())?;
    let lines: Vec<&str> = content.lines().collect();
    let mut parts = Vec::new();
    let mut start = 0;
    while start < lines.len() {
        let mut end = start;
        let mut size = 0;
        while end < lines.len() && (size < PAGE_CHARS / 2 || end == start) {
            size += lines[end].len() + 1;
            end += 1;
        }
        parts.push(Part {
            label: format!("строки {}–{}", start + 1, end),
            text: lines[start..end].join("\n"),
        });
        start = end;
    }
    Ok(parts)
}

/// Убирает лишние пробелы и пустые строки, которые оставляет разбор PDF.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = false;
    for line in text.lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")) {
        if line.is_empty() {
            if !blank && !out.is_empty() {
                out.push('\n');
            }
            blank = true;
        } else {
            out.push_str(&line);
            out.push('\n');
            blank = false;
        }
    }
    out.trim().to_string()
}

/// Страница выдачи: части начиная с `from`, пока не наберётся [`PAGE_CHARS`].
pub fn page(path: &Path, from: usize) -> Result<DocumentPage, String> {
    let (kind, all, note) = parts(path)?;
    let total = all.len();
    let mut size = 0;
    let mut out = Vec::new();
    for part in all.into_iter().skip(from) {
        if !out.is_empty() && size + part.text.len() > PAGE_CHARS {
            break;
        }
        size += part.text.len();
        let mut part = part;
        if part.text.len() > PAGE_CHARS {
            let cut = part.text.char_indices().nth(PAGE_CHARS).map(|(i, _)| i).unwrap_or(part.text.len());
            part.text.truncate(cut);
            part.text.push_str(" …");
        }
        out.push(part);
    }
    let shown = out.len();
    Ok(DocumentPage { kind, total, from, parts: out, more: from + shown < total, note })
}

/// Ищет в документе: части, где встречается больше всего слов запроса.
///
/// Слова сравниваются по началу (первые пять букв): «договор» найдёт и
/// «договора», и «договору». Без модели и без сети — быстро и предсказуемо.
pub fn search(path: &Path, query: &str) -> Result<Vec<Hit>, String> {
    let (_, all, _) = parts(path)?;
    let stems: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(|w| w.chars().take(5).collect())
        .collect();
    if stems.is_empty() {
        return Err("запрос слишком короткий — нужно хотя бы одно слово из трёх букв".into());
    }

    let mut hits: Vec<Hit> = Vec::new();
    for part in &all {
        // Каждая строка части — отдельный кандидат: отрывок должен быть коротким.
        for line in part.text.lines() {
            let lower = line.to_lowercase();
            let score = stems.iter().filter(|stem| lower.contains(stem.as_str())).count();
            if score == 0 {
                continue;
            }
            let snippet: String = line.chars().take(300).collect();
            hits.push(Hit { label: part.label.clone(), snippet, score });
        }
    }
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    hits.truncate(12);
    Ok(hits)
}

#[tauri::command]
pub async fn document_read(path: String, from: Option<usize>) -> Result<DocumentPage, String> {
    tauri::async_runtime::spawn_blocking(move || page(Path::new(&path), from.unwrap_or(0)))
        .await
        .map_err(err)?
}

#[tauri::command]
pub async fn document_search(path: String, query: String) -> Result<Vec<Hit>, String> {
    tauri::async_runtime::spawn_blocking(move || search(Path::new(&path), &query))
        .await
        .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_paragraphs_keep_text_and_skip_empty_ones() {
        let xml = r#"<w:document><w:body>
            <w:p><w:r><w:t>Договор &amp; условия</w:t></w:r></w:p>
            <w:p></w:p>
            <w:p><w:r><w:t>Срок:</w:t></w:r><w:r><w:tab/><w:t xml:space="preserve">30 дней</w:t></w:r></w:p>
        </w:body></w:document>"#;
        assert_eq!(docx_paragraphs(xml).unwrap(), vec!["Договор & условия", "Срок: 30 дней"]);
    }

    #[test]
    fn search_finds_word_forms_and_names_the_place() {
        let dir = std::env::temp_dir().join("yuki-doc-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("notes.txt");
        std::fs::write(&file, "Вступление\nСрок действия договора — один год\nПодписи сторон").unwrap();
        let hits = search(&file, "когда кончается договор?").unwrap();
        assert_eq!(hits[0].snippet, "Срок действия договора — один год");
        assert!(hits[0].label.starts_with("строки"));
    }

    #[test]
    fn long_text_is_paged() {
        let dir = std::env::temp_dir().join("yuki-doc-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("long.txt");
        std::fs::write(&file, "строка текста\n".repeat(5000)).unwrap();
        let first = page(&file, 0).unwrap();
        assert!(first.more);
        let next = page(&file, first.parts.len()).unwrap();
        assert_eq!(next.from, first.parts.len());
    }

    #[test]
    fn old_doc_format_is_named_honestly() {
        let dir = std::env::temp_dir().join("yuki-doc-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("old.doc");
        std::fs::write(&file, b"\xd0\xcf\x11\xe0").unwrap();
        assert!(parts(&file).unwrap_err().contains(".docx"));
    }
}

#[cfg(test)]
mod real_files {
    //! Проверка на настоящих файлах из `tmp/docs` (генерируются `tmp/make_docs.py`).
    //! Пропускается, если файлов нет: в чистом клоне их нет.
    use super::*;

    fn docs() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tmp/docs")
    }

    #[test]
    fn reads_real_pdf_docx_xlsx() {
        let dir = docs();
        if !dir.exists() {
            return;
        }
        let pdf = page(&dir.join("договор.pdf"), 0).unwrap();
        println!("PDF: {:?}", pdf.parts.iter().map(|p| (&p.label, &p.text)).collect::<Vec<_>>());
        assert_eq!(pdf.total, 2);
        assert!(pdf.parts[1].text.contains("11 месяцев"));
        let hits = search(&dir.join("договор.pdf"), "сколько стоит залог").unwrap();
        println!("поиск: {:?}", hits);
        assert_eq!(hits[0].label, "стр. 2");

        let docx = page(&dir.join("план.docx"), 0).unwrap();
        println!("DOCX: {:?}", docx.parts);
        assert!(docx.parts[0].text.contains("созвон с командой"));

        let xlsx = page(&dir.join("отчёт.xlsx"), 0).unwrap();
        println!("XLSX: {:?}", xlsx.parts);
        assert_eq!(xlsx.parts[0].label, "лист Продажи");
        assert!(xlsx.parts[0].text.contains("Август | 154500 | 41"));
        assert_eq!(xlsx.parts[1].label, "лист Расходы");
    }
}
