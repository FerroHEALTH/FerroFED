// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders.
//!
//! The document carries the operator's theme, density, reduced motion and
//! shortcut hints from their cookies as `data-theme`, `data-density`,
//! `data-motion` and `data-hints`, and the request's locale as `lang`, so a
//! page arrives as the operator chose it before any script runs. A
//! signed-in operator reads every page in the shell; a visitor reads the
//! sign-in and About in the centred layout. Every route renders
//! `SsrMode::Async`: the server waits for every gateway answer and sends
//! the page whole, so it reads with no script (the Leptos book,
//! `ssr/23_ssr_modes`). The browser hydrates only the islands.

use ferrohealth_ui::i18n::provide_locale;
use ferrohealth_ui::island::ToastRegion;
use ferrohealth_ui::widget::toast::RegionWords;
use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::locale::{Msg, tr};
use crate::pages;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED Console";

/// The product version the footer and About name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path the server serves FerroUI's stylesheet at.
pub const STYLESHEET: &str = "/ferrohealth-ui.css";

/// The path the server serves FerroUI's FerroFED theme at, which loads
/// after the stylesheet.
pub const THEME: &str = "/ferrofed.css";

/// The path of the brand mark for a light ground.
pub const MARK_LIGHT: &str = "/brand/ferrofed-icon.svg";

/// The path of the brand mark for a dark ground.
pub const MARK_DARK: &str = "/brand/ferrofed-icon-dark.svg";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

/// The path of the sign-out route the server serves.
pub const SIGN_OUT: &str = "/logout";

/// The path of the About page, which every footer links to.
pub const ABOUT: &str = "/about";

/// The path of the topology.
pub const TOPOLOGY: &str = "/topology";

/// The path of the organisations.
pub const ORGANISATIONS: &str = "/organisations";

/// The path of every node across the organisations.
pub const ALL_NODES: &str = "/organisations/nodes";

/// The path of the services.
pub const SERVICES: &str = "/services";

/// The path of the integrity incidents.
pub const INTEGRITY: &str = "/integrity";

/// The path of the query console.
pub const QUERY: &str = "/query";

/// The path of the stored queries.
pub const STORED_QUERIES: &str = "/stored-queries";

/// The path of the gateway's self-description.
pub const SELF_DESCRIPTION: &str = "/self-description";

/// The path of the configuration.
pub const CONFIGURATION: &str = "/configuration";

/// The path of the find in the top bar.
pub const SEARCH: &str = "/search";

/// The pages a visitor may read with no session: the sign-in at `/` and
/// About, and the routes of the sign-in, the stylesheets, the brand marks,
/// the preference posts and the site bundle.
pub const PUBLIC: [&str; 12] = [
    "/",
    ABOUT,
    SIGN_IN,
    crate::oidc::CALLBACK,
    SIGN_OUT,
    crate::server::HEALTH,
    STYLESHEET,
    THEME,
    MARK_LIGHT,
    MARK_DARK,
    crate::prefs::PREFERENCE,
    "/favicon.ico",
];

/// Whether a request for `path` needs a live signed-in session: every path
/// but the public ones and the site bundle.
#[must_use]
pub fn needs_session(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    !(PUBLIC.contains(&path) || path.starts_with("/pkg/"))
}

/// The whole HTML document the server renders around [`App`], as the
/// request's preference cookies and locale ask.
#[must_use]
pub fn document(options: LeptosOptions) -> impl IntoView {
    let preferences = crate::prefs::requested();
    let lang = crate::locale::requested().tag();
    view! {
        <!DOCTYPE html>
        <html
            lang=lang
            data-theme=preferences.theme.key()
            data-density=preferences.density.key()
            data-motion=preferences.motion.key()
            data-hints=preferences.hints.key()
        >
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <link rel="icon" href="/favicon.ico" />
                <link rel="stylesheet" href=STYLESHEET />
                <link rel="stylesheet" href=THEME />
                <HydrationScripts options islands=true />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
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

/// The root component: the title format, the locale, the routes and the
/// toast region.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the #[component] macro drops a #[must_use] from the function it writes"
)]
pub fn App() -> impl IntoView {
    provide_meta_context();
    provide_locale(crate::locale::requested());
    let session = crate::read::session_state();
    let flash = crate::act::flash::taken();
    view! {
        <Title formatter=|text: String| {
            if text.is_empty() { String::from(PRODUCT) } else { format!("{text} · {PRODUCT}") }
        } />
        <Router>{routes(session)}</Router>
        <ToastRegion words=RegionWords::of_page() flash=flash />
    }
}

