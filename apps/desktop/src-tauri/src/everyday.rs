//! Погода и курсы валют (`docs/GAPS.md` §6).
//!
//! # Почему это вообще в ассистенте
//!
//! В ТЗ их нет, но именно такие вещи спрашивают у ассистента каждый день. У
//! Astra они вынесены в отдельные everyday-инструменты, и без них «помощник на
//! каждый день» отвечает на всё, кроме того, что спрашивают чаще всего.
//!
//! # Почему без ключей
//!
//! Open-Meteo и Frankfurter работают без регистрации. Это не экономия: сервис,
//! требующий ключ, превратил бы «сколько градусов» в ещё один мастер настройки,
//! а вшитый общий ключ мгновенно перестал бы быть ключом приложения.
//!
//! # Отношение к Local Only
//!
//! Погода и курсы — сетевые по своей природе, локального ответа у них не
//! бывает. Режим Local Only (ТЗ §29) их не запрещает: он обещает, что **разговор
//! и голос** не уходят с машины, а не что приложение не ходит в сеть вовсе.
//! Названия города в запросе достаточно, чтобы это оставалось честным.

use serde::Serialize;
use tauri::State;

use crate::state::AppState;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Погода сейчас и на ближайшие дни.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Weather {
    pub place: String,
    pub temperature: f64,
    /// Как ощущается — по ней человек и решает, что надеть.
    pub feels_like: f64,
    pub description: String,
    pub wind_speed: f64,
    pub forecast: Vec<DayForecast>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayForecast {
    /// Дата в виде ГГГГ-ММ-ДД.
    pub date: String,
    pub min: f64,
    pub max: f64,
    pub description: String,
}

/// Курсы валют относительно одной базовой.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub base: String,
    /// Дата курса: банковские курсы обновляются раз в сутки и в выходные стоят.
    pub date: String,
    pub rates: Vec<Rate>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Rate {
    pub code: String,
    pub value: f64,
}

/// Расшифровка кода погоды WMO.
///
/// Таблица нужна целиком: Open-Meteo возвращает только число, и «код 73» в
/// ответе ассистента — это не ответ. Значения — из спецификации WMO 4677,
/// сгруппированные по силе явления.
pub fn describe_weather(code: i64) -> &'static str {
    match code {
        0 => "ясно",
        1 => "почти ясно",
        2 => "переменная облачность",
        3 => "пасмурно",
        45 | 48 => "туман",
        51 | 53 | 55 => "морось",
        56 | 57 => "ледяная морось",
        61 => "небольшой дождь",
        63 => "дождь",
        65 => "сильный дождь",
        66 | 67 => "ледяной дождь",
        71 => "небольшой снег",
        73 => "снег",
        75 => "сильный снег",
        77 => "снежная крупа",
        80 | 81 => "ливень",
        82 => "сильный ливень",
        85 | 86 => "снегопад",
        95 => "гроза",
        96 | 99 => "гроза с градом",
        // Неизвестный код честнее назвать неизвестным, чем подставить «ясно».
        _ => "погода неопределённая",
    }
}

/// Находит координаты города.
///
/// Отдельным шагом, потому что человек говорит «в Питере», а не «59.94, 30.31».
async fn geocode(http: &reqwest::Client, city: &str) -> Result<(f64, f64, String), String> {
    let response = http
        .get("https://geocoding-api.open-meteo.com/v1/search")
        .query(&[("name", city), ("count", "1"), ("language", "ru")])
        .send()
        .await
        .map_err(|e| format!("не удалось найти город: {e}"))?;

    let body: serde_json::Value = response.json().await.map_err(err)?;

    let first = body["results"]
        .as_array()
        .and_then(|list| list.first())
        .ok_or_else(|| format!("город «{city}» не найден"))?;

    let latitude = first["latitude"].as_f64().ok_or("в ответе нет широты")?;
    let longitude = first["longitude"].as_f64().ok_or("в ответе нет долготы")?;

    // Собираем человеческое название: «Москва, Россия» понятнее, чем «Москва»,
    // когда одноимённых городов несколько.
    let name = first["name"].as_str().unwrap_or(city);
    let country = first["country"].as_str().unwrap_or_default();
    let place = if country.is_empty() {
        name.to_string()
    } else {
        format!("{name}, {country}")
    };

    Ok((latitude, longitude, place))
}

