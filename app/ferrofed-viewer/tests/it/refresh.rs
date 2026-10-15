// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refresh of the operator's access token against a test OpenID
//! Provider: a session outlives its access token by refreshing it (RFC 6749
//! §6), a rotated refresh token is the one sent next, a refused refresh ends
//! the session, the absolute timeout ends it whatever the refresh token, and
//! the refresh token reaches no answer, cookie or log line (RFC 6749 §10.4).
//! The sign-in asks for `offline_access` only when the configuration says so
//! (OpenID Connect Core 1.0 §11).

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ferrofed_testkit::mock::Server;
use ferrofed_viewer::config::Config;
use ferrofed_viewer::config::error;
use ferrofed_viewer::server::{ViewerState, router};
use ferrofed_viewer::session::{SessionId, SignedIn};
use http::StatusCode;
use secrecy::{ExposeSecret as _, SecretString};
use url::Url;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::{OPERATOR_ID_TOKEN, OPERATOR_TOKEN, get, get_as, header, send, settings};

/// The refresh token every test session starts with.
const FIRST: &str = "synthetic-refresh-first";

/// The configuration that signs in at the provider `uri`, whose stub also
/// stands in for the gateway.
fn configuration(uri: &str, oidc: &str) -> String {
    format!(
        "[gateway]\nbase_url = \"{uri}/\"\n\n[session]\nsecure_cookie = false\n\n[oidc]\n\
         issuer = \"{uri}\"\nauthorization_endpoint = \"{uri}/auth\"\n\
         token_endpoint = \"{uri}/token\"\njwks_uri = \"{uri}/jwks.json\"\n\
         client_id = \"ferrofed-viewer\"\n\
         redirect_uri = \"http://127.0.0.1:3000/auth/callback\"\n{oidc}"
    )
}

/// A console at the provider `server`, with `absolute` as the absolute
/// timeout when given.
fn console(
    server: &Server,
    absolute: Option<Duration>,
) -> Result<(ViewerState, axum::Router), Box<dyn Error>> {
    let mut settings = settings(&configuration(&server.uri(), ""))?;
    if let Some(absolute) = absolute {
        settings.session.absolute_timeout = absolute;
    }
    let state = ViewerState::new(settings)?;
    let service = router(state.clone());
    Ok((state, service))
}

/// A session whose access token lasts `lifetime` and that holds [`FIRST`].
fn session(state: &ViewerState, lifetime: Duration) -> Result<SessionId, Box<dyn Error>> {
    Ok(state.sessions().establish(SignedIn {
        access_token: SecretString::from(OPERATOR_TOKEN),
        expires_in: Some(lifetime),
        refresh_token: Some(SecretString::from(FIRST)),
        id_token: Some(SecretString::from(OPERATOR_ID_TOKEN)),
    })?)
}

