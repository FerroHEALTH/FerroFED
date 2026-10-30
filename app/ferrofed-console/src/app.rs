// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's paths, and the document the server draws around every
//! page.
//!
//! Every page is one document the server draws whole (FerroUI
//! `docs/consoles.md`, "How a console builds"): the handler reads what the
//! page draws of the gateway, then draws it through
//! `ferrohealth_ui::view::shell::document`, which writes the operator's
//! theme, density, reduced motion and shortcut hints from their cookies as
//! `data-theme`, `data-density`, `data-motion` and `data-hints`, the
//! request's locale as `lang`, the library's stylesheet, its FerroFED theme
//! and its one loader script, so a page arrives as the operator chose it
//! before any script runs. A signed-in operator reads every page in the
//! shell ([`crate::shell`]); a visitor reads the sign-in and About in the
//! centred layout.

use axum::response::{IntoResponse, Response};
use ferrohealth_ui::html::Html;
use ferrohealth_ui::serve::Assets;
use ferrohealth_ui::style::Brand;
use ferrohealth_ui::view::shell::{Document, document};
use http::header::CACHE_CONTROL;
use http::{HeaderValue, StatusCode};

use crate::locale::{Msg, tr};
use crate::visit::Visit;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED Console";

/// The product version the footer and About name.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The path of the brand mark for a light ground.
pub const MARK_LIGHT: &str = "/brand/ferrofed-icon.svg";

/// The path of the brand mark for a dark ground.
pub const MARK_DARK: &str = "/brand/ferrofed-icon-dark.svg";

/// The path of the favicon a browser asks for on every first page.
pub const FAVICON: &str = "/favicon.ico";

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
/// About, and the routes of the sign-in, the health routes, the brand marks,
/// the favicon and the preference posts.
pub const PUBLIC: [&str; 12] = [
    "/",
    ABOUT,
    SIGN_IN,
    crate::oidc::CALLBACK,
    SIGN_OUT,
    crate::server::HEALTH,
    ferrohealth_server::health::LIVENESS,
    ferrohealth_server::health::READINESS,
    MARK_LIGHT,
    MARK_DARK,
    crate::prefs::PREFERENCE,
    FAVICON,
];

/// Whether a request for `path` needs a live signed-in session: every path
/// but the public ones and FerroUI's assets.
#[must_use]
pub fn needs_session(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let path = if path.is_empty() { "/" } else { path };
    let assets = path
        .strip_prefix(ferrohealth_ui::serve::PREFIX)
        .is_some_and(|rest| rest.starts_with('/'));
    !(PUBLIC.contains(&path) || assets)
}

/// Names the page's title, `title` before the product's name, and draws
/// nothing.
#[must_use]
pub fn title(title: String) -> Html {
    crate::visit::with(|visit| visit.title(title));
    Html::empty()
}

/// The words of the product's name, for a title with no page of its own.
#[must_use]
pub fn product() -> String {
    tr(Msg::Product)
}

/// The document title of a page that named `title`.
fn document_title(title: Option<String>) -> String {
    match title.filter(|title| !title.is_empty()) {
        Some(title) => format!("{title} · {PRODUCT}"),
        None => String::from(PRODUCT),
    }
}

/// The whole document around `body`, as the visit `visit` asks: its
/// locale, the operator's preferences and the title the page named.
#[must_use]
pub fn document_of(visit: &Visit, body: Html) -> Html {
    document(Document {
        locale: visit.locale(),
        preferences: visit.chosen().preferences,
        title: document_title(visit.drawn_title()),
        assets: Assets::of(Brand::Fed),
        nonce: None,
        head: Html::raw(format!("<link rel=\"icon\" href=\"{FAVICON}\">")),
        body,
    })
}

/// The answer of the document around `body`, with the status the page set
/// (`200` when it set none); no cache keeps it.
#[must_use]
pub fn answer(visit: &Visit, body: Html) -> Response {
    let status = visit.drawn_status().unwrap_or(StatusCode::OK);
    let page = String::from(document_of(visit, body));
    (
        status,
        [
            (
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (CACHE_CONTROL, HeaderValue::from_static("no-store")),
        ],
        page,
    )
        .into_response()
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
            "/favicon.ico",
            "/ferrohealth-ui/0.6.0/ferrohealth-ui.css",
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
            "/ferrohealth-ui-other",
        ] {
            assert!(needs_session(closed), "{closed}");
        }
    }
}
