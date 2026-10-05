// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page tree the server renders and the browser hydrates: the document
//! shell, the landing page, and the page for a path the console does not
//! serve.

use leptos::prelude::*;
use leptos_meta::{Stylesheet, Title, provide_meta_context};
use leptos_router::components::{Route, Router, Routes};
use leptos_router::{SsrMode, path};

use crate::views;

/// The product name as an operator reads it.
pub const PRODUCT: &str = "FerroFED operator console";

/// The path of the sign-in route the server serves.
pub const SIGN_IN: &str = "/login";

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
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/ferrofed-viewer.css" />
        <Title text=PRODUCT />
        <Router>
            <nav aria-label="Views">
                <ul>
                    <li>
                        <a href="/">"Console"</a>
                    </li>
                    <li>
                        <a href=views::MEMBERS>"Members"</a>
                    </li>
                    <li>
                        <a href=views::INTEGRITY>"Integrity"</a>
                    </li>
                    <li>
                        <a href=views::STORED_QUERIES>"Stored queries"</a>
                    </li>
                    <li>
                        <a href=views::FEDERATION>"Self-description"</a>
                    </li>
                </ul>
            </nav>
            <main>
                <Routes fallback=NotFound>
                    <Route path=path!("/") view=Landing />
                    <Route
                        path=path!("/members")
                        view=views::MembersPage
                        ssr=SsrMode::PartiallyBlocked
                    />
                    <Route
                        path=path!("/integrity")
                        view=views::IntegrityPage
                        ssr=SsrMode::PartiallyBlocked
                    />
                    <Route
                        path=path!("/stored-queries")
                        view=views::StoredQueriesPage
                        ssr=SsrMode::PartiallyBlocked
                    />
                    <Route
                        path=path!("/federation")
                        view=views::FederationPage
                        ssr=SsrMode::PartiallyBlocked
                    />
                </Routes>
            </main>
        </Router>
    }
}

/// The landing page: what the console is, and the way to sign in.
#[component]
fn Landing() -> impl IntoView {
    view! {
        <h1>{PRODUCT}</h1>
        <p>
            "The console shows an operator the state of a FerroFED federation through the "
            "gateway's own public surface. It holds no clinical data."
        </p>
        <p>
            "Sign in to see the members and their health, the integrity incidents and the "
            "creating_system_id routing table, the stored queries, and the gateway's "
            "self-description. The query console is planned."
        </p>
        <p>
            // The sign-in route is the server's, so the client router must not
            // take the click once the page has hydrated.
            <a href=SIGN_IN rel="external">
                "Sign in"
            </a>
        </p>
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
