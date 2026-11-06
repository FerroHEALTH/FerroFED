// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's HTTP service: the health routes, the sign-in routes, the
//! pages, the form posts and FerroUI's assets, under one set of browser
//! security headers.
//!
//! Every response carries the browser security headers of
//! `ferrohealth-server`'s `console` (`nosniff`, `DENY` framing, no referrer)
//! and the Content-Security-Policy of a server-rendered console
//! (`ConsoleHeaders::server_rendered`), which admits scripts and
//! stylesheets from the console's own origin alone: `script-src 'self';
//! style-src 'self'` with no nonce, since every page is drawn by the server
//! and FerroUI's modules are files it serves (FerroUI `docs/consoles.md`,
//! "How a console builds", step 6). Every document is `no-store`; FerroUI's
//! assets keep the immutable cache of their versioned path. Nothing is
//! compressed, so a document carrying what the operator entered never opens
//! a compression side channel (BREACH). A request that is not a safe method
//! is taken only from the console's own pages
//! (`ferrohealth_server::same_origin`). The health routes are the family's.
//! No specification governs the console's HTTP surface: our own design.

use std::sync::Arc;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use ferrohealth_health::lifecycle::Lifecycle;
use ferrohealth_health::registry::Registry;
use ferrohealth_server::console::{self, ConsoleHeaders};
use ferrohealth_server::health::Health;
use ferrohealth_server::same_origin::SameOrigin;
use ferrohealth_server::section::ServerSection;
use ferrohealth_server::serve::Drained;
use http::header::{CACHE_CONTROL, CONTENT_TYPE};
use http::{HeaderValue, Method, StatusCode};

use ferrohealth_session::clock::Clock;
use ferrohealth_session::cookies::{CookieNames, HOST_PREFIX};
use ferrohealth_session::record::Held;
use ferrohealth_session::routes::{SignIn, keep_session};
use ferrohealth_session::sessions::Sessions;
use ferrohealth_smart::oidc::RelyingParty;

use crate::config::settings::Settings;
use crate::gateway::{Gateway, GatewayError};
use crate::locale::Msg;
use crate::sign_in::SignInPages;

/// The base name of every cookie the console sets:
/// `ferrofed_console_session` carries the session,
/// `ferrofed_console_sign_in` a pending sign-in, and each preference
/// `ferrofed_console_<preference>`.
pub const COOKIES: &str = "ferrofed_console";

/// The path of the console's summary health route.
pub const HEALTH: &str = ferrohealth_server::health::SUMMARY;

/// What every request handler shares.
#[derive(Debug, Clone)]
pub struct ConsoleState {
    settings: Arc<Settings>,
    sign_in: SignIn<SignInPages>,
    gateway: Gateway,
    health: Health,
}

/// Why the console's state could not be built.
#[derive(Debug, thiserror::Error)]
pub enum StateError {
    /// The client of the gateway could not be built.
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    /// The relying party at the OpenID Provider could not be built: its
    /// client, its discovery (OpenID Connect Discovery 1.0 §4), or endpoints
    /// it cannot use.
    #[error("the OpenID Provider cannot be used")]
    Provider {
        /// What the relying party refused.
        #[source]
        source: Box<ferrohealth_session::config::ConfigError>,
    },
}

impl ConsoleState {
    /// The state for `settings`, with an empty session store; the relying
    /// party discovers the provider's endpoints first when the configuration
    /// does not name them.
    ///
    /// # Errors
    /// Returns [`StateError::Gateway`] when the client of the gateway cannot
    /// be built, and [`StateError::Provider`] for a relying party that
    /// cannot be built or a provider whose metadata cannot be read.
    pub async fn new(settings: Settings) -> Result<Self, StateError> {
        Self::with_clock(settings, Clock::system()).await
    }

    /// The state of [`ConsoleState::new`], counting every sign-in and
    /// session lifetime on `clock`.
    ///
    /// # Errors
    /// As [`ConsoleState::new`].
    pub async fn with_clock(settings: Settings, clock: Clock) -> Result<Self, StateError> {
        let gateway = Gateway::new(&settings.gateway)?;
        let party =
            match &settings.oidc {
                Some(oidc) => Some(oidc.checked.relying_party().await.map_err(|source| {
                    StateError::Provider {
                        source: Box::new(source),
                    }
                })?),
                None => None,
            };
        let sessions = Sessions::with_clock(
            settings.session.clone(),
            CookieNames::for_console(COOKIES),
            clock,
        );
        let pages = SignInPages::new(settings.locale.clone(), prefix(&settings));
        let sign_in = SignIn::new(sessions, party, pages);
        // NOTE: no specification governs this: our own design; the settings are
        // resolved and the clients built, so the console serves from here.
        let lifecycle = Lifecycle::default();
        lifecycle.booted();
        Ok(Self {
            settings: Arc::new(settings),
            sign_in,
            gateway,
            health: Health::new(Registry::default(), lifecycle),
        })
    }

