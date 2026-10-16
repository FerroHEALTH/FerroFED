// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page at `/`: the sign-in card a visitor reads, and the dashboard a
//! signed-in operator lands on.
//!
//! The sign-in is static, so the server renders its markup and the page
//! shows that HTML, the way the admin actions' fields are shown: the
//! browser bundle carries none of its drawing. The dashboard is read from
//! the gateway, and the server renders it too ([`crate::dashboard`]); it
//! reaches the page through a server function, so a navigation of the
//! client router loads it as a full load does. Leptos escapes every text
//! and attribute value in both.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::SIGN_IN;
use crate::components::{icon, page_header};
use crate::styles;
use crate::views::titled;

/// The markup `render` writes on the server, which the browser reads back
/// from the page it hydrates; empty in the browser only if the page carries
/// none, which a full load never does.
pub(crate) fn rendered(render: impl FnOnce() -> String) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    {
        render()
    }
    // NOTE: the Leptos book, `ssr/22_life_cycle`: the browser reads the value the
    // server rendered, so this runs only for a page that carries none.
    #[cfg(target_arch = "wasm32")]
    {
        drop(render);
        String::new()
    }
}

/// The sign-in: a card with the mark centred on the page, as the family's
/// viewers draw it.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn SignInPage() -> impl IntoView {
    let html = SharedValue::new_str(|| rendered(sign_in)).into_inner();
    view! {
        <Title text=titled("Sign in") />
        <div class="contents" inner_html=html></div>
    }
}

/// The markup of the sign-in card.
#[must_use]
pub fn sign_in() -> String {
    view! {
        <div class=styles::CENTRED>
            <div class="w-full max-w-sm rounded-card border border-line bg-raised p-8 text-center shadow-card">
                <div class="mb-6 flex items-center justify-center gap-2">
                    <span class=styles::MARK aria-hidden="true"></span>
                    <span class="text-lg font-semibold whitespace-nowrap">
                        "FerroFED " <span class=styles::LOCKUP_ACCENT>"Console"</span>
                    </span>
                </div>
                <h1 class="m-0 text-xl font-semibold">"Sign in to the console"</h1>
                <p class="mt-2 mb-6 text-sm text-muted">
                    "Use your organisation's account to see the state of the federation."
                </p>
                // The sign-in route is the server's, so the client router must not
                // take the click once the page has hydrated.
                <a href=SIGN_IN rel="external" class=format!("{} w-full", styles::BUTTON_PRIMARY)>
                    {icon(icondata_lu::LuLogIn, "16")}
                    "Sign in"
                </a>
                <p class="mt-6 mb-0 text-xs text-muted">"The console holds no clinical data."</p>
            </div>
        </div>
    }
    .to_html()
}

/// The subtitle of the dashboard.
const DASHBOARD: &str = "The state of the federation, as this console reads it.";

/// The dashboard a signed-in operator lands on: the counts of the
/// federation, each endpoint's health and the recent incidents, rendered on
/// the server and read through [`crate::views::load::dashboard`].
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn DashboardPage() -> impl IntoView {
    let loaded = Resource::new_blocking(|| (), |()| crate::views::load::dashboard());
    view! {
        <Title text=titled("Dashboard") />
        {page_header("Dashboard", DASHBOARD, ())}
        <Suspense fallback=|| {
            view! { <p>"Loading the dashboard."</p> }
        }>
            {move || Suspend::new(async move {
                crate::views::shown(
                    loaded.await,
                    |html| view! { <div class="contents" inner_html=html></div> }.into_any(),
                )
            })}
        </Suspense>
    }
}
