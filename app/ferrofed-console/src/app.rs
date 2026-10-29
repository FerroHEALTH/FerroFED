// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server draws and the browser hydrates whole.
//!
//! The server draws the first page and the browser takes the whole tree
//! over, then draws every page after it, each group of pages a lazy route
//! ([`crate::routes`]), so no link or form reloads the document (FerroUI
//! `docs/consoles.md`, "How a console builds"). The document carries the
//! operator's theme, density, reduced motion and shortcut hints from their
//! cookies as `data-theme`, `data-density`, `data-motion` and `data-hints`,
//! and the request's locale as `lang`, so a page arrives as the operator
//! chose it before any script runs. A signed-in operator reads every page
//! in the shell; a visitor reads the sign-in and About in the centred
//! layout. Every route renders `SsrMode::Async`: the server waits for every
//! gateway answer and sends the page whole, so it reads with no script (the
//! Leptos book, `ssr/23_ssr_modes`).

use ferrohealth_ui::i18n::provide_locale;
use ferrohealth_ui::island::{FormNavigator, PageAnnouncer, ToastRegion};
use ferrohealth_ui::widget::toast::RegionWords;
use leptos::prelude::*;
use leptos_meta::{Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{Lazy, SsrMode, path};

use crate::locale::{Msg, tr};
use crate::pages;
use crate::routes::{LConfiguration, LConsole, LFederation, LOverview, LQueries};
use crate::visit::{FollowVisit, SessionState, Visits};

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
/// About, and the routes of the sign-in, the health routes, the stylesheets,
/// the brand marks, the preference posts and the site bundle.
#[cfg(not(target_arch = "wasm32"))]
pub const PUBLIC: [&str; 14] = [
    "/",
    ABOUT,
    SIGN_IN,
    crate::oidc::CALLBACK,
    SIGN_OUT,
    crate::server::HEALTH,
    ferrohealth_server::health::LIVENESS,
    ferrohealth_server::health::READINESS,
    STYLESHEET,
    THEME,
    MARK_LIGHT,
    MARK_DARK,
    crate::prefs::PREFERENCE,
    "/favicon.ico",
];

/// Whether a request for `path` needs a live signed-in session: every path
/// but the public ones and the site bundle.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn needs_session(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    // NOTE: no specification governs this: our own design; a server function checks the
    // session itself and answers a visitor's page, such as the visit, so it is not gated here.
    !(PUBLIC.contains(&path) || path.starts_with("/pkg/") || path.starts_with("/api/"))
}

/// The whole HTML document the server renders around [`App`], as the
/// request's preference cookies and locale ask.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn document(options: LeptosOptions) -> impl IntoView {
    let visit = crate::visit::Visit::requested();
    let preferences = visit.chosen().preferences;
    let lang = visit.locale().tag();
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
                <HydrationScripts options />
                <leptos_meta::MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

/// The root component: the title format, the routes in the layout the
/// visit's session calls for, the toast region and the page announcer.
///
/// The server draws it and the browser hydrates it whole, both from the
/// same [`crate::visit::Visit`], which the browser asks for again after
/// each navigation.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    // NOTE: the Leptos book, `ssr/22_life_cycle`: what the server read reaches the
    // hydrating page serialized, so both halves draw the same page.
    let visits = RwSignal::new((*SharedValue::new(crate::visit::server_visit)).clone());
    provide_context(Visits(visits));
    provide_context(crate::read::Changes(RwSignal::new(0)));
    provide_locale(visits.with_untracked(crate::visit::Visit::locale));
    ferrohealth_ui::widget::prefetch::routes(crate::routes::ROUTES);
    let session = Memo::new(move |_| visits.with(|visit| visit.session));
    let region = move || {
        (session.get() == SessionState::SignedIn).then(|| {
            view! { <ToastRegion words=RegionWords::of_page() flash=Vec::new() /> }
        })
    };
    view! {
        <Title formatter=|text: String| {
            if text.is_empty() { String::from(PRODUCT) } else { format!("{text} · {PRODUCT}") }
        } />
        <Router>
            <FollowVisit />
            <FormNavigator />
            {move || layout(session.get())}
        </Router>
        {region}
        <PageAnnouncer />
    }
}

/// The page for a request whose session is `session`.
fn layout(session: SessionState) -> AnyView {
    // NOTE: leptos_axum, `generate_route_list`: the server lists its routes by
    // rendering the app with no request, so every layout draws the same routes.
    let routes = routes(session);
    match session {
        SessionState::SignedIn => crate::shell::shell(routes),
        SessionState::SignedOut | SessionState::Unreadable => routes,
    }
}

/// The page `page` for the visit's session: the page for a signed-in
/// operator, the sign-in for a visitor, and the unreadable store's page
/// when the session cannot be read.
pub fn guarded(page: fn() -> AnyView) -> AnyView {
    match crate::visit::visit_untracked().session {
        SessionState::SignedIn => page(),
        SessionState::SignedOut => pages::sign_in::page(),
        SessionState::Unreadable => pages::errors::unreadable(),
    }
}

/// The routes, for a request whose session is `session`.
fn routes(session: SessionState) -> AnyView {
    let fallback = move || match session {
        SessionState::SignedIn => pages::errors::not_found(),
        SessionState::SignedOut => pages::sign_in::page(),
        SessionState::Unreadable => pages::errors::unreadable(),
    };
    let overview = Lazy::<LOverview>::new;
    let federation = Lazy::<LFederation>::new;
    let queries = Lazy::<LQueries>::new;
    let configuration = Lazy::<LConfiguration>::new;
    let console = Lazy::<LConsole>::new;
    view! {
        <Routes fallback>
            <Route path=path!("/") view=overview() ssr=SsrMode::Async />
            <Route path=path!("/topology") view=overview() ssr=SsrMode::Async />
            <Route path=path!("/search") view=overview() ssr=SsrMode::Async />
            <Route path=path!("/organisations") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/organisations/nodes") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/organisations/:org") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/organisations/:org/:node") view=federation() ssr=SsrMode::Async />
            <Route
                path=path!("/organisations/:org/:node/:tab")
                view=federation()
                ssr=SsrMode::Async
            />
            <Route path=path!("/services") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/services/:service") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/integrity") view=federation() ssr=SsrMode::Async />
            <Route path=path!("/query") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/query/:tab") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/stored-queries") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/stored-queries/:name") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/stored-queries/:name/:tab") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/self-description") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/self-description/:tab") view=queries() ssr=SsrMode::Async />
            <Route path=path!("/configuration") view=configuration() ssr=SsrMode::Async />
            <Route path=path!("/configuration/:tab") view=configuration() ssr=SsrMode::Async />
            <Route
                path=path!("/configuration/:tab/:item")
                view=configuration()
                ssr=SsrMode::Async
            />
            <Route path=path!("/preferences") view=console() ssr=SsrMode::Async />
            <Route path=path!("/preferences/:section") view=console() ssr=SsrMode::Async />
            <Route path=path!("/about") view=console() ssr=SsrMode::Async />
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
