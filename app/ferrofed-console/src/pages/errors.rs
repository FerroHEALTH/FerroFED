// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (FerroUI R20).
//!
//! An address the console does not serve (404), an operator the gateway's
//! operator surface refuses (403), and a session store the console cannot
//! read (500), each with its status in words before its number, what it
//! means and the way on.

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};
use ferrohealth_ui::view::shell::centred;
use leptos::prelude::*;

use crate::locale::{Msg, tr, tr_with};

/// Sets the answer's status to `status` unless an earlier part of the
/// render set a server error, which outranks it (RFC 9110 §15.6).
fn answer(status: http::StatusCode) {
    if let Some(response) = use_context::<leptos_axum::ResponseOptions>() {
        let failed = response
            .0
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .is_some_and(|set| set.is_server_error());
        if !failed {
            response.set_status(status);
        }
    }
}

/// The page for an address the console does not serve.
#[must_use]
pub fn not_found() -> AnyView {
    answer(http::StatusCode::NOT_FOUND);
    let here = crate::pages::here();
    let page = error_page(ErrorPage {
        status: Status::NotFound,
        title: tr(Msg::NotFoundTitle),
        body: view! { <p>{tr_with(Msg::NotFoundBody, &[("path", here.into())])}</p> }.into_any(),
        hint: None,
        ways: vec![
            Way {
                label: tr(Msg::NotFoundDashboard),
                href: String::from("/"),
                glyph: Some(Glyph::ARROW_LEFT),
            },
            Way {
                label: tr(Msg::NotFoundOrganisations),
                href: String::from(crate::app::ORGANISATIONS),
                glyph: None,
            },
        ],
        detail: None,
    });
    view! {
        {crate::app::title(tr(Msg::NotFoundTitle))}
        {page}
    }
    .into_any()
}

/// The page for an operator whose token carries no operator scope: the
/// gateway's operator surface answered `403`.
#[must_use]
pub fn forbidden() -> AnyView {
    answer(http::StatusCode::FORBIDDEN);
    let page = error_page(ErrorPage {
        status: Status::Forbidden,
        title: tr(Msg::ForbiddenTitle),
        body: view! { <p>{tr(Msg::ForbiddenBody)}</p> }.into_any(),
        hint: Some(
            view! { <p class="fui-small fui-muted">{tr(Msg::ForbiddenHint)}</p> }.into_any(),
        ),
        ways: vec![Way {
            label: tr(Msg::SignInAgain),
            href: String::from(crate::app::SIGN_IN),
            glyph: Some(Glyph::LOG_IN),
        }],
        detail: None,
    });
    view! {
        {crate::app::title(tr(Msg::ForbiddenTitle))}
        {page}
    }
    .into_any()
}

/// The page a visitor reads when the console cannot read its session store:
/// it cannot tell whether they are signed in, so it offers neither the
/// sign-in nor the pages, and answers `500`.
#[must_use]
pub fn unreadable() -> AnyView {
    answer(http::StatusCode::INTERNAL_SERVER_ERROR);
    let page = error_page(ErrorPage {
        status: Status::Fault,
        title: tr(Msg::UnreadableTitle),
        body: view! { <p>{tr(Msg::UnreadableBody)}</p> }.into_any(),
        hint: None,
        ways: vec![Way {
            label: tr(Msg::TryAgain),
            href: String::from("/"),
            glyph: Some(Glyph::REFRESH_CW),
        }],
        detail: None,
    });
    view! {
        {crate::app::title(tr(Msg::UnreadableTitle))}
        {centred(page, crate::shell::footer())}
    }
    .into_any()
}
