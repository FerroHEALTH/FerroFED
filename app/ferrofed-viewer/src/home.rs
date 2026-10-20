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

use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::sign_in::{Notice, Provider, SignIn};
use ferrohealth_ui::view::surface::page_header;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::SIGN_IN;
use crate::locale::{Msg, tr};
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
        <Title text=titled(&tr(Msg::SignIn)) />
        <div inner_html=html></div>
    }
}

/// The markup of the sign-in card.
#[must_use]
pub fn sign_in() -> String {
    // NOTE: OpenID Connect Core 1.0 §3.1: the console signs an operator in at
    // their provider alone, so the card offers the provider's button and no form.
    let card = ferrohealth_ui::view::sign_in::sign_in(SignIn {
        lockup: crate::shell::lockup().into_any(),
        notices: vec![Notice::new(
            Tone::Neutral,
            "Use your organisation's account to see the state of the federation. \
             The console holds no clinical data.",
        )],
        refusal: None,
        basic: None,
        provider: Some(Provider {
            href: String::from(SIGN_IN),
            label: tr(Msg::SignIn),
        }),
        links: Vec::new(),
    });
    view! {
        <div class="fui-centred">
            <h1 class="fui-sr-only">{tr(Msg::SignConsole)}</h1>
            {card}
        </div>
    }
    .to_html()
}

/// The subtitle of the dashboard.
fn dashboard_line() -> String {
    tr(Msg::StateFederationConsoleReads)
}

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
        <Title text=titled(&tr(Msg::Dashboard)) />
        {page_header(tr(Msg::Dashboard), Some(dashboard_line()), ())}
        <Suspense fallback=|| busy(tr(Msg::LoadingTheDashboard), Busy::Section)>
            {move || Suspend::new(async move {
                crate::views::shown(
                    loaded.await,
                    |html| view! { <div inner_html=html></div> }.into_any(),
                )
            })}
        </Suspense>
    }
}
