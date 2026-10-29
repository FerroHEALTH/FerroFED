// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's pages as lazy routes, one for each group of the
//! navigation an operator reads together.
//!
//! Under `cargo leptos build --split` each group loads its code from chunks
//! of its own (the Leptos book, "Code Splitting"), so the first page costs
//! the entry and its group, and every route of a group draws the page its
//! path names. [`ROUTES`] tells the library which link loads which group
//! ahead of a visit (`ferrohealth_ui::widget::prefetch`). No specification
//! governs the grouping: our own design.

use ferrohealth_ui::widget::prefetch::Route;
use leptos::prelude::*;
use leptos_router::{LazyRoute, lazy_route};

use crate::app::guarded;

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

/// Declares the lazy route `$name` of a group of pages, drawing each page
/// `$page` with `$draw`.
///
/// Each group matches its own pages alone, so its chunks hold their code
/// and no other group's, and draws its page again when the address changes.
macro_rules! group {
    ($(#[$doc:meta])* $name:ident { $($page:ident => $draw:expr),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        #[lazy_route]
        impl LazyRoute for $name {
            fn data() -> Self {
                Self
            }

            fn view(_this: Self) -> AnyView {
                let location = leptos_router::hooks::use_location();
                // NOTE: leptos_router, `Route`: a navigation that stays on a route keeps
                // its view, so the group draws the page its address names as it changes.
                (move || {
                    location.search.track();
                    match Page::of(&location.pathname.get()) {
                        $(Some(Page::$page) => $draw,)+
                        _ => ().into_any(),
                    }
                })
                .into_any()
            }
        }
    };
}

group!(
    /// The dashboard, the topology and the find.
    LOverview {
        Dashboard => guarded(crate::pages::dashboard::page),
        Topology => guarded(crate::pages::topology::page),
        Search => guarded(crate::pages::search::page),
    }
);

group!(
    /// The organisations and their nodes, the services and integrity.
    LFederation {
        Organisations => guarded(crate::pages::organisations::list),
        AllNodes => guarded(crate::pages::organisations::all_nodes),
        Organisation => guarded(crate::pages::organisation::page),
        Node => guarded(crate::pages::node::page),
        Services => guarded(crate::pages::services::list),
        Service => guarded(crate::pages::services::page),
        Integrity => guarded(crate::pages::integrity::page),
    }
);

group!(
    /// The query console, the stored queries and the self-description,
    /// which draw the code editor.
    LQueries {
        Query => guarded(crate::pages::query::page),
        StoredQueries => guarded(crate::pages::stored::list),
        StoredQuery => guarded(crate::pages::stored::page),
        SelfDescription => guarded(crate::pages::describe::page),
    }
);

group!(
    /// The configuration revisions and their forms.
    LConfiguration {
        Configuration => guarded(crate::pages::configuration::page),
    }
);

group!(
    /// Your preferences and About, which a visitor reads too.
    LConsole {
        Preferences => guarded(crate::pages::prefs::page),
        About => crate::pages::about::routed(),
    }
);

/// The route at `path`, loaded by the group `$group`.
macro_rules! route {
    ($path:literal, $group:ident, $idle:literal) => {
        Route {
            path: $path,
            preload: || Box::pin($group::preload()),
            idle: $idle,
        }
    };
}

/// Every route by its path and the group that draws it.
///
/// The groups of the main navigation load once the first page is idle,
/// each once, through the first of its routes; every other route loads on
/// a hover or a focus.
pub static ROUTES: &[Route] = &[
    route!("/", LOverview, true),
    route!("/topology", LOverview, false),
    route!("/search", LOverview, false),
    route!("/organisations", LFederation, true),
    route!("/organisations/nodes", LFederation, false),
    route!("/organisations/:org", LFederation, false),
    route!("/organisations/:org/:node", LFederation, false),
    route!("/organisations/:org/:node/:tab", LFederation, false),
    route!("/services", LFederation, false),
    route!("/services/:service", LFederation, false),
    route!("/integrity", LFederation, false),
    route!("/query", LQueries, true),
    route!("/query/:tab", LQueries, false),
    route!("/stored-queries", LQueries, false),
    route!("/stored-queries/:name", LQueries, false),
    route!("/stored-queries/:name/:tab", LQueries, false),
    route!("/self-description", LQueries, false),
    route!("/self-description/:tab", LQueries, false),
    route!("/configuration", LConfiguration, true),
    route!("/configuration/:tab", LConfiguration, false),
    route!("/configuration/:tab/:item", LConfiguration, false),
    route!("/preferences", LConsole, false),
    route!("/preferences/:section", LConsole, false),
    route!("/about", LConsole, true),
];

#[cfg(test)]
mod tests {
    use super::{Page, ROUTES};

    #[test]
    fn every_route_names_a_page() {
        for route in ROUTES {
            let path = route
                .path
                .replace(":org", "org-a")
                .replace(":node", "node-a")
                .replace(":tab", "a")
                .replace(":item", "b")
                .replace(":service", "c")
                .replace(":name", "d")
                .replace(":section", "e");
            assert!(Page::of(&path).is_some(), "{}", route.path);
        }
        assert_eq!(Some(Page::AllNodes), Page::of("/organisations/nodes"));
        assert_eq!(Some(Page::Organisation), Page::of("/organisations/org-a"));
        assert_eq!(None, Page::of("/no/such/page"));
    }
}
