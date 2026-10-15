// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates: the document
//! shell, the landing page, the page for a path the console does not serve,
//! and the footer every page carries.

use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::manufacturer::MANUFACTURER;
use crate::{shell, styles, views};

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED operator console";

/// The product version the footer names.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The whole HTML document the server renders around [`App`].
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the stylesheet, the document title and the routes.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: a value the server decides reaches
    // the hydrating page serialized, so both halves draw the same navigation.
    let administered = *SharedValue::new_str(administered);
    let session = *SharedValue::new_str(session_state);
    let signed_in = session == SessionState::SignedIn;
    let sidebar = signed_in.then(|| shell::sidebar(administered));
    let unreadable = (session == SessionState::Unreadable).then(unreadable);
    view! {
        <Stylesheet id="leptos" href="/pkg/ferrofed-viewer.css" />
        <Title text=PRODUCT />
        <Router>
            <div class=styles::PAGE>
                {shell::topbar(signed_in)}
                <div class=styles::BODY>
                    {sidebar} <main class=styles::MAIN tabindex="0">
                        <div class=styles::COLUMN>
                            {unreadable}
                            // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the gateway fills
                            // renders `Async`, whole in one sweep, so no content waits on a script.
                            <Routes fallback=NotFound>
                                <Route path=path!("/") view=move || view! { <Landing session /> } />
                                <Route
                                    path=path!("/members")
                                    view=views::MembersPage
                                    ssr=SsrMode::Async
                                />
                                <Route
                                    path=path!("/integrity")
                                    view=views::IntegrityPage
                                    ssr=SsrMode::Async
                                />
                                <Route
                                    path=path!("/stored-queries")
                                    view=views::StoredQueriesPage
                                    ssr=SsrMode::Async
                                />
                                <Route
                                    path=path!("/federation")
                                    view=views::FederationPage
                                    ssr=SsrMode::Async
                                />
                                <Route
                                    path=path!("/query")
                                    view=crate::query::QueryPage
                                    ssr=SsrMode::Async
                                />
                                <Route
                                    path=path!("/configuration")
                                    view=crate::configure::ConfigurationPage
                                    ssr=SsrMode::Async
                                />
                            </Routes>
                        </div>
                    </main>
                </div> <Footer />
            </div>
        </Router>
    }
}

/// The footer of every page: the product, its version and its manufacturer,
/// named in the system as Regulation (EU) 2025/327 Art 30(1)(g) asks.
#[component]
fn Footer() -> impl IntoView {
    view! {
        <footer>
            <p>
                {PRODUCT} " v" {VERSION} ". Manufactured by " {MANUFACTURER.name} ", "
                {MANUFACTURER.postal_address} ". Contact: "
                <a href=format!("mailto:{}", MANUFACTURER.email)>{MANUFACTURER.email}</a> ", "
                <a href=MANUFACTURER.website>{MANUFACTURER.website}</a> "."
            </p>
        </footer>
    }
}

/// The notice a page carries when the console cannot read its session store,
/// in place of the sign-in a visitor would be offered.
fn unreadable() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
    }
    view! {
        <p role="alert">
            "The console cannot read its session store, so it cannot tell whether you are "
            "signed in: 500. Ask the console's operator to restart it."
        </p>
    }
}

/// The landing page: what the console is, and the way to sign in, or for a
/// signed-in operator what the navigation leads to.
#[component]
fn Landing(
    /// What the server knows of the request's session.
    session: SessionState,
) -> impl IntoView {
    let next = if session == SessionState::Unreadable {
        ().into_any()
    } else if session == SessionState::SignedIn {
        view! {
            <p>
                "The navigation leads to the members and their health, the integrity incidents "
                "and the creating_system_id routing table, the stored queries, the gateway's "
                "self-description, and the query console, which runs an AQL or a stored query "
                "through the gateway and shows every node's status beside the rows."
            </p>
        }
        .into_any()
    } else {
        view! {
            <p>
                "Sign in to see the members and their health, the integrity incidents and the "
                "creating_system_id routing table, the stored queries, the gateway's "
                "self-description, and the query console."
            </p>
            <p>
                // The sign-in route is the server's, so the client router must not
                // take the click once the page has hydrated.
                <a href=SIGN_IN rel="external">
                    "Sign in"
                </a>
            </p>
        }
        .into_any()
    };
    view! {
        <h1>{PRODUCT}</h1>
        <p>
            "The console shows an operator the state of a FerroFED federation through the "
            "gateway's own public surface. It holds no clinical data."
        </p>
        {next}
    }
}

