//! Календари: Google Calendar и Outlook (ТЗ §25).
//!
//! # Почему нет встроенных ключей приложения
//!
//! И Google, и Microsoft выдают доступ к календарю только зарегистрированному
//! приложению. Вшить его учётные данные в открытый репозиторий нельзя: они
//! немедленно перестают быть учётными данными этого приложения и становятся
//! общими, а квоту и отзыв доступа получает кто угодно. Поэтому client_id
//! заводит сам пользователь — один раз, в консоли соответствующего сервиса, —
//! и Yuki хранит его как обычную настройку, а refresh-токен как секрет ОС.
//!
//! # Почему loopback, а не встроенный браузер
//!
//! Провайдеры давно запрещают показывать свою форму входа внутри чужого
//! WebView: страница входа в таком окне неотличима от подделки, и пароль
//! вводится в приложение, а не в браузер. Единственный правильный путь для
//! настольной программы — системный браузер и возврат на `127.0.0.1` (RFC 8252),
//! с PKCE (RFC 7636) вместо секрета клиента.

pub mod oauth;
pub mod parse;

use serde::{Deserialize, Serialize};

pub use oauth::{AuthRequest, Pkce, Tokens};

#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("сеть: {0}")]
    Network(String),

    #[error("{provider} ответил {status}: {message}")]
    Api {
        provider: &'static str,
        status: u16,
        message: String,
    },

    #[error("доступ к календарю истёк и не был продлён: {0}")]
    Auth(String),

    #[error("неожиданный ответ {provider}: {reason}")]
    Shape {
        provider: &'static str,
        reason: String,
    },
}

pub type CalendarResult<T> = Result<T, CalendarError>;

/// Поддерживаемые календари (ТЗ §25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CalendarProvider {
    Google,
    /// Outlook / Microsoft 365 через Microsoft Graph.
    Microsoft,
}

impl CalendarProvider {
    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "google" => Self::Google,
            "microsoft" | "outlook" => Self::Microsoft,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Google => "google",
            Self::Microsoft => "microsoft",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Google => "Google Calendar",
            Self::Microsoft => "Outlook",
        }
    }

    /// Где пользователь заводит приложение и берёт client_id.
    ///
    /// Часть контракта, а не документации: без этой ссылки шаг «получите
    /// client_id» превращается в тупик, и подключение не состоится.
    pub fn console_url(self) -> &'static str {
        match self {
            Self::Google => "https://console.cloud.google.com/apis/credentials",
            Self::Microsoft => "https://entra.microsoft.com/#view/Microsoft_AAD_RegisteredApps",
        }
    }

    pub fn authorize_endpoint(self) -> &'static str {
        match self {
            Self::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    pub fn token_endpoint(self) -> &'static str {
        match self {
            Self::Google => "https://oauth2.googleapis.com/token",
            Self::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    /// Запрашиваемые права.
    ///
    /// Ровно чтение и запись событий — не весь аккаунт. Список календарей,
    /// почта и контакты сюда не входят: доступ, который не нужен для ТЗ §25,
    /// просить нельзя, даже если это упростило бы код.
    pub fn scopes(self) -> &'static str {
        match self {
            Self::Google => "https://www.googleapis.com/auth/calendar.events",
            // offline_access обязателен: без него не выдаётся refresh-токен,
            // и доступ отвалится через час, потребовав нового входа.
            Self::Microsoft => "offline_access Calendars.ReadWrite",
        }
    }
}

/// Событие в едином для обоих провайдеров виде.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    /// Начало в RFC 3339 либо `ГГГГ-ММ-ДД` для события на весь день.
    pub start: String,
    pub end: String,
    pub all_day: bool,
    pub location: Option<String>,
    pub description: Option<String>,
    /// Ссылка на событие в веб-интерфейсе календаря.
    pub link: Option<String>,
}

/// Что создать в календаре.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventDraft {
    pub title: String,
    /// RFC 3339 с зоной, например `2026-09-10T10:00:00+03:00`.
    pub start: String,
    pub end: String,
    pub location: Option<String>,
    pub description: Option<String>,
}

