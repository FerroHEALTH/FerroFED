// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the server read of the request for the page: the session, the
//! operator, the preferences, the locale and the console's own settings.
//!
//! The server reads it from the request as it draws the first page and the
//! browser takes it serialized with the page, then asks the server again
//! ([`visit_now`]) after each navigation and each post drawn in the page,
//! so both halves draw the same page and no component reads a cookie, the
//! session or the request on the server alone (FerroUI `docs/consoles.md`,
//! "How a console builds", step 4). It carries the preference cookies and
//! never the session cookie, so no session id reaches a script. No
//! specification governs the console: our own design.
#![cfg_attr(
    target_arch = "wasm32",
    allow(
        clippy::unused_async_trait_impl,
        reason = "the server function macro writes the browser half's call as an async trait method; only that target has it"
    )
)]
#![cfg_attr(
    not(target_arch = "wasm32"),
    expect(
        clippy::unused_async,
        reason = "the visit's server function reads the request alone, with nothing to await"
    )
)]

use ferrohealth_ui::i18n::Locale;
use ferrohealth_ui::theme::{Preferences, cookie};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// The name of the navigation's cookie after the preference prefix.
pub const RAIL: &str = "sidebar";

/// What the server knows of the request's session, which decides the shell
/// the page is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Settings {
    /// The gateway's base address.
    pub gateway: String,
    /// The name of each replica whose admin listener the console reaches,
    /// in the order configured; the first is the one the configuration and
    /// a stored query's repair go to.
    pub replicas: Vec<String>,
}

/// What the server read of the request for the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Visit {
    /// The request's session.
    pub session: SessionState,
    /// The signed-in operator's name, as the provider gave it.
    pub operator: Option<String>,
    /// The request's preference cookies alone, as one `Cookie` header value.
    pub preferences: String,
    /// The prefix each preference cookie's name takes.
    pub prefix: String,
    /// The tag of the page's locale.
    pub locale: String,
    /// The tag of the locale the browser asks for, which Your preferences
    /// names as the browser's own.
    pub browser: String,
    /// The console's settings, for a signed-in operator alone.
    pub settings: Option<Settings>,
    /// Whether the console has an OpenID Provider to sign operators in at.
    pub sign_in: bool,
}

impl Default for Visit {
    fn default() -> Self {
        Self {
            session: SessionState::SignedOut,
            operator: None,
            preferences: String::new(),
            prefix: String::new(),
            locale: Locale::english().tag(),
            browser: Locale::english().tag(),
            settings: None,
            sign_in: false,
        }
    }
}

impl Visit {
    /// The preferences the visit carries.
    #[must_use]
    pub fn chosen(&self) -> Chosen {
        chosen_in(&self.preferences, &self.prefix)
    }

    /// The page's locale, English for a tag no catalog names.
    #[must_use]
    pub fn locale(&self) -> Locale {
        // NOTE: no specification governs this: our own design; the server writes a
        // tag it parsed, so a tag that does not parse is English, the fallback.
        crate::locale::configured(&self.locale).unwrap_or_else(|_| Locale::english())
    }

    /// The locale the browser asks for, English for a tag no catalog names.
    #[must_use]
    pub fn browser(&self) -> Locale {
        Locale::parse(&self.browser).unwrap_or_else(|_| Locale::english())
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

    /// What the server reads of the request being served.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn requested() -> Self {
        let state = use_context::<crate::server::ConsoleState>();
        let (preferences, prefix) = requested_preferences();
        let session = crate::read::session_state();
        let settings = (session == SessionState::SignedIn)
            .then(|| {
                state.as_ref().map(|state| Settings {
                    gateway: state.settings().gateway.base.to_string(),
                    replicas: state
                        .settings()
                        .admin
                        .iter()
                        .map(|replica| replica.name.clone())
                        .collect(),
                })
            })
            .flatten();
        Self {
            session,
            operator: crate::read::operator(),
            preferences,
            prefix,
            locale: crate::locale::requested().tag(),
            browser: crate::locale::browser().tag(),
            settings,
            sign_in: state
                .as_ref()
                .is_some_and(|state| state.settings().oidc.is_some()),
        }
    }
}

