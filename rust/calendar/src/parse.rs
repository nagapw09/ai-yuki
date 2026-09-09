//! Перевод ответов Google Calendar и Microsoft Graph в общий вид.

use serde_json::{json, Value};

use crate::{CalendarError, CalendarEvent, CalendarProvider, CalendarResult, EventDraft};

/// Список событий из ответа.
pub fn events(provider: CalendarProvider, body: &Value) -> CalendarResult<Vec<CalendarEvent>> {
    let items = body["items"]
        .as_array()
        .or_else(|| body["value"].as_array())
        .ok_or(CalendarError::Shape {
            provider: provider.label(),
            reason: "в ответе нет списка событий".into(),
        })?;

    // Отдельное событие, которое не удалось разобрать, не должно обрушивать
    // весь ответ: одно испорченное вхождение — не повод скрыть остальной день.
    Ok(items
        .iter()
        .filter_map(|item| event(provider, item).ok())
        .collect())
}

/// Одно событие.
pub fn event(provider: CalendarProvider, item: &Value) -> CalendarResult<CalendarEvent> {
    match provider {
        CalendarProvider::Google => google_event(item),
        CalendarProvider::Microsoft => microsoft_event(item),
    }
}

fn google_event(item: &Value) -> CalendarResult<CalendarEvent> {
    let id = required_str(item, "id", "Google Calendar")?;

    // У события на весь день вместо dateTime приходит date — и это не то же
    // самое время: «10 сентября» и «10 сентября 00:00 по UTC» — разные вещи.
    let (start, start_all_day) = google_moment(&item["start"]);
    let (end, _) = google_moment(&item["end"]);

    Ok(CalendarEvent {
        id,
        title: item["summary"]
            .as_str()
            .unwrap_or("(без названия)")
            .to_string(),
        start,
        end,
        all_day: start_all_day,
        location: optional_str(item, "location"),
        description: optional_str(item, "description"),
        link: optional_str(item, "htmlLink"),
    })
}

fn google_moment(value: &Value) -> (String, bool) {
    if let Some(moment) = value["dateTime"].as_str() {
        return (moment.to_string(), false);
    }
    (value["date"].as_str().unwrap_or_default().to_string(), true)
}

