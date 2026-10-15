// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The page at `/`: the sign-in card a visitor reads, and the dashboard a
//! signed-in operator lands on.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::app::SIGN_IN;
use crate::components::{icon, page_header};
use crate::styles;
use crate::views::titled;

/// The sign-in: a card with the mark centred on the page, as the family's
/// viewers draw it.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn SignInPage() -> impl IntoView {
    view! {
        <Title text=titled("Sign in") />
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
}

/// One place the dashboard leads to.
#[derive(Clone, Copy)]
struct Place {
    /// Where it leads.
    href: &'static str,
    /// What it is called.
    label: &'static str,
    /// What it shows, in one line.
    line: &'static str,
    /// The icon beside it.
    glyph: icondata_core::Icon,
}

/// The card of `place`, which leads to it.
fn place(place: Place) -> impl IntoView {
    view! {
        <a
            href=place.href
            class="state-change flex items-start gap-3 rounded-card border border-line bg-raised p-4 text-ink no-underline shadow-card hover:border-accent hover:text-ink"
        >
            <span class="flex size-10 shrink-0 items-center justify-center rounded-control bg-accent-tint text-accent">
                {icon(place.glyph, "20")}
            </span>
            <span class="min-w-0">
                <span class="block font-semibold">{place.label}</span>
                <span class="mt-0.5 block text-sm text-muted">{place.line}</span>
            </span>
        </a>
    }
}

/// The dashboard a signed-in operator lands on: the places of the console,
/// each with what it shows.
#[component]
pub fn DashboardPage() -> impl IntoView {
    // TODO(#1076): the stat cards, the member health and the recent incidents.
    let places = [
        Place {
            href: crate::views::MEMBERS,
            label: "Members",
            line: "Each member endpoint and its health.",
            glyph: icondata_lu::LuUsers,
        },
        Place {
            href: crate::views::INTEGRITY,
            label: "Integrity",
            line: "Incidents and the creating_system_id routing table.",
            glyph: icondata_lu::LuShieldCheck,
        },
        Place {
            href: crate::query::QUERY,
            label: "Query console",
            line: "Run AQL or a stored query across the federation.",
            glyph: icondata_lu::LuSquareTerminal,
        },
        Place {
            href: crate::views::STORED_QUERIES,
            label: "Stored queries",
            line: "The queries the gateway holds, by version.",
            glyph: icondata_lu::LuDatabase,
        },
        Place {
            href: crate::views::FEDERATION,
            label: "Self-description",
            line: "What the gateway declares of itself.",
            glyph: icondata_lu::LuServer,
        },
    ]
    .into_iter()
    .map(place)
    .collect_view();
    view! {
        <Title text=titled("Dashboard") />
        {page_header("Dashboard", "The state of the federation this console reads.", ())}
        <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-3">{places}</div>
    }
}