/// The page `page` of a signed-in operator in the shell, the sign-in for a
/// visitor, and the unreadable store's page when the session cannot be
/// read.
fn guarded(
    session: SessionState,
    page: fn() -> AnyView,
) -> impl Fn() -> AnyView + Clone + Send + Sync {
    move || match session {
        SessionState::SignedIn => crate::shell::signed_in(page()),
        SessionState::SignedOut => pages::sign_in::page(),
        SessionState::Unreadable => pages::errors::unreadable(),
    }
}

/// The routes, for a request whose session is `session`.
#[expect(
    clippy::too_many_lines,
    reason = "the route table is one list, read as one"
)]
fn routes(session: SessionState) -> AnyView {
    view! {
        <Routes fallback=move || match session {
            SessionState::SignedIn => crate::shell::signed_in(pages::errors::not_found()),
            SessionState::SignedOut => pages::sign_in::page(),
            SessionState::Unreadable => pages::errors::unreadable(),
        }>
            <Route
                path=path!("/")
                view=guarded(session, pages::dashboard::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/about")
                view=move || match session {
                    SessionState::SignedIn => crate::shell::signed_in(pages::about::page()),
                    _ => pages::about::centred(),
                }
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/topology")
                view=guarded(session, pages::topology::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/organisations")
                view=guarded(session, pages::organisations::list)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/organisations/nodes")
                view=guarded(session, pages::organisations::all_nodes)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/organisations/:org")
                view=guarded(session, pages::organisation::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/organisations/:org/:node")
                view=guarded(session, pages::node::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/organisations/:org/:node/:tab")
                view=guarded(session, pages::node::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/services")
                view=guarded(session, pages::services::list)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/services/:service")
                view=guarded(session, pages::services::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/integrity")
                view=guarded(session, pages::integrity::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/query")
                view=guarded(session, pages::query::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/query/:tab")
                view=guarded(session, pages::query::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/stored-queries")
                view=guarded(session, pages::stored::list)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/stored-queries/:name")
                view=guarded(session, pages::stored::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/stored-queries/:name/:tab")
                view=guarded(session, pages::stored::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/self-description")
                view=guarded(session, pages::describe::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/self-description/:tab")
                view=guarded(session, pages::describe::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration")
                view=guarded(session, pages::configuration::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration/:tab")
                view=guarded(session, pages::configuration::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/configuration/:tab/:item")
                view=guarded(session, pages::configuration::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/preferences")
                view=guarded(session, pages::prefs::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/preferences/:section")
                view=guarded(session, pages::prefs::page)
                ssr=SsrMode::Async
            />
            <Route
                path=path!("/search")
                view=guarded(session, pages::search::page)
                ssr=SsrMode::Async
            />
        </Routes>
    }
    .into_any()
}

/// The title of a page, `title` before the product's name.
#[must_use]
pub fn title(title: String) -> impl IntoView {
    view! { <Title text=title /> }
}

/// The words of the product's name, for a title with no page of its own.
#[must_use]
pub fn product() -> String {
    tr(Msg::Product)
}

#[cfg(test)]
mod tests {
    use super::needs_session;

    #[test]
    fn every_page_but_the_sign_in_and_about_needs_a_session() {
        for open in [
            "/",
            "/about",
            "/login",
            "/health",
            "/pkg/ferrofed-console.wasm",
        ] {
            assert!(!needs_session(open), "{open}");
        }
        for closed in [
            "/organisations",
            "/organisations/org-a/node-a",
            "/query",
            "/configuration/history",
            "/preferences",
            "/no/such/page",
        ] {
            assert!(needs_session(closed), "{closed}");
        }
    }
}
