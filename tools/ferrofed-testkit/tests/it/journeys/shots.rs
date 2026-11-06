// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pages the book's screenshots show, each with the route it answers,
//! the path the browser loads and the element it waits for. No
//! specification governs them: our own design.

/// One screenshot of a console page.
#[derive(Debug)]
pub(crate) struct Shot {
    /// The image's name, without the theme suffix and the extension.
    pub(crate) image: &'static str,
    /// The route the page answers, as `ferrofed_console::routes::ROUTES` names
    /// it, or `*` for the fallback.
    pub(crate) route: &'static str,
    /// The path the browser loads.
    pub(crate) url: &'static str,
    /// The `XPath` of an element the page shows once it has loaded.
    pub(crate) ready: &'static str,
}

/// The sign-in card a visitor reads.
pub(crate) const SIGN_IN: Shot = Shot {
    image: "sign-in",
    route: "/",
    url: "/",
    ready: "//a[normalize-space(.) = \"Sign in with your organisation's account\"]",
};

/// The pages a signed-in operator loads and reads as they are.
pub(crate) const PAGES: [Shot; 29] = [
    Shot {
        image: "dashboard",
        route: "/",
        url: "/",
        ready: "//h2[contains(., 'Nodes that need attention')]",
    },
    Shot {
        image: "topology",
        route: "/topology",
        url: "/topology",
        ready: "//h1",
    },
    Shot {
        image: "organisations",
        route: "/organisations",
        url: "/organisations",
        ready: "//main//a[@href = '/organisations/org-a']",
    },
    Shot {
        image: "nodes",
        route: "/organisations/nodes",
        url: "/organisations/nodes",
        ready: "//main//a[@href = '/organisations/org-a/node-a']",
    },
    Shot {
        image: "organisation",
        route: "/organisations/:org",
        url: "/organisations/org-a",
        ready: "//main//a[@href = '/organisations/org-a/node-a']",
    },
    Shot {
        image: "node",
        route: "/organisations/:org/:node",
        url: "/organisations/org-a/node-a",
        ready: "//h1",
    },
    Shot {
        image: "node-routes",
        route: "/organisations/:org/:node/:tab",
        url: "/organisations/org-a/node-a/routes",
        ready: "//caption[. = 'Routes']",
    },
    Shot {
        image: "services",
        route: "/services",
        url: "/services",
        ready: "//h1",
    },
    Shot {
        image: "service",
        route: "/services/:service",
        url: "/services/resolver",
        ready: "//h1",
    },
    Shot {
        image: "integrity",
        route: "/integrity",
        url: "/integrity",
        ready: "//h1",
    },
    Shot {
        image: "query",
        route: "/query",
        url: "/query",
        ready: "//form[@id = 'query-form']//button[@type = 'submit' and normalize-space(.) = 'Run']",
    },
    Shot {
        image: "query-stored",
        route: "/query/:tab",
        url: "/query/stored",
        ready: "//*[@id = 'query-name']",
    },
    Shot {
        image: "stored-queries",
        route: "/stored-queries",
        url: "/stored-queries",
        ready: "//h1",
    },
    Shot {
        image: "stored-query",
        route: "/stored-queries/:name",
        url: "/stored-queries/org.example::journey_001",
        ready: "//section[contains(@class, 'fui-code-panel')]",
    },
    Shot {
        image: "stored-query-versions",
        route: "/stored-queries/:name/:tab",
        url: "/stored-queries/org.example::journey_001/versions",
        ready: "//main//table",
    },
    Shot {
        image: "self-description",
        route: "/self-description",
        url: "/self-description",
        ready: "//code[. = 'example-federation']",
    },
    Shot {
        image: "self-description-limits",
        route: "/self-description/:tab",
        url: "/self-description/limits",
        ready: "//h1",
    },
    Shot {
        image: "configuration-federation",
        route: "/configuration",
        url: "/configuration",
        ready: "//a[@data-tab = 'tab-federation'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-settings-edit",
        route: "/configuration/:tab/:item",
        url: "/configuration/federation/edit",
        ready: "//button[@type = 'submit' and contains(., 'Save as revision')]",
    },
    Shot {
        image: "configuration-stored-queries",
        route: "/configuration/:tab",
        url: "/configuration/stored-queries",
        ready: "//a[@data-tab = 'tab-stored-queries'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-members",
        route: "/configuration/:tab",
        url: "/configuration/members",
        ready: "//a[@data-tab = 'tab-members'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-credentials",
        route: "/configuration/:tab",
        url: "/configuration/credentials",
        ready: "//a[@data-tab = 'tab-credentials'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-history",
        route: "/configuration/:tab",
        url: "/configuration/history",
        ready: "//a[@data-tab = 'tab-history'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-import-export",
        route: "/configuration/:tab",
        url: "/configuration/import-export",
        ready: "//a[@data-tab = 'tab-import-export'][@aria-current = 'page']",
    },
    Shot {
        image: "configuration-member-form",
        route: "/configuration/:tab/:item",
        url: "/configuration/members/new",
        ready: "//button[@type = 'submit' and contains(., 'Save as revision')]",
    },
    Shot {
        image: "preferences",
        route: "/preferences",
        url: "/preferences",
        ready: "//h1",
    },
    Shot {
        image: "preferences-language",
        route: "/preferences/:section",
        url: "/preferences/language",
        ready: "//h1",
    },
    Shot {
        image: "search",
        route: "/search",
        url: "/search?q=node",
        ready: "//h1",
    },
    Shot {
        image: "about",
        route: "/about",
        url: "/about",
        ready: "//h1",
    },
];

/// The page no route answers.
pub(crate) const NOT_FOUND: Shot = Shot {
    image: "not-found",
    route: "*",
    url: "/no/such/page",
    ready: "//h1",
};

/// A complete answer: node A alone answers.
pub(crate) const QUERY_COMPLETE: Shot = Shot {
    image: "query-complete",
    route: "/query",
    url: "/query",
    ready: "//span[contains(@class, 'fui-pill') and . = 'Complete']",
};

/// An incomplete answer: both nodes, best effort, and node B fails.
pub(crate) const QUERY_PARTIAL: Shot = Shot {
    image: "query-partial",
    route: "/query",
    url: "/query",
    ready: "//span[contains(@class, 'fui-pill') and . = 'Incomplete']",
};

/// A query the gateway refuses.
pub(crate) const QUERY_REFUSED: Shot = Shot {
    image: "query-refused",
    route: "/query",
    url: "/query",
    ready: "//main//*[@role = 'alert'][contains(., '400')]",
};

/// Every shot the capture takes.
pub(crate) fn shots() -> impl Iterator<Item = &'static Shot> {
    [&SIGN_IN].into_iter().chain(PAGES.iter()).chain([
        &QUERY_COMPLETE,
        &QUERY_PARTIAL,
        &QUERY_REFUSED,
        &NOT_FOUND,
    ])
}
