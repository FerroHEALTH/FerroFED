// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The chrome around every page: the top bar with the brand lockup and the
//! sign-out, and the navigation beside the page, both drawn only for a
//! signed-in operator.
//!
//! Each section returns its typed view rather than an erased one: the shell
//! is shallow, and each erasure costs the browser bundle its own rendering
//! code, which the budget of the bundle has no room for.

use leptos::prelude::*;

use crate::app::SIGN_OUT;
use crate::styles;

/// The top bar: the lockup, which leads to the landing page, and for a
/// signed-in operator the sign-out at the far end.
#[must_use]
pub fn topbar(signed_in: bool) -> impl IntoView {
    let sign_out = signed_in.then(|| {
        view! {
            // The sign-out route is the server's: a plain form post it
            // answers with a redirect, which no client router takes.
            <form method="post" action=SIGN_OUT class="ml-auto">
                <button type="submit" class=styles::BUTTON_QUIET>
                    "Sign out"
                </button>
            </form>
        }
    });
    view! {
        <header class=styles::TOPBAR>
            <div class=styles::TOPBAR_ROW>
                <a href="/" class=styles::LOCKUP>
                    // The brand mark is the stylesheet's, drawn for the theme; it is
                    // decoration, and the product name stands beside it.
                    <span class=styles::MARK aria-hidden="true"></span>
                    <span class=styles::LOCKUP_TEXT>
                        <span class=styles::LOCKUP_NAME>"FerroFED"</span>
                        <span class=styles::EYEBROW>"operator console"</span>
                    </span>
                </a>
                {sign_out}
            </div>
        </header>
    }
}

/// The navigation a signed-in operator reads: a link to every view, and to
/// the configuration page when a replica's admin listener is configured.
#[must_use]
pub fn sidebar(administered: bool) -> impl IntoView {
    let configuration = administered.then(|| {
        view! {
            <li>
                <a href=crate::configure::CONFIGURATION class=styles::NAV_ENTRY>
                    "Configuration"
                </a>
            </li>
        }
    });
    view! {
        <nav aria-label="Views" class=styles::SIDEBAR>
            <ul class=styles::NAV_LIST>
                <li>
                    <a href="/" class=styles::NAV_ENTRY>
                        "Console"
                    </a>
                </li>
                <li>
                    <a href=crate::views::MEMBERS class=styles::NAV_ENTRY>
                        "Members"
                    </a>
                </li>
                <li>
                    <a href=crate::views::INTEGRITY class=styles::NAV_ENTRY>
                        "Integrity"
                    </a>
                </li>
                <li>
                    <a href=crate::views::STORED_QUERIES class=styles::NAV_ENTRY>
                        "Stored queries"
                    </a>
                </li>
                <li>
                    <a href=crate::views::FEDERATION class=styles::NAV_ENTRY>
                        "Self-description"
                    </a>
                </li>
                <li>
                    <a href=crate::query::QUERY class=styles::NAV_ENTRY>
                        "Query"
                    </a>
                </li>
                {configuration}
            </ul>
        </nav>
    }
}
