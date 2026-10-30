// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages by their address, and the handlers that draw them.
//!
//! [`get`] draws the page a `GET` names, and [`post`] the page a form posts
//! to: a form that answers on its page posts to the page's own address,
//! and the handler draws that page with the answer and the fields as they
//! were posted (FerroUI `docs/consoles.md`, "An action that answers on its
//! page"). A signed-in operator reads each page in the shell; a visitor
//! reads the sign-in, or About. No specification governs the console's
//! routes: our own design.

use axum::body::Bytes;
use axum::response::{IntoResponse, Response};
use ferrohealth_ui::html::Html;
use http::StatusCode;

use crate::act::fields::Fields;
use crate::pages;
use crate::visit::SessionState;

/// A console page, as its path names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    /// `/`.
    Dashboard,
    /// `/topology`.
    Topology,
    /// `/search`.
    Search,
    /// `/organisations`.
    Organisations,
    /// `/organisations/nodes`.
    AllNodes,
    /// `/organisations/:org`.
    Organisation,
    /// `/organisations/:org/:node` and its tabs.
    Node,
    /// `/services`.
    Services,
    /// `/services/:service`.
    Service,
    /// `/integrity`.
    Integrity,
    /// `/query` and its tabs.
    Query,
    /// `/stored-queries`.
    StoredQueries,
    /// `/stored-queries/:name` and its tabs.
    StoredQuery,
    /// `/self-description` and its tabs.
    SelfDescription,
    /// `/configuration`, its tabs and their items.
    Configuration,
    /// `/preferences` and its sections.
    Preferences,
    /// `/about`.
    About,
}

impl Page {
    /// The page the address path `path` names, if it names one.
    #[must_use]
    pub fn of(path: &str) -> Option<Self> {
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        Some(match parts.as_slice() {
            [] => Self::Dashboard,
            ["topology"] => Self::Topology,
            ["search"] => Self::Search,
            ["organisations"] => Self::Organisations,
            ["organisations", "nodes"] => Self::AllNodes,
            ["organisations", _] => Self::Organisation,
            ["organisations", _, _] | ["organisations", _, _, _] => Self::Node,
            ["services"] => Self::Services,
            ["services", _] => Self::Service,
            ["integrity"] => Self::Integrity,
            ["query"] | ["query", _] => Self::Query,
            ["stored-queries"] => Self::StoredQueries,
            ["stored-queries", _] | ["stored-queries", _, _] => Self::StoredQuery,
            ["self-description"] | ["self-description", _] => Self::SelfDescription,
            ["configuration"] | ["configuration", _] | ["configuration", _, _] => {
                Self::Configuration
            }
            ["preferences"] | ["preferences", _] => Self::Preferences,
            ["about"] => Self::About,
            _ => return None,
        })
    }
}

/// The page of a signed-in operator that `page` names, as a `GET` asks.
async fn signed_in_page(page: Option<Page>) -> Html {
    match page {
        Some(Page::Dashboard) => pages::dashboard::page().await,
        Some(Page::Topology) => pages::topology::page().await,
        Some(Page::Search) => pages::search::page().await,
        Some(Page::Organisations) => pages::organisations::list().await,
        Some(Page::AllNodes) => pages::organisations::all_nodes().await,
        Some(Page::Organisation) => pages::organisation::page().await,
        Some(Page::Node) => pages::node::page().await,
        Some(Page::Services) => pages::services::list().await,
        Some(Page::Service) => pages::services::page().await,
        Some(Page::Integrity) => pages::integrity::page(None).await,
        Some(Page::Query) => pages::query::page(None).await,
        Some(Page::StoredQueries) => pages::stored::list().await,
        Some(Page::StoredQuery) => pages::stored::page(None).await,
        Some(Page::SelfDescription) => pages::describe::page().await,
        Some(Page::Configuration) => pages::configuration::page(None).await,
        Some(Page::Preferences) => pages::prefs::page(),
        Some(Page::About) => pages::about::page().await,
        None => pages::errors::not_found(),
    }
}

/// The page a visitor reads at `page`: About, or the sign-in.
fn visitor_page(page: Option<Page>) -> Html {
    match page {
        Some(Page::About) => pages::about::centred(),
        _ => pages::sign_in::page(),
    }
}

/// The page `draw` drew, in the shell for a signed-in operator, as the
/// document the visit asks for.
async fn drawn(page: Option<Page>, draw: impl Future<Output = Html>) -> Response {
    let visit = crate::visit::visit();
    let body = match visit.session {
        SessionState::SignedIn => {
            let content = draw.await;
            crate::shell::shell(content).await
        }
        SessionState::SignedOut => visitor_page(page),
        SessionState::Unreadable => pages::errors::unreadable(),
    };
    crate::app::answer(&visit, body)
}

/// Draws the page a `GET` of the request's address names.
pub async fn get() -> Response {
    let page = crate::visit::with(|visit| Page::of(&visit.path));
    drawn(page, signed_in_page(page)).await
}

/// Draws the page a form posted to, with its answer.
///
/// The forms are the clear of a held `ehr_id` on Integrity, a stored
/// query's drift repair on its page, a query on the query console, and a
/// change of the configuration on the configuration page the form was on.
pub async fn post(body: Bytes) -> Response {
    let page = crate::visit::with(|visit| Page::of(&visit.path));
    let fields = Fields::read(&body);
    let draw = async move {
        match page {
            Some(Page::Integrity) => pages::integrity::page(Some(fields)).await,
            Some(Page::Query) => pages::query::page(Some(fields)).await,
            Some(Page::StoredQuery) => pages::stored::page(Some(fields)).await,
            Some(Page::Configuration) => pages::configuration::page(Some(fields)).await,
            _ => Html::empty(),
        }
    };
    let answers = matches!(
        page,
        Some(Page::Integrity | Page::Query | Page::StoredQuery | Page::Configuration)
    );
    if !answers {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    drawn(page, draw).await
}

#[cfg(test)]
mod tests {
    use super::Page;

    #[test]
    fn every_page_is_named_by_its_address() {
        for path in [
            "/",
            "/topology",
            "/search",
            "/organisations",
            "/organisations/nodes",
            "/organisations/org-a",
            "/organisations/org-a/node-a",
            "/organisations/org-a/node-a/routes",
            "/services",
            "/services/c",
            "/integrity",
            "/query",
            "/query/a",
            "/stored-queries",
            "/stored-queries/d",
            "/stored-queries/d/a",
            "/self-description",
            "/self-description/a",
            "/configuration",
            "/configuration/a",
            "/configuration/a/b",
            "/preferences",
            "/preferences/e",
            "/about",
        ] {
            assert!(Page::of(path).is_some(), "{path}");
        }
        assert_eq!(Some(Page::AllNodes), Page::of("/organisations/nodes"));
        assert_eq!(Some(Page::Organisation), Page::of("/organisations/org-a"));
        assert_eq!(None, Page::of("/no/such/page"));
    }
}