/// Answers a refresh with `refresh` by the access token `access`, lasting
/// `lifetime` seconds, and the rotated refresh token `rotated` when given,
/// once.
async fn answers(
    server: &Server,
    refresh: &str,
    access: &str,
    lifetime: u64,
    rotated: Option<&str>,
) {
    let rotated = rotated
        .map(|token| format!(r#","refresh_token":"{token}""#))
        .unwrap_or_default();
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains(format!("refresh_token={refresh}")))
        .and(body_string_contains("client_id=ferrofed-viewer"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(format!(
                    r#"{{"access_token":"{access}","token_type":"Bearer","expires_in":{lifetime}{rotated}}}"#
                )),
        )
        .expect(1)
        .mount(server)
        .await;
}

/// The access token the session `id` holds now.
fn held(state: &ViewerState, id: &SessionId) -> Result<String, Box<dyn Error>> {
    Ok(state
        .sessions()
        .access_token(id)?
        .ok_or("a live session")?
        .expose_secret()
        .to_owned())
}

#[tokio::test]
async fn a_session_outlives_its_access_token_by_refreshing_it() -> Result<(), Box<dyn Error>> {
    let server = Server::start().await;
    let (state, service) = console(&server, None)?;
    let id = session(&state, Duration::ZERO)?;
    assert!(state.sessions().access_token(&id)?.is_none());
    answers(&server, FIRST, "synthetic-access-second", 3600, None).await;
    for _ in 0..3 {
        let (response, _body) = send(&service, get_as("/health", &id)?).await?;
        assert_eq!(StatusCode::OK, response.status());
    }
    assert_eq!("synthetic-access-second", held(&state, &id)?);
    // The refreshed token is not due, so the later requests refreshed nothing.
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn a_rotated_refresh_token_is_the_one_sent_next() -> Result<(), Box<dyn Error>> {
    let server = Server::start().await;
    let (state, service) = console(&server, None)?;
    let id = session(&state, Duration::ZERO)?;
    let second = "synthetic-refresh-second";
    answers(&server, FIRST, "synthetic-access-second", 0, Some(second)).await;
    answers(&server, second, "synthetic-access-third", 3600, None).await;
    send(&service, get_as("/health", &id)?).await?;
    send(&service, get_as("/health", &id)?).await?;
    assert_eq!("synthetic-access-third", held(&state, &id)?);
    server.verify().await;
    Ok(())
}

#[tokio::test]
async fn a_refused_refresh_ends_the_session_and_sends_the_operator_to_sign_in()
-> Result<(), Box<dyn Error>> {
    for (status, body) in [
        (400, r#"{"error":"invalid_grant"}"#),
        (401, r#"{"error":"invalid_client"}"#),
        (503, ""),
    ] {
        let server = Server::start().await;
        let (state, service) = console(&server, None)?;
        let id = session(&state, Duration::ZERO)?;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let (response, _body) = send(&service, get_as("/members", &id)?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status(), "{status}");
        assert_eq!("/login", header(&response, "location"), "{status}");
        assert!(
            response
                .headers()
                .get_all("set-cookie")
                .iter()
                .filter_map(|value| value.to_str().ok())
                .any(|cookie| cookie.starts_with("ferrofed_viewer_session=;")
                    && cookie.contains("Max-Age=0")),
            "{status}"
        );
        assert_eq!(0, state.sessions().occupancy()?.sessions, "{status}");
        server.verify().await;
    }
    Ok(())
}

#[tokio::test]
async fn the_absolute_timeout_ends_a_session_whatever_its_refresh_token()
-> Result<(), Box<dyn Error>> {
    let server = Server::start().await;
    let (state, service) = console(&server, Some(Duration::ZERO))?;
    let id = session(&state, Duration::from_secs(3600))?;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let (response, _body) = send(&service, get_as("/members", &id)?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/login", header(&response, "location"));
    assert_eq!(0, state.sessions().occupancy()?.sessions);
    server.verify().await;
    Ok(())
}

/// Every line the console logs while it is captured.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// What was logged.
    fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn the_refresh_token_reaches_no_answer_cookie_or_log_line() -> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);

    let server = Server::start().await;
    let (state, service) = console(&server, None)?;
    let id = session(&state, Duration::ZERO)?;
    let second = "synthetic-refresh-second";
    answers(&server, FIRST, "synthetic-access-second", 0, Some(second)).await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains(format!("refresh_token={second}")))
        .respond_with(ResponseTemplate::new(400).set_body_string(r#"{"error":"invalid_grant"}"#))
        .expect(1)
        .mount(&server)
        .await;
    let mut seen = Vec::new();
    for page in ["/members", "/members", "/health"] {
        let (response, body) = send(&service, get_as(page, &id)?).await?;
        seen.push(body);
        seen.extend(
            response
                .headers()
                .iter()
                .filter_map(|(_, value)| value.to_str().ok().map(str::to_owned)),
        );
    }
    let logged = captured.text();
    assert!(
        logged.contains("could not be refreshed"),
        "the capture saw the refused refresh: {logged}"
    );
    seen.push(logged);
    for text in &seen {
        assert!(!text.contains("synthetic-refresh-"), "{text}");
    }
    assert_eq!(0, state.sessions().occupancy()?.sessions);
    server.verify().await;
    Ok(())
}

/// The query of the authorization request a `GET /login` redirects to.
async fn authorization_request(oidc: &str) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let state = ViewerState::new(settings(&configuration("http://127.0.0.1:9", oidc))?)?;
    let (response, _body) = send(&router(state), get("/login")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    let location = Url::parse(header(&response, "location"))?;
    Ok(location.query_pairs().into_owned().collect())
}

#[tokio::test]
async fn the_sign_in_asks_for_offline_access_only_when_configured() -> Result<(), Box<dyn Error>> {
    let plain = authorization_request("").await?;
    assert_eq!(Some("openid"), plain.get("scope").map(String::as_str));
    assert!(!plain.contains_key("prompt"), "{plain:?}");
    let offline = authorization_request("offline_access = true\n").await?;
    assert_eq!(
        Some("openid offline_access"),
        offline.get("scope").map(String::as_str)
    );
    // OpenID Connect Core 1.0 §11: offline access is asked with consent.
    assert_eq!(Some("consent"), offline.get("prompt").map(String::as_str));
    Ok(())
}

#[test]
fn offline_access_is_refused_in_the_scope_list() -> Result<(), Box<dyn Error>> {
    let text = configuration(
        "http://127.0.0.1:9",
        "scopes = [\"openid\", \"offline_access\"]\n",
    );
    let refused = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve();
    assert!(
        matches!(refused, Err(error::Error::Scope { ref key, .. }) if key == "oidc.scopes"),
        "{refused:?}"
    );
    Ok(())
}