fn microsoft_event(item: &Value) -> CalendarResult<CalendarEvent> {
    let id = required_str(item, "id", "Outlook")?;
    let all_day = item["isAllDay"].as_bool().unwrap_or(false);

    Ok(CalendarEvent {
        id,
        title: item["subject"].as_str().unwrap_or("(без названия)").to_string(),
        start: microsoft_moment(&item["start"]),
        end: microsoft_moment(&item["end"]),
        all_day,
        location: item["location"]["displayName"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        description: item["bodyPreview"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        link: optional_str(item, "webLink"),
    })
}

/// Graph отдаёт время без обозначения зоны, вынося её в соседнее поле.
///
/// Строку приходится собирать вручную: `2026-09-10T10:00:00.0000000` без зоны
/// прочитается как местное время у одного читателя и как UTC у другого.
fn microsoft_moment(value: &Value) -> String {
    let moment = value["dateTime"].as_str().unwrap_or_default();
    let zone = value["timeZone"].as_str().unwrap_or("UTC");

    if moment.is_empty() {
        return String::new();
    }
    // Graph по умолчанию отвечает в UTC — тогда обозначение зоны однозначно.
    if zone.eq_ignore_ascii_case("UTC") {
        let trimmed = moment.trim_end_matches('Z');
        return format!("{trimmed}Z");
    }
    format!("{moment} {zone}")
}

/// Тело запроса на создание события.
pub fn draft_body(provider: CalendarProvider, draft: &EventDraft) -> Value {
    match provider {
        CalendarProvider::Google => {
            let mut body = json!({
                "summary": draft.title,
                "start": { "dateTime": draft.start },
                "end": { "dateTime": draft.end },
            });
            if let Some(location) = &draft.location {
                body["location"] = json!(location);
            }
            if let Some(description) = &draft.description {
                body["description"] = json!(description);
            }
            body
        }
        CalendarProvider::Microsoft => {
            let mut body = json!({
                "subject": draft.title,
                // Graph требует зону отдельным полем и игнорирует смещение
                // внутри строки, поэтому время отправляется как UTC.
                "start": { "dateTime": draft.start, "timeZone": "UTC" },
                "end": { "dateTime": draft.end, "timeZone": "UTC" },
            });
            if let Some(location) = &draft.location {
                body["location"] = json!({ "displayName": location });
            }
            if let Some(description) = &draft.description {
                body["body"] = json!({ "contentType": "text", "content": description });
            }
            body
        }
    }
}

/// Достаёт человеческую причину из тела ошибки провайдера.
pub fn error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;

    for path in [
        &value["error"]["message"],
        &value["error_description"],
        &value["message"],
    ] {
        if let Some(text) = path.as_str() {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    // У Google форма ошибки бывает и такой: {"error": "invalid_grant"}.
    value["error"].as_str().map(str::to_string)
}

fn required_str(item: &Value, field: &str, provider: &'static str) -> CalendarResult<String> {
    item[field]
        .as_str()
        .map(str::to_string)
        .ok_or(CalendarError::Shape {
            provider,
            reason: format!("у события нет поля {field}"),
        })
}

fn optional_str(item: &Value, field: &str) -> Option<String> {
    item[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_google_event() {
        let body = json!({
            "items": [{
                "id": "abc",
                "summary": "Созвон с Иваном",
                "start": { "dateTime": "2026-09-10T10:00:00+03:00" },
                "end": { "dateTime": "2026-09-10T10:30:00+03:00" },
                "location": "Zoom",
                "htmlLink": "https://calendar.google.com/event?eid=abc"
            }]
        });

        let parsed = events(CalendarProvider::Google, &body).expect("должен разобраться");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Созвон с Иваном");
        assert_eq!(parsed[0].start, "2026-09-10T10:00:00+03:00");
        assert!(!parsed[0].all_day);
        assert_eq!(parsed[0].location.as_deref(), Some("Zoom"));
    }

    #[test]
    fn tells_an_all_day_event_from_a_timed_one() {
        let body = json!({
            "items": [{
                "id": "holiday",
                "summary": "Отпуск",
                "start": { "date": "2026-09-10" },
                "end": { "date": "2026-09-17" }
            }]
        });

        let parsed = events(CalendarProvider::Google, &body).expect("должен разобраться");
        assert!(parsed[0].all_day, "событие на весь день не должно стать полуночью");
        assert_eq!(parsed[0].start, "2026-09-10");
    }

    #[test]
    fn reads_an_outlook_event_and_marks_its_time_zone() {
        let body = json!({
            "value": [{
                "id": "AAMk",
                "subject": "Планёрка",
                "isAllDay": false,
                "start": { "dateTime": "2026-09-10T07:00:00.0000000", "timeZone": "UTC" },
                "end": { "dateTime": "2026-09-10T07:30:00.0000000", "timeZone": "UTC" },
                "location": { "displayName": "Teams" },
                "webLink": "https://outlook.office.com/calendar/item/AAMk"
            }]
        });

        let parsed = events(CalendarProvider::Microsoft, &body).expect("должен разобраться");
        assert_eq!(parsed[0].title, "Планёрка");
        // Время без обозначения зоны читается по-разному — обозначение обязано быть.
        assert_eq!(parsed[0].start, "2026-09-10T07:00:00.0000000Z");
        assert_eq!(parsed[0].location.as_deref(), Some("Teams"));
    }

    #[test]
    fn a_single_broken_entry_does_not_hide_the_rest_of_the_day() {
        let body = json!({
            "items": [
                { "summary": "без идентификатора" },
                { "id": "ok", "summary": "Обед",
                  "start": { "dateTime": "2026-09-10T13:00:00Z" },
                  "end": { "dateTime": "2026-09-10T14:00:00Z" } }
            ]
        });

        let parsed = events(CalendarProvider::Google, &body).expect("должен разобраться");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].title, "Обед");
    }

    #[test]
    fn a_response_that_is_not_a_list_is_an_error() {
        assert!(events(CalendarProvider::Google, &json!({ "error": "boom" })).is_err());
    }

    #[test]
    fn builds_bodies_in_the_shape_each_service_expects() {
        let draft = EventDraft {
            title: "Звонок".into(),
            start: "2026-09-10T10:00:00Z".into(),
            end: "2026-09-10T10:30:00Z".into(),
            location: Some("Zoom".into()),
            description: None,
        };

        let google = draft_body(CalendarProvider::Google, &draft);
        assert_eq!(google["summary"], "Звонок");
        assert_eq!(google["start"]["dateTime"], "2026-09-10T10:00:00Z");
        assert!(google.get("description").is_none());

        let microsoft = draft_body(CalendarProvider::Microsoft, &draft);
        assert_eq!(microsoft["subject"], "Звонок");
        assert_eq!(microsoft["start"]["timeZone"], "UTC");
        assert_eq!(microsoft["location"]["displayName"], "Zoom");
    }

    #[test]
    fn digs_the_reason_out_of_every_error_shape_these_services_use() {
        assert_eq!(
            error_message(r#"{"error":{"message":"Insufficient Permission"}}"#).as_deref(),
            Some("Insufficient Permission")
        );
        assert_eq!(
            error_message(r#"{"error":"invalid_grant","error_description":"Token expired"}"#)
                .as_deref(),
            Some("Token expired")
        );
        assert_eq!(
            error_message(r#"{"error":"invalid_grant"}"#).as_deref(),
            Some("invalid_grant")
        );
        assert!(error_message("не json").is_none());
    }
}
