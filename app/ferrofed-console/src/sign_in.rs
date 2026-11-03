// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pages the console draws around `ferrohealth-session`'s sign-in.
//!
//! A stopped sign-in is answered in plain text in the request's locale, and
//! the sign-in and the sign-out land on the console's own pages. No
//! specification governs the console: our own design.

use axum::response::{IntoResponse as _, Response};
use ferrohealth_session::paths::{LOGOUT, is_own};
use ferrohealth_session::routes::{Pages, Stopped};
use ferrohealth_smart::oidc::OidcError;
use ferrohealth_ui::i18n::Locale;
use http::header::CACHE_CONTROL;
use http::request::Parts;

use crate::locale::{Msg, requested, tr_in};

/// The console's [`Pages`]: a stopped sign-in answered in plain text from
/// the catalog, in the locale the request asks for.
#[derive(Debug, Clone)]
pub struct SignInPages {
    /// The locale a request falls back to.
    locale: Locale,
    /// The name every preference cookie starts with, the language among
    /// them.
    prefix: String,
}

impl SignInPages {
    /// The pages of a console whose fallback locale is `locale` and whose
    /// preference cookies start with `prefix`.
    #[must_use]
    pub const fn new(locale: Locale, prefix: String) -> Self {
        Self { locale, prefix }
    }

    /// The locale of the request `parts`.
    fn locale(&self, parts: &Parts) -> Locale {
        requested(&parts.headers, &self.prefix, self.locale.clone())
    }
}

impl Pages for SignInPages {
    fn stopped(&self, parts: Parts, why: Stopped) -> impl Future<Output = Response> + Send {
        let msg = match &why {
            Stopped::NotConfigured => Msg::AnswerSignInNotConfigured,
            Stopped::NoSignInPending => Msg::AnswerNoSignInPending,
            Stopped::StateMismatch => Msg::AnswerStateMismatch,
            Stopped::NoCode => Msg::AnswerNoCode,
            Stopped::Rejected(OidcError::Authorization { .. }) => Msg::AnswerProviderRefused,
            Stopped::Rejected(_) => Msg::AnswerIdTokenUnverified,
            Stopped::NotCompleted(_) => Msg::AnswerExchangeFailed,
            Stopped::Full => Msg::AnswerSessionsFull,
            Stopped::Store(_) if parts.uri.path() == LOGOUT => Msg::AnswerSessionNotEnded,
            Stopped::Store(_) if is_own(parts.uri.path()) => Msg::AnswerSignInSessionNotBegun,
            _ => Msg::AnswerSessionUnreadable,
        };
        let text = tr_in(&self.locale(&parts), msg, &[]);
        std::future::ready((why.status(), [(CACHE_CONTROL, "no-store")], text).into_response())
    }

    fn session_gone(&self, parts: &Parts) -> String {
        tr_in(&self.locale(parts), Msg::AnswerSessionGone, &[])
    }

    fn sign_in_page(&self) -> &'static str {
        "/"
    }

    fn signed_out(&self) -> &'static str {
        "/"
    }

    fn home(&self) -> &'static str {
        "/"
    }
}
