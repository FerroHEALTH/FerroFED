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
    let signed_in = *SharedValue::new_str(signed_in);
    let sidebar = signed_in.then(|| shell::sidebar(administered));
    view! {
        <Stylesheet id="leptos" href="/pkg/ferrofed-viewer.css" />
        <Title text=PRODUCT />
        <Router>
            <div class=styles::PAGE>
                {shell::topbar(signed_in)}
                <div class=styles::BODY>
                    {sidebar} <main class=styles::MAIN tabindex="0">
                        <div class=styles::COLUMN>
                            // NOTE: the Leptos book, `ssr/23_ssr_modes`: a page the gateway fills
                            // renders `Async`, whole in one sweep, so no content waits on a script.
                            <Routes fallback=NotFound>
                                <Route
                                    path=path!("/")
                                    view=move || view! { <Landing signed_in /> }
                                />
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

/// The landing page: what the console is, and the way to sign in, or for a
/// signed-in operator what the navigation leads to.
#[component]
fn Landing(
    /// Whether the request carries a live signed-in session.
    signed_in: bool,
) -> impl IntoView {
    let next = if signed_in {
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

/// Whether the request carries a live signed-in session, which the
/// navigation and the sign-out are drawn for.
fn signed_in() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        views::load::server::signed_in().is_ok()
    }
    // NOTE: the browser reads the value the server decided; this runs only if
    // that value is missing, and then draws the page a visitor sees.
    #[cfg(target_arch = "wasm32")]
    {
        false
    }
}