    /// The health routes' view: no indicator, and the lifecycle the drain
    /// moves.
    #[must_use]
    pub fn health(&self) -> &Health {
        &self.health
    }

    /// The lifecycle the health routes and the drain share.
    #[must_use]
    pub fn lifecycle(&self) -> &Lifecycle {
        self.health.lifecycle()
    }

    /// The resolved configuration.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// The sign-in: the session store and the relying party.
    #[must_use]
    pub fn sign_in(&self) -> &SignIn<SignInPages> {
        &self.sign_in
    }

    /// The session store.
    #[must_use]
    pub fn sessions(&self) -> &Sessions {
        self.sign_in.sessions()
    }

    /// The relying party of the OpenID Provider; `None` when no provider is
    /// configured.
    #[must_use]
    pub fn relying_party(&self) -> Option<&RelyingParty> {
        self.sign_in.party()
    }

    /// The name every preference cookie starts with on this console, under
    /// the `__Host-` prefix when the cookies are `Secure`.
    #[must_use]
    pub fn preference_prefix(&self) -> String {
        prefix(&self.settings)
    }

    /// The gateway client.
    #[must_use]
    pub fn gateway(&self) -> &Gateway {
        &self.gateway
    }
}

/// Builds the console's whole HTTP service over `state`.
pub fn router(state: ConsoleState) -> axum::Router {
    let health = ferrohealth_server::health::routes(state.health().clone());
    let pages = axum::Router::new()
        .merge(health)
        .route(
            crate::app::MARK_LIGHT,
            get(|| async { svg(MARK_LIGHT_SVG) }),
        )
        .route(crate::app::MARK_DARK, get(|| async { svg(MARK_DARK_SVG) }))
        .route(crate::app::FAVICON, get(favicon))
        .route(
            crate::pages::about::OPTIONS_DOCUMENT,
            get(crate::act::options_document),
        )
        .route(crate::prefs::PREFERENCE, post(crate::prefs::choose))
        .route(crate::prefs::RESET, post(crate::prefs::reset))
        .route(crate::prefs::SIDEBAR, post(crate::prefs::sidebar))
        .route(
            crate::configure::export::EXPORT,
            get(crate::configure::export::export),
        )
        .route(
            crate::pages::search::ANSWER,
            get(crate::pages::search::find_answer),
        )
        .nest(
            ferrohealth_ui::serve::PREFIX,
            ferrohealth_ui::serve::axum_router::router(),
        )
        .fallback(pages)
        .layer(axum::middleware::from_fn(crate::visit::scoped))
        .layer(axum::middleware::from_fn(clean_query_address))
        .layer(axum::middleware::from_fn(require_session))
        .layer(axum::middleware::from_fn_with_state(
            state.sign_in().clone(),
            keep_session::<SignInPages>,
        ))
        .merge(ferrohealth_session::routes::router(state.sign_in().clone()))
        .layer(Extension(state.clone()))
        .layer(axum::extract::DefaultBodyLimit::max(CONFIGURE_BODY_LIMIT));
    // NOTE: no specification governs this: our own design; the origin check and the body
    // limit hold before a page reads a post, so a refused one reads nothing.
    let headers = ConsoleHeaders::server_rendered(form_action(&state));
    pages
        .layer(axum::middleware::from_fn(
            move |request: Request, next: Next| console::headers(headers.clone(), request, next),
        ))
        .layer(axum::middleware::from_fn(same_origin_only))
        .layer(Extension(state))
        .layer(axum::middleware::from_fn(limit_body))
}

/// Draws the page the request's address names: as a `GET` asks, or with the
/// answer of the form a `POST` sent.
async fn pages(method: Method, body: Bytes) -> Response {
    match method {
        Method::GET | Method::HEAD => crate::routes::get().await,
        Method::POST => crate::routes::post(body).await,
        _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
    }
}

/// The largest body a request to the console carries, but the
/// configuration form's.
pub const BODY_LIMIT: usize = 64 * 1024;

/// The largest body the configuration's import takes: the largest TOML an
/// import sends, each byte form-encoded as three, and the form's other
/// fields.
pub const CONFIGURE_BODY_LIMIT: usize = 3 * crate::configure::import::IMPORT_LIMIT + BODY_LIMIT;

