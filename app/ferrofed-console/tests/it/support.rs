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

/// A plain post of the form body `body` to the page at `page`, as the
/// console's own form posts it, carrying the session cookie of `session`
/// when given and the headers `from` names.
pub(crate) fn answer_post(
    page: &str,
    body: &str,
    session: Option<&SessionId>,
    from: &[(&str, &str)],
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(page)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html,application/xhtml+xml");
    if let Some(session) = session {
        request = request.header(
            "cookie",
            format!("{}={}", ferrofed_console::session::COOKIE, session.as_str()),
        );
    }
    for (name, value) in from {
        request = request.header(*name, *value);
    }
    Ok(request.body(Body::from(body.to_owned()))?)
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

/// Whether the policy `policy` admits the scripts and stylesheets of the
/// console's own origin alone, and the page `body` loads FerroUI's one
/// loader module from there: the strict policy every page carries (FerroUI
/// `docs/consoles.md`, "How a console builds", step 6).
pub(crate) fn strict(policy: &str, body: &str) -> Result<(), String> {
    let loader = ferrohealth_ui::serve::Assets::of(ferrohealth_ui::style::Brand::Fed).loader();
    let tag = format!(r#"<script type="module" src="{loader}"></script>"#);
    if !policy.contains("script-src 'self';") || !policy.contains("style-src 'self';") {
        return Err(format!("the policy admits more than the console: {policy}"));
    }
    if ["'nonce-", "'unsafe-inline'", "'unsafe-eval'", "'wasm-unsafe-eval'"]
        .iter()
        .any(|source| policy.contains(source))
    {
        return Err(format!("the policy admits an inline or evaluated script: {policy}"));
    }
    if !body.contains(&tag) {
        return Err(format!("the page does not load {loader}: {body}"));
    }
    if body.split("<script").skip(1).any(|script| {
        let opening = script.split('>').next().unwrap_or_default();
        !opening.contains(&format!(r#"src="{loader}""#))
    }) {
        return Err(format!("every script is the loader: {body}"));
    }
    Ok(())
}