/// Подключённый календарь.
pub struct CalendarClient {
    provider: CalendarProvider,
    access_token: String,
    http: reqwest::Client,
}

impl CalendarClient {
    pub fn new(provider: CalendarProvider, access_token: String, http: reqwest::Client) -> Self {
        Self {
            provider,
            access_token,
            http,
        }
    }

    /// События в промежутке. Границы — RFC 3339.
    pub async fn events(&self, from: &str, to: &str) -> CalendarResult<Vec<CalendarEvent>> {
        let request = match self.provider {
            CalendarProvider::Google => self
                .http
                .get("https://www.googleapis.com/calendar/v3/calendars/primary/events")
                .query(&[
                    ("timeMin", from),
                    ("timeMax", to),
                    // Повторяющееся событие разворачивается в отдельные вхождения:
                    // «что у меня завтра» — это вхождения, а не правила повтора.
                    ("singleEvents", "true"),
                    ("orderBy", "startTime"),
                    ("maxResults", "100"),
                ]),
            CalendarProvider::Microsoft => self
                .http
                .get("https://graph.microsoft.com/v1.0/me/calendarView")
                .query(&[
                    ("startDateTime", from),
                    ("endDateTime", to),
                    ("$orderby", "start/dateTime"),
                    ("$top", "100"),
                ]),
        };

        let body = self.send(request).await?;
        parse::events(self.provider, &body)
    }

    /// Создаёт событие и возвращает его в том виде, в каком его принял сервис.
    ///
    /// Возвращается именно ответ сервиса, а не то, что отправили: календарь мог
    /// сдвинуть время по своей зоне, и рапортовать пользователю надо о том, что
    /// действительно записано.
    pub async fn create(&self, draft: &EventDraft) -> CalendarResult<CalendarEvent> {
        let payload = parse::draft_body(self.provider, draft);

        let request = match self.provider {
            CalendarProvider::Google => self
                .http
                .post("https://www.googleapis.com/calendar/v3/calendars/primary/events")
                .json(&payload),
            CalendarProvider::Microsoft => self
                .http
                .post("https://graph.microsoft.com/v1.0/me/events")
                .json(&payload),
        };

        let body = self.send(request).await?;
        parse::event(self.provider, &body)
    }

    /// Удаляет событие по идентификатору.
    pub async fn delete(&self, id: &str) -> CalendarResult<()> {
        let url = match self.provider {
            CalendarProvider::Google => format!(
                "https://www.googleapis.com/calendar/v3/calendars/primary/events/{id}"
            ),
            CalendarProvider::Microsoft => format!("https://graph.microsoft.com/v1.0/me/events/{id}"),
        };

        let response = self
            .http
            .delete(url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| CalendarError::Network(e.to_string()))?;

        let status = response.status();
        if status.is_success() || status.as_u16() == 404 {
            // 404 после удаления — это уже удалённое событие, а не ошибка:
            // повтор безопасной операции должен приводить к тому же состоянию.
            return Ok(());
        }

        Err(self.api_error(status.as_u16(), response.text().await.unwrap_or_default()))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> CalendarResult<serde_json::Value> {
        let response = request
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| CalendarError::Network(e.to_string()))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| CalendarError::Network(e.to_string()))?;

        if !status.is_success() {
            return Err(self.api_error(status.as_u16(), text));
        }

        serde_json::from_str(&text).map_err(|e| CalendarError::Shape {
            provider: self.provider.label(),
            reason: e.to_string(),
        })
    }

    fn api_error(&self, status: u16, body: String) -> CalendarError {
        // Текст ошибки провайдера почти всегда объясняет причину точнее, чем
        // код: «недостаточно прав» и «неверный формат времени» — это оба 400.
        let message = parse::error_message(&body).unwrap_or(body);

        if status == 401 {
            return CalendarError::Auth(message);
        }

        CalendarError::Api {
            provider: self.provider.label(),
            status,
            message,
        }
    }
}
