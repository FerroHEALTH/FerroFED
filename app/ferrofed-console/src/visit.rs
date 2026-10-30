// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server read of the request it draws a page for: the session,
//! the operator, the preferences, the locale, the address and the
//! console's own settings.
//!
//! The [`scoped`] middleware reads it once per request and keeps it for
//! the request's task, so the handler, the reads it makes of the gateway
//! and every part of the page it draws read the same visit (FerroUI
//! `docs/consoles.md`, "How a console builds", step 3). The page is drawn
//! whole from it: the theme and the navigation's rail from the preference
//! cookies, the words in its locale, the marked entry from its path. The
//! page names its title and, where it is a refusal or a fault of its own,
//! its status as it draws ([`Visit::title`], [`Visit::status`]). No
//! specification governs the console: our own design.

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::theme::{Preferences, cookie};
use http::StatusCode;

use crate::server::ConsoleState;

/// The name of the navigation's cookie after the preference prefix.
pub const RAIL: &str = "sidebar";

tokio::task_local! {
    /// The visit of the request the task serves.
    static VISIT: Visit;
}

/// What the server knows of the request's session, which decides the shell
/// the page is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A live signed-in session: the shell.
    SignedIn,
    /// No live session: the sign-in.
    SignedOut,
    /// The session store cannot be read, so the console cannot tell: the
    /// page says so, and offers neither the sign-in nor the pages.
    Unreadable,
}

/// The preferences and the navigation's state one request carries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Chosen {
    /// The family's preferences.
    pub preferences: Preferences,
    /// Whether the navigation is folded to its rail.
    pub rail: bool,
}

/// The preferences the `Cookie` header value `cookies` holds, each named
/// `prefix` then its cookie's name.
#[must_use]
pub fn chosen_in(cookies: &str, prefix: &str) -> Chosen {
    Chosen {
        preferences: Preferences::read(cookies, prefix),
        rail: cookie(cookies, &format!("{prefix}{RAIL}")) == Some("rail"),
    }
}

/// The console's own settings a signed-in operator's pages read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// The gateway's base address.
    pub gateway: String,
    /// The name of each replica whose admin listener the console reaches,
    /// in the order configured; the first is the one the configuration and
    /// a stored query's repair go to.
    pub replicas: Vec<String>,
}

/// What the page names of itself as it is drawn: its title and the status
/// its answer carries.
#[derive(Debug, Default)]
struct Drawn {
    title: Option<String>,
    status: Option<StatusCode>,
}

/// What the server read of the request for the page.
#[derive(Clone)]
pub struct Visit {
    /// The console's state, absent outside a request the console serves.
    state: Option<ConsoleState>,
    /// The request's headers, which the session is read from.
    headers: http::HeaderMap,
    /// The address path.
    pub path: String,
    /// The address query, after the `?`, empty for none.
    pub query: String,
    /// The request's session.
    pub session: SessionState,
    /// The signed-in operator's name, as the provider gave it.
    pub operator: Option<String>,
    /// The request's preference cookies alone, as one `Cookie` header value.
    pub preferences: String,
    /// The prefix each preference cookie's name takes.
    pub prefix: String,
    /// The page's locale.
    pub locale: Locale,
    /// The locale the browser asks for, which Your preferences names as the
    /// browser's own.
    pub browser: Locale,
    /// The console's settings, for a signed-in operator alone.
    pub settings: Option<Settings>,
    /// Whether the console has an OpenID Provider to sign operators in at.
    pub sign_in: bool,
    /// What the page names of itself as it is drawn.
    drawn: Arc<Mutex<Drawn>>,
}

impl fmt::Debug for Visit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // NOTE: no specification governs this: our own design; the headers carry the
        // session cookie and the query what a link put there, so neither is shown.
        f.debug_struct("Visit")
            .field("path", &self.path)
            .field("session", &self.session)
            .field("locale", &self.locale.tag())
            .finish_non_exhaustive()
    }
}

impl Default for Visit {
    fn default() -> Self {
        Self {
            state: None,
            headers: http::HeaderMap::new(),
            path: String::from("/"),
            query: String::new(),
            session: SessionState::SignedOut,
            operator: None,
            preferences: String::new(),
            prefix: String::new(),
            locale: Locale::english(),
            browser: Locale::english(),
            settings: None,
            sign_in: false,
            drawn: Arc::default(),
        }
    }
}

