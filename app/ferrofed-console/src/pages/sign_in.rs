// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The sign-in a visitor reads (FerroUI R19): one provider button, since the
//! console signs an operator in at their OpenID Provider and never sees a
//! password (OpenID Connect Core 1.0 §3.1).
//!
//! A console with no provider configured shows the button disabled and says
//! who sets it up.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::shell::{brand_mark, centred, lockup_plain};
use ferrohealth_ui::view::sign_in::{Notice, Provider, SignIn, State, sign_in};
use leptos::prelude::*;

use crate::app::{MARK_DARK, MARK_LIGHT, SIGN_IN};
use crate::locale::{Msg, tr};

/// Whether the console is configured with an OpenID Provider.
fn configured() -> bool {
    use_context::<crate::server::ConsoleState>()
        .is_some_and(|state| state.settings().oidc.is_some())
}

/// The sign-in page.
#[must_use]
pub fn page() -> AnyView {
    let state = if configured() {
        State::Ready
    } else {
        crate::pages::set_status(http::StatusCode::SERVICE_UNAVAILABLE);
        State::Unavailable(Notice {
            detail: Some(tr(Msg::SignInUnavailableDetail)),
            ..Notice::new(tr(Msg::SignInUnavailable))
        })
    };
    let card = sign_in(SignIn {
        lockup: lockup_plain(
            brand_mark(MARK_LIGHT, MARK_DARK),
            "FerroFED",
            tr(Msg::LockupWord),
        )
        .into_any(),
        title: Some(tr(Msg::SignInTitle)),
        state,
        provider: Some(Provider {
            href: String::from(SIGN_IN),
            label: tr(Msg::SignInWith),
            busy: tr(Msg::SignInRedirecting),
            glyph: Glyph::LOG_IN,
        }),
        basic: None,
        line: None,
        note: Some(tr(Msg::SignInNote)),
        links: vec![(tr(Msg::SignInAbout), String::from(crate::app::ABOUT))],
        caption: Some(tr(Msg::SignInCaption)),
    })
    .into_any();
    view! {
        {crate::app::title(tr(Msg::SignInTitle))}
        {centred(card, crate::shell::footer())}
    }
    .into_any()
}