#[tauri::command]
pub async fn weather_get(state: State<'_, AppState>, city: String) -> Result<Weather, String> {
    let city = city.trim();
    if city.is_empty() {
        return Err("не указан город".into());
    }

    let (latitude, longitude, place) = geocode(&state.http, city).await?;

    let response = state
        .http
        .get("https://api.open-meteo.com/v1/forecast")
        .query(&[
            ("latitude", latitude.to_string()),
            ("longitude", longitude.to_string()),
            (
                "current",
                "temperature_2m,apparent_temperature,weather_code,wind_speed_10m".into(),
            ),
            (
                "daily",
                "temperature_2m_max,temperature_2m_min,weather_code".into(),
            ),
            ("forecast_days", "4".into()),
            // Часовой пояс места, а не UTC: «завтра» должно означать завтра там.
            ("timezone", "auto".into()),
        ])
        .send()
        .await
        .map_err(|e| format!("не удалось получить погоду: {e}"))?;

    let body: serde_json::Value = response.json().await.map_err(err)?;
    parse_weather(&body, place)
}

/// Разбор ответа Open-Meteo.
pub fn parse_weather(body: &serde_json::Value, place: String) -> Result<Weather, String> {
    let current = &body["current"];
    let temperature = current["temperature_2m"]
        .as_f64()
        .ok_or("в ответе нет температуры")?;

    let daily = &body["daily"];
    let dates = daily["time"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    let max = daily["temperature_2m_max"].as_array();
    let min = daily["temperature_2m_min"].as_array();
    let codes = daily["weather_code"].as_array();

    let forecast = dates
        .iter()
        .enumerate()
        .filter_map(|(index, date)| {
            Some(DayForecast {
                date: date.as_str()?.to_string(),
                min: min?.get(index)?.as_f64()?,
                max: max?.get(index)?.as_f64()?,
                description: describe_weather(codes?.get(index)?.as_i64()?).to_string(),
            })
        })
        .collect();

    Ok(Weather {
        place,
        temperature,
        // Ощущаемая температура может не прийти — тогда честнее показать
        // фактическую, чем ноль.
        feels_like: current["apparent_temperature"]
            .as_f64()
            .unwrap_or(temperature),
        description: describe_weather(current["weather_code"].as_i64().unwrap_or(-1)).to_string(),
        wind_speed: current["wind_speed_10m"].as_f64().unwrap_or(0.0),
        forecast,
    })
}

#[tauri::command]
pub async fn rates_get(
    state: State<'_, AppState>,
    base: Option<String>,
    symbols: Option<Vec<String>>,
) -> Result<Rates, String> {
    let base = base
        .map(|b| b.trim().to_uppercase())
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "USD".into());

    let symbols = symbols
        .filter(|list| !list.is_empty())
        .unwrap_or_else(|| ["RUB", "EUR", "USD"].iter().map(|s| s.to_string()).collect())
        .into_iter()
        .map(|s| s.trim().to_uppercase())
        // Базовая валюта в списке даёт бессмысленную строку «1 USD = 1 USD».
        .filter(|s| !s.is_empty() && *s != base)
        .collect::<Vec<_>>();

    if symbols.is_empty() {
        return Err("не задано, к каким валютам считать курс".into());
    }

    let response = state
        .http
        .get("https://api.frankfurter.app/latest")
        .query(&[("from", base.clone()), ("to", symbols.join(","))])
        .send()
        .await
        .map_err(|e| format!("не удалось получить курсы: {e}"))?;

    let body: serde_json::Value = response.json().await.map_err(err)?;
    parse_rates(&body, base)
}