impl Visit {
    /// The visit of the request `parts` on the console `state`.
    #[must_use]
    pub fn of(state: &ConsoleState, parts: &http::request::Parts) -> Self {
        let prefix = state.sessions().preference_prefix();
        let preferences = preference_cookies(&parts.headers, &prefix);
        let session = match crate::read::signed_in_with(state, &parts.headers) {
            Ok(_) => SessionState::SignedIn,
            Err(crate::read::Fault::SignedOut) => SessionState::SignedOut,
            Err(_) => SessionState::Unreadable,
        };
        let settings = (session == SessionState::SignedIn).then(|| Settings {
            gateway: state.settings().gateway.base.to_string(),
            replicas: state
                .settings()
                .admin
                .iter()
                .map(|replica| replica.name.clone())
                .collect(),
        });
        Self {
            path: parts.uri.path().to_owned(),
            query: parts.uri.query().unwrap_or_default().to_owned(),
            session,
            operator: crate::read::operator_of(state, &parts.headers),
            locale: crate::locale::requested_from(state, &parts.headers),
            browser: crate::locale::browser_of(&parts.headers),
            preferences,
            prefix,
            settings,
            sign_in: state.settings().oidc.is_some(),
            headers: parts.headers.clone(),
            state: Some(state.clone()),
            drawn: Arc::default(),
        }
    }

    /// The visit of a page at `path` with the address query `query`, in
    /// `locale`, for a test that draws a part with no request.
    #[must_use]
    pub fn at(path: &str, query: &str, locale: Locale) -> Self {
        Self {
            path: path.to_owned(),
            query: query.to_owned(),
            locale,
            ..Self::default()
        }
    }

    /// The preferences the visit carries.
    #[must_use]
    pub fn chosen(&self) -> Chosen {
        chosen_in(&self.preferences, &self.prefix)
    }

    /// The page's locale.
    #[must_use]
    pub fn locale(&self) -> Locale {
        self.locale.clone()
    }

    /// The names of the replicas whose admin listeners the console reaches;
    /// empty for a visitor.
    #[must_use]
    pub fn replicas(&self) -> Vec<String> {
        self.settings
            .as_ref()
            .map(|settings| settings.replicas.clone())
            .unwrap_or_default()
    }

    /// The console's state, when the visit is a request the console serves.
    #[must_use]
    pub const fn state(&self) -> Option<&ConsoleState> {
        self.state.as_ref()
    }

    /// The request's headers.
    #[must_use]
    pub const fn headers(&self) -> &http::HeaderMap {
        &self.headers
    }

    /// Names the page's title, which the document writes before the
    /// product's name.
    pub fn title(&self, title: String) {
        self.drawn
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .title = Some(title);
    }

    /// Sets the answer's status to `status` unless an earlier part of the
    /// page set a server error, which outranks it (RFC 9110 §15.6).
    pub fn status(&self, status: StatusCode) {
        let mut drawn = self.drawn.lock().unwrap_or_else(PoisonError::into_inner);
        if !drawn.status.is_some_and(|set| set.is_server_error()) {
            drawn.status = Some(status);
        }
    }

    /// The title the page named, if it named one.
    #[must_use]
    pub fn drawn_title(&self) -> Option<String> {
        self.drawn
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .title
            .clone()
    }

    /// The status the page set, if it set one.
    #[must_use]
    pub fn drawn_status(&self) -> Option<StatusCode> {
        self.drawn
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .status
    }
}

/// The preference cookies of `headers`, each named `prefix` then the name
/// of a preference or the navigation's, joined as one `Cookie` header
/// value; the session's cookie shares the prefix and is never among them.
fn preference_cookies(headers: &http::HeaderMap, prefix: &str) -> String {
    // NOTE: RFC 9113 §8.2.3: a request may split its cookies over several
    // headers, which read as one list joined by `; `.
    headers
        .get_all(http::header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .map(str::trim)
        .filter(|pair| {
            pair.split_once('=').is_some_and(|(name, _value)| {
                name.strip_prefix(prefix).is_some_and(|name| {
                    !prefix.is_empty() && (name == RAIL || Preferences::COOKIES.contains(&name))
                })
            })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Reads the visit of `request` and keeps it for the request's task while
/// the rest of the stack and the handler serve it.
pub async fn scoped(
    axum::Extension(state): axum::Extension<ConsoleState>,
    request: Request,
    next: Next,
) -> Response {
    let (parts, body) = request.into_parts();
    let visit = Visit::of(&state, &parts);
    VISIT
        .scope(visit, next.run(Request::from_parts(parts, body)))
        .await
}

/// Runs `draw` with `visit` as the visit of the task, for a test that draws
/// a part of a page with no request.
pub fn drawing<R>(visit: Visit, draw: impl FnOnce() -> R) -> R {
    VISIT.sync_scope(visit, draw)
}

/// The visit of the request the task serves, or a visitor's with no
/// request outside one.
#[must_use]
pub fn visit() -> Visit {
    VISIT.try_with(Clone::clone).unwrap_or_default()
}

/// What `read` reads of the visit of the request the task serves, or of a
/// visitor's with no request outside one.
pub fn with<R>(read: impl FnOnce(&Visit) -> R) -> R {
    if VISIT.try_with(|_| ()).is_ok() {
        VISIT.with(read)
    } else {
        read(&Visit::default())
    }
}