/// The request's preference cookies as one `Cookie` header value, with the
/// prefix each one's name takes.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn requested_preferences() -> (String, String) {
    let prefix = use_context::<crate::server::ConsoleState>().map_or_else(
        || String::from(crate::session::PREFERENCE_PREFIX),
        |state| state.sessions().preference_prefix(),
    );
    let preferences = use_context::<http::request::Parts>()
        .map(|parts| preference_cookies(&parts.headers, &prefix))
        .unwrap_or_default();
    (preferences, prefix)
}

/// The preference cookies of `headers`, each named `prefix` then the name
/// of a preference or the navigation's, joined as one `Cookie` header
/// value; the session's cookie shares the prefix and is never among them.
#[cfg(not(target_arch = "wasm32"))]
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

/// The visit the page knows, which the browser asks for again after each
/// navigation.
#[derive(Debug, Clone, Copy)]
pub struct Visits(pub RwSignal<Visit>);

/// The visit the page knows ([`Visits`]), or the request's own outside the
/// page tree.
#[must_use]
pub fn visit() -> Visit {
    use_context::<Visits>().map_or_else(server_visit, |visits| visits.0.get())
}

/// The visit the page knows, read without tracking it.
#[must_use]
pub fn visit_untracked() -> Visit {
    use_context::<Visits>().map_or_else(server_visit, |visits| visits.0.get_untracked())
}

/// The visit the server reads of the request; in the browser, which reads
/// the value the server serialized, the default.
#[must_use]
pub fn server_visit() -> Visit {
    #[cfg(not(target_arch = "wasm32"))]
    {
        Visit::requested()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Visit::default()
    }
}

/// The visit of the request that asks, for the page after a navigation.
///
/// # Errors
///
/// When the server function cannot be reached or answered.
#[server(endpoint = "visit")]
pub async fn visit_now() -> Result<Visit, ServerFnError> {
    Ok(Visit::requested())
}

/// Asks the server for the visit again after each navigation and each post
/// drawn in the page, so a preference a post just kept, or a session that
/// ended, reaches the page drawn in the browser.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn FollowVisit() -> impl IntoView {
    let location = leptos_router::hooks::use_location();
    let visits = use_context::<Visits>();
    Effect::watch(
        move || {
            (
                location.pathname.get(),
                location.search.get(),
                ferrohealth_ui::widget::submit::posts().get(),
            )
        },
        move |_, _, _| {
            if let Some(Visits(visits)) = visits {
                leptos::task::spawn_local(async move {
                    match visit_now().await {
                        Ok(now) => {
                            follow_root(&now);
                            // NOTE: no specification governs this: our own design; the same
                            // visit redraws nothing, so a page keeps the focus it took.
                            if visits.get_untracked() != now {
                                visits.set(now);
                            }
                        }
                        Err(error) => leptos::logging::warn!("the visit did not come: {error}"),
                    }
                });
            }
        },
        false,
    );
}

/// Sets the root element's theme, density, motion, hints and language from
/// the visit `now`, as the server writes them on a document it draws, so a
/// preference a post just kept takes effect in the page.
fn follow_root(now: &Visit) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(root) = document().document_element() else {
            return;
        };
        let language = now.locale().tag();
        // NOTE: no specification governs this: our own design; the words of a page are
        // drawn in its locale, so a new language loads the page as a document.
        if root
            .get_attribute("lang")
            .is_some_and(|lang| lang != language)
        {
            if window().location().reload().is_err() {
                leptos::logging::warn!("the browser refused to load the page in its language");
            }
            return;
        }
        let chosen = now.chosen().preferences;
        let attributes = [
            ("data-theme", chosen.theme.key()),
            ("data-density", chosen.density.key()),
            ("data-motion", chosen.motion.key()),
            ("data-hints", chosen.hints.key()),
        ];
        for (name, value) in attributes {
            if root.get_attribute(name).as_deref() != Some(value)
                && root.set_attribute(name, value).is_err()
            {
                leptos::logging::warn!("the browser refused the root's {name}");
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: &Visit = now;
}