/// Holds every request's body to [`BODY_LIMIT`], or the configuration
/// form's to [`CONFIGURE_BODY_LIMIT`], before anything reads it: a body
/// over the limit is `413` (RFC 9110 §15.5.14), whether its
/// `Content-Length` declares it or it is sent chunked and grows past it.
///
/// The limit holds before the session is checked, so it holds for a
/// caller with no session too. The body is read
/// here, up to the limit and within [`BODY_READ_TIMEOUT`], and handed on
/// whole; a body that breaks off is `400`, and one still unread at the
/// deadline is `408`. Axum's own default limit is raised to [`CONFIGURE_BODY_LIMIT`]
/// beside it, so an import at its limit is read whole.
async fn limit_body(request: Request, next: Next) -> Response {
    use http_body_util::BodyExt as _;
    let limit = if request.uri().path() == crate::act::configure::IMPORT {
        CONFIGURE_BODY_LIMIT
    } else {
        BODY_LIMIT
    };
    let (parts, body) = request.into_parts();
    let declared = parts
        .headers
        .get(http::header::CONTENT_LENGTH)
        .and_then(|length| length.to_str().ok()?.parse::<u64>().ok());
    if declared.is_some_and(|length| u64::try_from(limit).is_ok_and(|limit| length > limit)) {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let read = http_body_util::Limited::new(body, limit).collect();
    match tokio::time::timeout(BODY_READ_TIMEOUT, read).await {
        Ok(Ok(read)) => {
            let body = axum::body::Body::from(read.to_bytes());
            next.run(Request::from_parts(parts, body)).await
        }
        Ok(Err(error)) if error.is::<http_body_util::LengthLimitError>() => {
            StatusCode::PAYLOAD_TOO_LARGE.into_response()
        }
        Ok(Err(_broken)) => StatusCode::BAD_REQUEST.into_response(),
        Err(_elapsed) => StatusCode::REQUEST_TIMEOUT.into_response(),
    }
}

/// How long the console waits for a request's whole body before it answers
/// `408` (RFC 9110 §15.5.9), so a body sent slowly holds no task for long.
pub const BODY_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Sends a request for an operator view that carries no live signed-in
/// session to sign-in, before anything is rendered or asked of the gateway.
///
/// A form's handler checks the session again before it asks a replica.
async fn require_session(request: Request, next: Next) -> Response {
    // NOTE: no specification governs this: our own design; a trailing slash or
    // another case names the same view, so the gate never misses one the router serves.
    let path = request
        .uri()
        .path()
        .trim_end_matches('/')
        .to_ascii_lowercase();
    if !crate::app::needs_session(&path) {
        return next.run(request).await;
    }
    if matches!(request.extensions().get::<Held>(), Some(Held::SignedIn(_))) {
        return next.run(request).await;
    }
    let mut response = StatusCode::SEE_OTHER.into_response();
    response.headers_mut().insert(
        http::header::LOCATION,
        HeaderValue::from_static(crate::app::SIGN_IN),
    );
    response
}

/// The query keys the query console reads from its address: the stored
/// query and the version a stored query's Run link names.
const QUERY_KEYS: [&str; 2] = ["stored", "version"];

/// Sends a `GET` of the query console whose address carries any other key
/// to the same page without it, before anything draws it.
///
/// A query and its parameters travel in a `POST` body (N33), so a key the
/// console never reads, such as a parameter a link put in the address, is
/// dropped rather than echoed into the page's fields that name the address.
async fn clean_query_address(request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let on_console = path == crate::app::QUERY || path.starts_with("/query/");
    let Some(query) = request.uri().query().filter(|_| on_console) else {
        return next.run(request).await;
    };
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if pairs
        .iter()
        .all(|(key, _)| QUERY_KEYS.contains(&key.as_str()))
    {
        return next.run(request).await;
    }
    let kept: Vec<&(String, String)> = pairs
        .iter()
        .filter(|(key, _)| QUERY_KEYS.contains(&key.as_str()))
        .collect();
    let mut location = path.to_owned();
    if !kept.is_empty() {
        let mut form = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in kept {
            form.append_pair(key, value);
        }
        location.push('?');
        location.push_str(&form.finish());
    }
    match HeaderValue::from_str(&location) {
        Ok(location) => {
            let mut response = StatusCode::SEE_OTHER.into_response();
            response
                .headers_mut()
                .insert(http::header::LOCATION, location);
            response
        }
        Err(_illegal) => StatusCode::BAD_REQUEST.into_response(),
    }
}

/// Refuses every request that is not a safe method, a form post and a
/// sign-out among them, unless it comes from the console's own pages, by the
/// check of `ferrohealth_server::same_origin`, before anything reads the
/// session or asks the gateway. The refusal speaks the request's language.
///
/// The console's own origin is the one of the provider's `redirect_uri`; a
/// console with none refuses every unsafe request that carries no fetch
/// metadata. The session cookie is `SameSite=Lax` as well, so a cross-site
/// `POST` carries no session in the first place; this check holds without
/// it.
async fn same_origin_only(
    Extension(state): Extension<ConsoleState>,
    request: Request,
    next: Next,
) -> Response {
    if request.method().is_safe() {
        return next.run(request).await;
    }
    let Err(refused) = same_origin_check(state.settings()).check(request.headers()) else {
        return next.run(request).await;
    };
    tracing::warn!(
        method = %request.method(),
        reason = refused.reason(),
        "a request that did not come from the console's own pages was refused"
    );
    plain(
        &state,
        request.headers(),
        StatusCode::FORBIDDEN,
        Msg::AnswerOwnPagesOnly,
    )
}

/// The same-origin check of the console at the origin of the provider's
/// `redirect_uri`.
fn same_origin_check(settings: &Settings) -> SameOrigin {
    let origin = settings
        .oidc
        .as_ref()
        .and_then(|oidc| url::Url::parse(&oidc.origin()).ok());
    match origin {
        // NOTE: no specification governs this: our own design; an origin that is
        // opaque names none, which refuses every unsafe request lacking fetch metadata.
        Some(origin) => SameOrigin::default()
            .with_origin(&origin)
            .unwrap_or_else(|_opaque| SameOrigin::default()),
        None => SameOrigin::default(),
    }
}

/// A handler's plain-text answer `msg`, in the language of the request
/// whose headers are `headers`, that no cache keeps.
#[must_use]
pub fn plain(
    state: &ConsoleState,
    headers: &http::HeaderMap,
    status: StatusCode,
    msg: Msg,
) -> Response {
    let locale = crate::locale::requested_from(state, headers);
    let text = crate::locale::tr_in(&locale, msg, &[]);
    (status, [(CACHE_CONTROL, "no-store")], text).into_response()
}

/// The FerroFED brand mark for a light ground.
const MARK_LIGHT_SVG: &str = include_str!("../../../assets/brand/ferrofed-icon.svg");

/// The FerroFED brand mark for a dark ground.
const MARK_DARK_SVG: &str = include_str!("../../../assets/brand/ferrofed-icon-dark.svg");

/// An SVG answer of `text`.
fn svg(text: &'static str) -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("image/svg+xml"))],
        text,
    )
        .into_response()
}

