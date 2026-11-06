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
use ferrohealth_session::clock::ManualClock;
use ferrohealth_session::record::{Credential, Ended, Held, SignedIn};
use ferrohealth_session::sessions::Occupancy;
use http::{HeaderMap, HeaderValue, Request, Response};
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
pub(crate) async fn console(text: &str) -> Result<(ConsoleState, axum::Router), Box<dyn Error>> {
    let state = ConsoleState::new(settings(text)?).await?;
    let service = router(state.clone());
    Ok((state, service))
}

/// The console of [`console`], counting every lifetime on a manual clock
/// the test moves, and that clock.
pub(crate) async fn console_on_clock(
    text: &str,
) -> Result<(ConsoleState, axum::Router, ManualClock), Box<dyn Error>> {
    let time = ManualClock::new(jiff::Timestamp::now().as_second());
    let state = ConsoleState::with_clock(settings(text)?, time.clock()).await?;
    let service = router(state.clone());
    Ok((state, service, time))
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
        request = request.header("cookie", format!("{COOKIE}={}", session.as_str()));
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

/// The synthetic name every test operator signs in under.
pub(crate) const OPERATOR_NAME: &str = "Synthetic Operator";

/// What a completed sign-in leaves a session: [`OPERATOR_TOKEN`] under
/// [`OPERATOR_NAME`].
pub(crate) async fn signed_in() -> Result<SignedIn, Box<dyn Error>> {
    Ok(signed_in_as(Some(OPERATOR_NAME), None, None).await?.0)
}

/// What a completed sign-in leaves a session, with the ID Token it was
/// completed with: [`OPERATOR_TOKEN`] living `expires_in`, the refresh
/// token `refresh_token`, and an ID Token naming the operator `name`.
///
/// The sign-in is completed at a test OpenID Provider of its own
/// (`ferrofed_testkit::console_sign_in`), so the session holds what a real
/// sign-in leaves.
pub(crate) async fn signed_in_as(
    name: Option<&str>,
    expires_in: Option<std::time::Duration>,
    refresh_token: Option<&str>,
) -> Result<(SignedIn, String), Box<dyn Error>> {
    ferrofed_testkit::console_sign_in::signed_in(ferrofed_testkit::console_sign_in::Answer {
        access_token: OPERATOR_TOKEN,
        name,
        expires_in,
        refresh_token,
    })
    .await
}

/// The name of the cookie that carries a session on a console whose cookies
/// are not `Secure`.
pub(crate) const COOKIE: &str = "ferrofed_console_session";

/// The value of a session cookie the console set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionId(String);

impl SessionId {
    /// The session the cookie value `value` names.
    pub(crate) fn from_cookie(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// The cookie's value.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The `Cookie` header a request carrying the session sends.
    fn headers(&self) -> Result<HeaderMap, Box<dyn Error>> {
        let mut headers = HeaderMap::new();
        let cookie = format!("{COOKIE}={}", self.0);
        headers.insert(http::header::COOKIE, HeaderValue::from_str(&cookie)?);
        Ok(headers)
    }
}

/// Begins a session for `signed_in` at the console `state`, as a completed
/// sign-in does, and returns the session its cookie names.
pub(crate) fn establish(
    state: &ConsoleState,
    signed_in: SignedIn,
) -> Result<SessionId, Box<dyn Error>> {
    let set = state.sessions().establish(&HeaderMap::new(), signed_in)?;
    session_of(set.to_str()?).ok_or_else(|| "the sign-in set no session cookie".into())
}

/// The session a `Set-Cookie` value `set` names, when it is the session
/// cookie and not its removal.
pub(crate) fn session_of(set: &str) -> Option<SessionId> {
    let pair = set.split(';').next()?;
    let (name, value) = pair.split_once('=')?;
    (name.trim_start_matches("__Host-") == COOKIE && !value.is_empty())
        .then(|| SessionId::from_cookie(value))
}

/// What the session `id` holds at the console `state`.
pub(crate) fn held(state: &ConsoleState, id: &SessionId) -> Result<Held, Box<dyn Error>> {
    Ok(state.sessions().held(&id.headers()?)?.value)
}

/// The access token the live session `id` carries, or `None` when it has
/// ended.
pub(crate) fn access_token(
    state: &ConsoleState,
    id: &SessionId,
) -> Result<Option<String>, Box<dyn Error>> {
    Ok(match held(state, id)? {
        Held::SignedIn(session) => match session.credential() {
            Credential::Bearer(token) => Some(token.expose().to_owned()),
            Credential::Basic { .. } => None,
        },
        Held::TokenExpired | Held::Ended | Held::Absent => None,
    })
}

/// The operator the live session `id` names.
pub(crate) fn operator(
    state: &ConsoleState,
    id: &SessionId,
) -> Result<Option<String>, Box<dyn Error>> {
    Ok(match held(state, id)? {
        Held::SignedIn(session) => session.operator().map(str::to_owned),
        Held::TokenExpired | Held::Ended | Held::Absent => None,
    })
}

/// Ends the session `id` at the console `state`, as a sign-out does.
pub(crate) fn end(state: &ConsoleState, id: &SessionId) -> Result<Option<Ended>, Box<dyn Error>> {
    Ok(state.sessions().end(&id.headers()?)?.value)
}

/// How many sign-ins and sessions the console `state` holds in memory.
pub(crate) fn occupancy(state: &ConsoleState) -> Result<Occupancy, Box<dyn Error>> {
    state
        .sessions()
        .occupancy()?
        .ok_or_else(|| "the console keeps its sessions in memory".into())
}

/// A `GET` of `path` carrying the session cookie of `session`.
pub(crate) fn get_as(path: &str, session: &SessionId) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(path)
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
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
    if [
        "'nonce-",
        "'unsafe-inline'",
        "'unsafe-eval'",
        "'wasm-unsafe-eval'",
    ]
    .iter()
    .any(|source| policy.contains(source))
    {
        return Err(format!(
            "the policy admits an inline or evaluated script: {policy}"
        ));
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
