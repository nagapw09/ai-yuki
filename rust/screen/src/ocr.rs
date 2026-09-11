//! Распознавание текста на экране (ТЗ §6: advanced computer vision).
//!
//! # Почему не tesseract
//!
//! Раньше OCR был отложен как слишком дорогой: tesseract тянет в дистрибутив
//! библиотеку и файлы языковых моделей, и это десятки мегабайт на каждую ОС.
//! Решение было верным для tesseract и неверным вообще: у обеих целевых систем
//! движок распознавания уже встроен. На Windows это `Windows.Media.Ocr`,
//! появившийся в Windows 10; на macOS — Vision. Ничего скачивать не нужно,
//! языки берутся из тех, что установлены в системе.
//!
//! # Когда это нужно, а когда нет
//!
//! Дерево интерфейса (ТЗ §6) остаётся приоритетным: оно точное, структурное и
//! содержит готовые действия. OCR нужен там, где текста в дереве нет вовсе —
//! картинки, PDF в просмотрщике, игры, удалённый рабочий стол, приложения на
//! своём движке рисования.
//!
//! Отдавать модели картинку в таких случаях тоже можно, но распознанный текст
//! дешевле на порядок: снимок экрана — это сотни килобайт в каждом запросе,
//! строки текста — единицы.

use yuki_system::{SystemError, SystemResult, TextLine};

/// Распознаёт текст на изображении в формате PNG.
///
/// Возвращает строки с их прямоугольниками в координатах **изображения**.
/// Координаты именно изображения, а не экрана: снимок мог быть кадрирован и
/// уменьшен, и пересчёт в экранные — забота вызывающего, который знает, что
/// именно он снимал.
#[cfg(windows)]
pub fn recognize(png: &[u8]) -> SystemResult<Vec<TextLine>> {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};

    fn platform(e: impl std::fmt::Display) -> SystemError {
        SystemError::Platform(e.to_string())
    }

    // Движок берёт язык из профиля пользователя: у человека с русской системой
    // распознается русский, и навязывать английский было бы хуже.
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(platform)?;

    let stream = InMemoryRandomAccessStream::new().map_err(platform)?;
    let writer = DataWriter::CreateDataWriter(&stream.GetOutputStreamAt(0).map_err(platform)?)
        .map_err(platform)?;
    writer.WriteBytes(png).map_err(platform)?;
    writer.StoreAsync().map_err(platform)?.get().map_err(platform)?;
    writer.FlushAsync().map_err(platform)?.get().map_err(platform)?;
    stream.Seek(0).map_err(platform)?;

    let decoder = BitmapDecoder::CreateAsync(&stream)
        .map_err(platform)?
        .get()
        .map_err(platform)?;
    let bitmap = decoder
        .GetSoftwareBitmapAsync()
        .map_err(platform)?
        .get()
        .map_err(platform)?;

    let result = engine
        .RecognizeAsync(&bitmap)
        .map_err(platform)?
        .get()
        .map_err(platform)?;

    let mut out = Vec::new();

    for line in result.Lines().map_err(platform)?.into_iter() {
        let text = line.Text().map_err(platform)?.to_string();
        if text.trim().is_empty() {
            continue;
        }

        // Прямоугольник строки движок не даёт — он даёт слова. Объединяем их
        // рамки: строка целиком полезнее, чем двадцать слов по отдельности,
        // когда надо понять, где на экране эта надпись.
        let mut bounds: Option<yuki_system::Rect> = None;
        for word in line.Words().map_err(platform)?.into_iter() {
            let rect = word.BoundingRect().map_err(platform)?;
            let word_rect = yuki_system::Rect {
                x: rect.X as i32,
                y: rect.Y as i32,
                width: rect.Width as i32,
                height: rect.Height as i32,
            };
            bounds = Some(match bounds {
                None => word_rect,
                Some(current) => merge(current, word_rect),
            });
        }

        out.push(TextLine {
            text,
            rect: bounds.unwrap_or_default(),
        });
    }

    Ok(out)
}

#[cfg(not(windows))]
pub fn recognize(_png: &[u8]) -> SystemResult<Vec<TextLine>> {
    // На macOS распознавание есть в Vision, но реализация ждёт живой машины:
    // код, который нельзя запустить, нельзя и объявить работающим.
    Err(SystemError::NotImplemented(
        "распознавание текста на этой платформе",
    ))
}

/// Объединяет два прямоугольника в минимальный, вмещающий оба.
///
/// Отдельной функцией ради теста: слова в строке идут не строго слева направо
/// (арабский, смешанный текст), и наивное «взять первый и последний» дало бы
/// рамку не по строке.
pub fn merge(a: yuki_system::Rect, b: yuki_system::Rect) -> yuki_system::Rect {
    let left = a.x.min(b.x);
    let top = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);

    yuki_system::Rect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use yuki_system::Rect;

    fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn merges_two_side_by_side_words_into_one_line() {
        let merged = merge(rect(10, 20, 30, 12), rect(45, 20, 25, 12));
        assert_eq!(merged, rect(10, 20, 60, 12));
    }

    #[test]
    fn order_of_words_does_not_matter() {
        // Слова приходят не строго слева направо: наивное «первое и последнее»
        // дало бы рамку не по строке.
        let forward = merge(rect(10, 20, 30, 12), rect(45, 20, 25, 12));
        let backward = merge(rect(45, 20, 25, 12), rect(10, 20, 30, 12));
        assert_eq!(forward, backward);
    }

    #[test]
    fn keeps_the_tallest_word_in_the_height() {
        // Буквы с выносными элементами выше остальных, и обрезать их нельзя.
        let merged = merge(rect(0, 10, 20, 10), rect(25, 8, 20, 16));
        assert_eq!(merged, rect(0, 8, 45, 16));
    }

    #[test]
    fn a_word_inside_another_changes_nothing() {
        let outer = rect(0, 0, 100, 40);
        assert_eq!(merge(outer, rect(10, 10, 20, 20)), outer);
    }
}