/// The page for a path the console does not serve.
#[component]
fn NotFound() -> impl IntoView {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        response.set_status(http::StatusCode::NOT_FOUND);
    }
    view! {
        <Title text=format!("Not found · {PRODUCT}") />
        <h1>"Not found"</h1>
        <p>
            <a href="/">"Back to the console"</a>
        </p>
    }
}

/// Whether the console is configured with a replica's admin listener, which
/// the configuration page and the admin actions need.
fn administered() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use_context::<crate::server::ViewerState>()
            .is_some_and(|state| !state.settings().admin.is_empty())
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then shows the link the page explains.
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
}

/// What the server knows of the request's session, which decides the shell
/// the page is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// A live signed-in session: the navigation and the sign-out.
    SignedIn,
    /// No live session: the sign-in, and neither the navigation nor the
    /// sign-out.
    SignedOut,
    /// The session store cannot be read, so the console cannot tell: the
    /// page says so, and offers neither the sign-in nor the views.
    Unreadable,
}

impl SessionState {
    /// The state a session lookup's answer `looked_up` stands for: only a
    /// missing or ended session is signed out, and any other fault is
    /// unreadable.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn of<T>(looked_up: &Result<T, views::model::ViewError>) -> Self {
        match looked_up {
            Ok(_) => Self::SignedIn,
            Err(views::model::ViewError::SignedOut) => Self::SignedOut,
            Err(_) => Self::Unreadable,
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SignedIn => "in",
            Self::SignedOut => "out",
            Self::Unreadable => "unreadable",
        })
    }
}

impl std::str::FromStr for SessionState {
    type Err = UnknownSessionState;

    fn from_str(said: &str) -> Result<Self, Self::Err> {
        match said {
            "in" => Ok(Self::SignedIn),
            "out" => Ok(Self::SignedOut),
            "unreadable" => Ok(Self::Unreadable),
            _ => Err(UnknownSessionState),
        }
    }
}

/// A serialized [`SessionState`] the browser cannot read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the page names a session state the console does not know")]
pub struct UnknownSessionState;

/// The state of the request's session, which the shell is drawn for.
fn session_state() -> SessionState {
    #[cfg(not(target_arch = "wasm32"))]
    {
        SessionState::of(&views::load::server::signed_in())
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the page a visitor sees.
    #[cfg(target_arch = "wasm32")]
    {
        SessionState::SignedOut
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::SessionState;
    use crate::views::model::ViewError;

    #[test]
    fn only_a_missing_session_reads_as_signed_out_and_a_broken_store_as_unreadable() {
        assert_eq!(
            SessionState::SignedIn,
            SessionState::of(&Ok::<(), ViewError>(()))
        );
        assert_eq!(
            SessionState::SignedOut,
            SessionState::of(&Err::<(), _>(ViewError::SignedOut))
        );
        assert_eq!(
            SessionState::Unreadable,
            SessionState::of(&Err::<(), _>(ViewError::Unavailable))
        );
    }

    #[test]
    fn the_state_the_page_carries_reads_back_as_itself() {
        for state in [
            SessionState::SignedIn,
            SessionState::SignedOut,
            SessionState::Unreadable,
        ] {
            assert_eq!(Ok(state), state.to_string().parse());
        }
        assert!("signed in".parse::<SessionState>().is_err());
    }
}