/// The brand favicon.
const FAVICON_ICO: &[u8] = include_bytes!("../../../assets/brand/favicon.ico");

/// Serves the brand favicon, which a browser asks for on every first page.
async fn favicon() -> Response {
    (
        [(CONTENT_TYPE, HeaderValue::from_static("image/x-icon"))],
        FAVICON_ICO,
    )
        .into_response()
}

/// The sources of the policy's `form-action`: the console itself, and the
/// origin of the provider's end-session endpoint when one is configured.
///
/// A browser holds every redirect a form submission follows to
/// `form-action` (Content Security Policy Level 3, `form-action`), and the
/// sign-out form is answered with a redirect to that endpoint (OpenID
/// Connect RP-Initiated Logout 1.0 §2), so without its origin the browser
/// blocks the sign-out on its way to the provider.
#[must_use]
pub fn form_action(state: &ConsoleState) -> String {
    let end_session = state
        .relying_party()
        .and_then(|party| party.provider().end_session_endpoint())
        .map(url::Url::origin)
        .filter(url::Origin::is_tuple);
    match end_session {
        Some(origin) => format!("'self' {}", origin.ascii_serialization()),
        None => String::from("'self'"),
    }
}

/// The name every preference cookie of the console `settings` describe
/// starts with, under the `__Host-` prefix when its cookies are `Secure`.
fn prefix(settings: &Settings) -> String {
    let base = format!("{COOKIES}_");
    if settings.session.secure_cookie() {
        format!("{HOST_PREFIX}{base}")
    } else {
        base
    }
}

/// Serves `router` on `listener` until `shutdown` resolves, then drains as
/// `ferrohealth-server` does: `lifecycle` turns draining, and the requests in
/// flight get the family's default shutdown bound.
///
/// # Errors
/// Returns the I/O error of a listener that fails.
pub async fn serve(
    listener: tokio::net::TcpListener,
    router: axum::Router,
    lifecycle: Lifecycle,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<Drained> {
    let section = ServerSection::default();
    ferrohealth_server::serve::serve(
        listener,
        router,
        lifecycle,
        shutdown,
        section.drain_delay(),
        section.shutdown_timeout(),
    )
    .await
}