/// Разбор ответа Frankfurter.
pub fn parse_rates(body: &serde_json::Value, base: String) -> Result<Rates, String> {
    let table = body["rates"]
        .as_object()
        .ok_or("в ответе нет курсов")?;

    let mut rates: Vec<Rate> = table
        .iter()
        .filter_map(|(code, value)| {
            Some(Rate {
                code: code.clone(),
                value: value.as_f64()?,
            })
        })
        .collect();

    // Порядок ответа сервиса произволен, а список в интерфейсе не должен
    // прыгать между запросами.
    rates.sort_by(|a, b| a.code.cmp(&b.code));

    Ok(Rates {
        base,
        date: body["date"].as_str().unwrap_or_default().to_string(),
        rates,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_documented_weather_code_has_words() {
        // Коды из WMO 4677, которые реально отдаёт Open-Meteo.
        for code in [0, 1, 2, 3, 45, 48, 51, 55, 61, 65, 71, 75, 80, 82, 95, 96] {
            let text = describe_weather(code);
            assert_ne!(text, "погода неопределённая", "код {code} остался без текста");
        }
    }

    #[test]
    fn an_unknown_code_says_so_instead_of_guessing() {
        // «Ясно» вместо неизвестного кода — это выдуманный ответ.
        assert_eq!(describe_weather(1234), "погода неопределённая");
        assert_eq!(describe_weather(-1), "погода неопределённая");
    }

    #[test]
    fn reads_current_weather_and_the_forecast() {
        let body = json!({
            "current": {
                "temperature_2m": -3.4,
                "apparent_temperature": -8.1,
                "weather_code": 73,
                "wind_speed_10m": 5.2
            },
            "daily": {
                "time": ["2026-09-10", "2026-09-11"],
                "temperature_2m_max": [1.0, 3.0],
                "temperature_2m_min": [-5.0, -2.0],
                "weather_code": [71, 3]
            }
        });

        let weather = parse_weather(&body, "Москва, Россия".into()).expect("должно разобраться");
        assert_eq!(weather.description, "снег");
        assert_eq!(weather.feels_like, -8.1);
        assert_eq!(weather.forecast.len(), 2);
        assert_eq!(weather.forecast[1].description, "пасмурно");
    }

    #[test]
    fn falls_back_to_the_real_temperature_when_the_felt_one_is_missing() {
        let body = json!({ "current": { "temperature_2m": 12.0 }, "daily": {} });
        let weather = parse_weather(&body, "где-то".into()).expect("должно разобраться");
        assert_eq!(weather.feels_like, 12.0);
        assert!(weather.forecast.is_empty());
    }

    #[test]
    fn a_response_without_a_temperature_is_an_error_not_a_zero() {
        assert!(parse_weather(&json!({ "current": {} }), "город".into()).is_err());
    }

    #[test]
    fn a_partial_forecast_row_is_skipped_rather_than_faked() {
        // У последнего дня нет минимума — строку с выдуманным нулём показывать
        // нельзя, а остальные дни терять незачем.
        let body = json!({
            "current": { "temperature_2m": 5.0, "weather_code": 0 },
            "daily": {
                "time": ["2026-09-10", "2026-09-11"],
                "temperature_2m_max": [7.0, 8.0],
                "temperature_2m_min": [2.0],
                "weather_code": [0, 1]
            }
        });

        let weather = parse_weather(&body, "город".into()).expect("должно разобраться");
        assert_eq!(weather.forecast.len(), 1);
    }

    #[test]
    fn sorts_rates_so_the_list_does_not_jump_between_requests() {
        let body = json!({
            "date": "2026-09-09",
            "rates": { "RUB": 92.5, "EUR": 0.91, "GBP": 0.78 }
        });

        let rates = parse_rates(&body, "USD".into()).expect("должно разобраться");
        let codes: Vec<&str> = rates.rates.iter().map(|r| r.code.as_str()).collect();
        assert_eq!(codes, vec!["EUR", "GBP", "RUB"]);
        assert_eq!(rates.date, "2026-09-09");
    }

    #[test]
    fn a_response_without_rates_is_an_error() {
        assert!(parse_rates(&json!({ "date": "2026-09-09" }), "USD".into()).is_err());
    }
}
