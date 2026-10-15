// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The light, dark and system themes, and the server function that keeps
//! the operator's choice.
//!
//! The choice lives in a cookie the server reads while it renders, and the
//! server writes it as `data-theme` on `<html>`, so a page arrives in the
//! theme it is read in, before any script runs, and the browser keeps no
//! storage of its own (the Leptos book, `ssr/22_life_cycle`). `system`
//! follows the operating system through `prefers-color-scheme`, which the
//! stylesheet reads. No specification governs this: our own design.

use leptos::prelude::*;

use crate::views::model::{Outcome, ViewError};

/// A theme the operator can choose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Theme {
    /// The light theme.
    Light,
    /// The dark theme.
    Dark,
    /// The theme the operating system asks for.
    #[default]
    System,
}

impl Theme {
    /// Every theme, in the order the switch offers them.
    pub const ALL: [Self; 3] = [Self::Light, Self::Dark, Self::System];

    /// The theme's key, as the cookie and `data-theme` carry it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::System => "system",
        }
    }

    /// The theme's name, as the switch shows it.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::System => "System",
        }
    }

    /// The theme `key` names, or `None` for a key that names none.
    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.key() == key)
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.key())
    }
}

impl std::str::FromStr for Theme {
    type Err = UnknownTheme;

    fn from_str(key: &str) -> Result<Self, Self::Err> {
        Self::parse(key).ok_or(UnknownTheme)
    }
}

/// A key that names no [`Theme`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the key names no theme of the console")]
pub struct UnknownTheme;

/// Keeps the signed-in operator's theme choice `theme` in its cookie.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and
/// [`ViewError::Unavailable`] when the console cannot set the cookie; a key
/// that names no theme is an [`Outcome::Refused`].
#[cfg_attr(
    not(target_arch = "wasm32"),
    allow(
        clippy::unused_async,
        reason = "a server function is async on both halves, and its server half sets a cookie and awaits nothing (https://docs.rs/server_fn/0.8/server_fn/attr.server.html)"
    )
)]
#[server(endpoint = "theme")]
pub async fn set_theme(
    /// The key of the theme chosen.
    theme: String,
) -> Result<Outcome<()>, ViewError> {
    server::keep(&theme)
}

/// The server half: the cookie the choice is kept in, and the choice a
/// request carries.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use http::HeaderValue;
    use http::header::SET_COOKIE;
    use leptos::context::use_context;

    use crate::server::ViewerState;
    use crate::session::THEME_COOKIE;
    use crate::theme::Theme;
    use crate::views::model::{Outcome, Refusal, ViewError};

    /// Sets the cookie of the theme `key` names on the answer, for a request
    /// with a live signed-in session.
    ///
    /// # Errors
    /// As [`crate::theme::set_theme`].
    pub fn keep(key: &str) -> Result<Outcome<()>, ViewError> {
        let (state, _token) = crate::views::load::server::signed_in()?;
        let Some(theme) = Theme::parse(key) else {
            return Ok(Outcome::Refused(Refusal::Invalid {
                reason: String::from("the theme is not light, dark or system"),
            }));
        };
        let response =
            use_context::<leptos_axum::ResponseOptions>().ok_or(ViewError::Unavailable)?;
        let cookie = state.sessions().theme_cookie(theme.key()).to_string();
        let value = HeaderValue::from_str(&cookie).map_err(|_invalid| ViewError::Unavailable)?;
        response.append_header(SET_COOKIE, value);
        Ok(Outcome::Shown(()))
    }

    /// The theme the request's cookie names, and the system theme for a
    /// request with none or one that names no theme.
    #[must_use]
    pub fn requested() -> Theme {
        let Some(state) = use_context::<ViewerState>() else {
            return Theme::default();
        };
        let Some(parts) = use_context::<http::request::Parts>() else {
            return Theme::default();
        };
        // NOTE: no specification governs this: our own design; a cookie that
        // names no theme is the choice not made, as a missing one is.
        crate::oidc::cookie_named(&parts.headers, &state.sessions().cookie_name(THEME_COOKIE))
            .and_then(|value| Theme::parse(value.as_str()))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn every_theme_reads_back_from_its_key_and_no_other_key_names_one() {
        for theme in Theme::ALL {
            assert_eq!(Some(theme), Theme::parse(theme.key()));
            assert_eq!(Ok(theme), theme.to_string().parse());
        }
        assert_eq!(None, Theme::parse("Dark"));
        assert_eq!(None, Theme::parse("sepia"));
        assert_eq!(Theme::System, Theme::default());
    }
}
