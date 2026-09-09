//! OAuth 2.0 с PKCE и возвратом на loopback (RFC 8252, RFC 7636).

use std::collections::HashMap;

use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{CalendarError, CalendarProvider, CalendarResult};

/// Пара verifier/challenge для PKCE.
///
/// Смысл PKCE: код авторизации, перехваченный по дороге, бесполезен без
/// verifier, который никогда не покидал приложение. Для настольной программы
/// это замена секрета клиента, которого у неё в принципе не может быть.
#[derive(Debug, Clone)]
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn generate() -> Self {
        let verifier = random_token(64);
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        Self {
            verifier,
            challenge,
        }
    }
}

/// Случайная строка из символов, разрешённых RFC 7636 для verifier.
fn random_token(length: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Всё, что нужно для одного захода авторизации.
#[derive(Debug, Clone)]
pub struct AuthRequest {
    pub url: String,
    pub pkce: Pkce,
    /// Значение state: ответ с чужим state — это чужой ответ, и принимать его нельзя.
    pub state: String,
    pub redirect_uri: String,
}

/// Токены доступа.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    /// Приходит только при первом обмене; при обновлении его может не быть,
    /// и тогда продолжает действовать прежний.
    pub refresh_token: Option<String>,
    /// Момент истечения в unix-секундах.
    pub expires_at: i64,
}

/// Собирает адрес страницы согласия.
pub fn authorize(
    provider: CalendarProvider,
    client_id: &str,
    redirect_uri: &str,
) -> AuthRequest {
    let pkce = Pkce::generate();
    let state = random_token(24);

    let mut params: Vec<(&str, &str)> = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", provider.scopes()),
        ("state", &state),
        ("code_challenge", &pkce.challenge),
        ("code_challenge_method", "S256"),
    ];

    if provider == CalendarProvider::Google {
        // Google выдаёт refresh-токен только при явном запросе оффлайн-доступа,
        // а повторно — только при `prompt=consent`. Без этих двух параметров
        // подключение живёт час и молча отваливается.
        params.push(("access_type", "offline"));
        params.push(("prompt", "consent"));
    }

    let query = params
        .iter()
        .map(|(key, value)| format!("{key}={}", encode(value)))
        .collect::<Vec<_>>()
        .join("&");

    AuthRequest {
        url: format!("{}?{query}", provider.authorize_endpoint()),
        pkce,
        state,
        redirect_uri: redirect_uri.to_string(),
    }
}

/// Кодирование значения параметра запроса.
///
/// Ручное, потому что зависимость ради одной функции здесь не окупается, а
/// набор незарезервированных символов задан RFC 3986 и не меняется.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Меняет код авторизации на токены.
pub async fn exchange(
    provider: CalendarProvider,
    http: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> CalendarResult<Tokens> {
    let mut form: HashMap<&str, &str> = HashMap::new();
    form.insert("client_id", client_id);
    form.insert("code", code);
    form.insert("code_verifier", verifier);
    form.insert("grant_type", "authorization_code");
    form.insert("redirect_uri", redirect_uri);
    if let Some(secret) = client_secret.filter(|s| !s.is_empty()) {
        form.insert("client_secret", secret);
    }

    post_tokens(provider, http, form).await
}

/// Продлевает доступ по refresh-токену.
pub async fn refresh(
    provider: CalendarProvider,
    http: &reqwest::Client,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> CalendarResult<Tokens> {
    let mut form: HashMap<&str, &str> = HashMap::new();
    form.insert("client_id", client_id);
    form.insert("refresh_token", refresh_token);
    form.insert("grant_type", "refresh_token");
    if let Some(secret) = client_secret.filter(|s| !s.is_empty()) {
        form.insert("client_secret", secret);
    }
    if provider == CalendarProvider::Microsoft {
        // Graph требует повторять список прав при обновлении.
        form.insert("scope", provider.scopes());
    }

    post_tokens(provider, http, form).await
}

async fn post_tokens(
    provider: CalendarProvider,
    http: &reqwest::Client,
    form: HashMap<&str, &str>,
) -> CalendarResult<Tokens> {
    let response = http
        .post(provider.token_endpoint())
        .form(&form)
        .send()
        .await
        .map_err(|e| CalendarError::Network(e.to_string()))?;

    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| CalendarError::Network(e.to_string()))?;

    if !status.is_success() {
        return Err(CalendarError::Auth(
            crate::parse::error_message(&text).unwrap_or(text),
        ));
    }

    parse_tokens(&text, now())
}

/// Разбирает ответ сервера токенов.
pub fn parse_tokens(body: &str, now: i64) -> CalendarResult<Tokens> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| CalendarError::Shape {
        provider: "сервер токенов",
        reason: e.to_string(),
    })?;

    let access_token = value["access_token"]
        .as_str()
        .ok_or(CalendarError::Shape {
            provider: "сервер токенов",
            reason: "в ответе нет access_token".into(),
        })?
        .to_string();

    // Срок жизни хранится как момент истечения, а не как длительность: пересчёт
    // «сколько осталось» после перезапуска приложения иначе невозможен.
    let expires_in = value["expires_in"].as_i64().unwrap_or(3600);

    Ok(Tokens {
        access_token,
        refresh_token: value["refresh_token"].as_str().map(str::to_string),
        expires_at: now + expires_in,
    })
}

