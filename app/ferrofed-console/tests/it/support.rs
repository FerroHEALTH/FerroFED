// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Shared helpers: a configuration from TOML text, the console's service, and
//! one request through it.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Body;
use ferrofed_console::config::Config;
use ferrofed_console::config::settings::Settings;
use ferrofed_console::server::{ConsoleState, router};
use ferrofed_console::session::{SessionId, SignedIn};
use http::{Request, Response};
use secrecy::SecretString;
use tower::ServiceExt as _;

/// A configuration with an OpenID Provider on loopback, for the sign-in
/// tests.
pub(crate) const WITH_OIDC: &str = r#"
[session]
secure_cookie = false

[oidc]
issuer = "https://idp.example.org/realms/ferrofed"
authorization_endpoint = "https://idp.example.org/realms/ferrofed/auth?kc_idp_hint=example"
token_endpoint = "http://127.0.0.1:9/token"
jwks_uri = "http://127.0.0.1:9/jwks.json"
client_id = "ferrofed-console"
redirect_uri = "http://127.0.0.1:3000/auth/callback"
scopes = ["openid", "profile"]
"#;

/// Resolves the configuration `text` writes, with no environment.
pub(crate) fn settings(text: &str) -> Result<Settings, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// The state and the service for the configuration `text` writes.
pub(crate) fn console(text: &str) -> Result<(ConsoleState, axum::Router), Box<dyn Error>> {
    let state = ConsoleState::new(settings(text)?)?;
    let service = router(state.clone());
    Ok((state, service))
}

/// Sends `request` through `service` and reads the whole answer.
pub(crate) async fn send(
    service: &axum::Router,
    request: Request<Body>,
) -> Result<(Response<()>, String), Box<dyn Error>> {
    let response = service.clone().oneshot(request).await?;
    let (parts, body) = response.into_parts();
    let bytes = axum::body::to_bytes(body, usize::MAX).await?;
    Ok((
        Response::from_parts(parts, ()),
        String::from_utf8(bytes.to_vec())?,
    ))
}

/// A `GET` of `path` with no body.
pub(crate) fn get(path: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path).body(Body::empty())?)
}

/// The value of the header `name` on `response`, or an empty string.
pub(crate) fn header<'a>(response: &'a Response<()>, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

/// The synthetic access token every test operator signs in with.
pub(crate) const OPERATOR_TOKEN: &str = "synthetic-operator-token";

/// The synthetic ID Token every test operator's sign-in leaves.
pub(crate) const OPERATOR_ID_TOKEN: &str = "synthetic.operator.id-token";

/// The synthetic name every test operator signs in under.
pub(crate) const OPERATOR_NAME: &str = "Synthetic Operator";

/// What a completed sign-in leaves a session: [`OPERATOR_TOKEN`] and
/// [`OPERATOR_ID_TOKEN`].
pub(crate) fn signed_in() -> SignedIn {
    SignedIn {
        access_token: SecretString::from(OPERATOR_TOKEN),
        expires_in: None,
        refresh_token: None,
        id_token: Some(SecretString::from(OPERATOR_ID_TOKEN)),
        operator: Some(String::from(OPERATOR_NAME)),
    }
}

/// A `GET` of `path` carrying the session cookie of `session`.
pub(crate) fn get_as(path: &str, session: &SessionId) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path)
        .header(
            "cookie",
            format!("{}={}", ferrofed_console::session::COOKIE, session.as_str()),
        )
        .body(Body::empty())?)
}

/// Every line the console logs while it is captured.
#[derive(Clone, Default)]
pub(crate) struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// Captures every event of this thread, at every level, until the
    /// guard drops.
    pub(crate) fn logging(&self) -> tracing::subscriber::DefaultGuard {
        let writer = self.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::set_default(subscriber)
    }

    /// What was logged.
    pub(crate) fn text(&self) -> String {
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
