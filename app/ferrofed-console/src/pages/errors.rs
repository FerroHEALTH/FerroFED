// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error pages (FerroUI R20).
//!
//! An address the console does not serve (404), an operator the gateway's
//! operator surface refuses (403), and a session store the console cannot
//! read (500), each with its status in words before its number, what it
//! means and the way on.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::error_page::{ErrorPage, Status, Way, error_page};
use ferrohealth_ui::view::shell::centred;
use ferrohealth_ui::view::toast::toast_region;

use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, set_status, t};

/// The page for an address the console does not serve.
#[must_use]
pub fn not_found() -> Html {
    set_status(http::StatusCode::NOT_FOUND);
    let here = crate::pages::here();
    let page = error_page(
        &current(),
        ErrorPage {
        status: Status::NotFound,
        title: tr(Msg::NotFoundTitle),
        body: h(format!(
            "<p>{}</p>",
            t(tr_with(Msg::NotFoundBody, &[("path", here.into())]))
        )),
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
        },
    );
    crate::app::title(tr(Msg::NotFoundTitle)).then(page)
}

/// The page for an operator whose token carries no operator scope: the
/// gateway's operator surface answered `403`.
#[must_use]
pub fn forbidden() -> Html {
    set_status(http::StatusCode::FORBIDDEN);
    let page = error_page(
        &current(),
        ErrorPage {
        status: Status::Forbidden,
        title: tr(Msg::ForbiddenTitle),
        body: h(format!("<p>{}</p>", t(tr(Msg::ForbiddenBody)))),
        hint: Some(h(format!(
            "<p class=\"fui-small fui-muted\">{}</p>",
            t(tr(Msg::ForbiddenHint))
        ))),
        ways: vec![Way {
            label: tr(Msg::SignInAgain),
            href: String::from(crate::app::SIGN_IN),
            glyph: Some(Glyph::LOG_IN),
        }],
        detail: None,
        },
    );
    crate::app::title(tr(Msg::ForbiddenTitle)).then(page)
}

/// The page a visitor reads when the console cannot read its session store:
/// it cannot tell whether they are signed in, so it offers neither the
/// sign-in nor the pages, and answers `500`.
#[must_use]
pub fn unreadable() -> Html {
    set_status(http::StatusCode::INTERNAL_SERVER_ERROR);
    let locale = current();
    let page = error_page(
        &locale,
        ErrorPage {
        status: Status::Fault,
        title: tr(Msg::UnreadableTitle),
        body: h(format!("<p>{}</p>", t(tr(Msg::UnreadableBody)))),
        hint: None,
        ways: vec![Way {
            label: tr(Msg::TryAgain),
            href: String::from("/"),
            glyph: Some(Glyph::REFRESH_CW),
        }],
        detail: None,
        },
    );
    crate::app::title(tr(Msg::UnreadableTitle)).then(centred(
        &locale,
        page,
        toast_region(&locale, Vec::new()),
        crate::shell::footer(),
    ))
}