/// Пора ли обновлять токен.
///
/// С запасом в минуту: токен, истекающий через секунду, формально ещё жив, но
/// запрос с ним уже уйдёт с просроченным доступом.
pub fn needs_refresh(expires_at: i64, now: i64) -> bool {
    expires_at - now < 60
}

pub fn now() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_the_sha256_of_the_verifier() {
        // Контрольный пример из RFC 7636, приложение B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let digest = Sha256::digest(verifier.as_bytes());
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn generated_verifiers_differ_and_fit_the_allowed_length() {
        let first = Pkce::generate();
        let second = Pkce::generate();
        assert_ne!(first.verifier, second.verifier, "verifier обязан быть случайным");
        assert!((43..=128).contains(&first.verifier.len()));
    }

    #[test]
    fn google_authorisation_asks_for_offline_access() {
        let request = authorize(
            CalendarProvider::Google,
            "client-123",
            "http://127.0.0.1:7788/callback",
        );
        assert!(request.url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        // Без этих двух параметров Google не отдаст refresh-токен, и подключение
        // умрёт через час — молча.
        assert!(request.url.contains("access_type=offline"));
        assert!(request.url.contains("prompt=consent"));
        assert!(request.url.contains("code_challenge_method=S256"));
        assert!(request.url.contains(&format!("state={}", request.state)));
    }

    #[test]
    fn microsoft_authorisation_requests_offline_scope() {
        let request = authorize(
            CalendarProvider::Microsoft,
            "client-123",
            "http://127.0.0.1:7788/callback",
        );
        assert!(request.url.contains("offline_access"));
        assert!(request.url.contains("Calendars.ReadWrite"));
    }

    #[test]
    fn redirect_uri_survives_encoding() {
        let request = authorize(
            CalendarProvider::Google,
            "id",
            "http://127.0.0.1:7788/callback",
        );
        assert!(request
            .url
            .contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A7788%2Fcallback"));
    }

    #[test]
    fn turns_the_lifetime_into_a_moment_of_expiry() {
        let tokens = parse_tokens(
            r#"{"access_token":"at","refresh_token":"rt","expires_in":3599}"#,
            1_000_000,
        )
        .expect("ответ должен разобраться");
        assert_eq!(tokens.expires_at, 1_003_599);
        assert_eq!(tokens.refresh_token.as_deref(), Some("rt"));
    }

    #[test]
    fn a_refresh_response_without_a_new_refresh_token_is_valid() {
        // Google при обновлении refresh-токен не присылает — прежний остаётся в силе.
        let tokens = parse_tokens(r#"{"access_token":"at2","expires_in":3600}"#, 0)
            .expect("ответ должен разобраться");
        assert!(tokens.refresh_token.is_none());
    }

    #[test]
    fn a_response_without_an_access_token_is_a_failure_not_an_empty_token() {
        assert!(parse_tokens(r#"{"error":"invalid_grant"}"#, 0).is_err());
    }

    #[test]
    fn refreshes_slightly_before_the_deadline() {
        assert!(!needs_refresh(1000, 900));
        assert!(needs_refresh(1000, 950));
        assert!(needs_refresh(1000, 1001));
    }
}
