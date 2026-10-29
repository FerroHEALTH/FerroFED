// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The operator's preferences, each kept in a cookie of its own.
//!
//! The theme, the density, reduced motion, the language, the first day of
//! the week and the shortcut hints are FerroUI's (family rule 9); the folded
//! navigation is the console's.
//!
//! The server reads the cookies as it renders (`crate::visit`), so the first
//! paint is in the chosen theme and density with no flash, and writes them
//! on `<html>` as `data-theme`, `data-density`, `data-motion` and
//! `data-hints`. Every preference is a form post to [`PREFERENCE`], which
//! FerroUI's form navigator posts in the page, which keeps the one
//! value it names in its cookie and sends the browser back to the page it
//! came from; the user menu's theme choice posts there too. Reset to
//! defaults posts to [`RESET`], which clears every preference cookie. The
//! cookies are `HttpOnly`, `SameSite=Lax`, `Secure` where the console's
//! cookies are, live one year and hold no patient data; the console keeps
//! no user store and nothing in browser storage. No specification governs
//! this: our own design.

#[cfg(not(target_arch = "wasm32"))]
use crate::locale::Msg;
#[cfg(not(target_arch = "wasm32"))]
use crate::server::plain;
#[cfg(not(target_arch = "wasm32"))]
use axum::Extension;
#[cfg(not(target_arch = "wasm32"))]
use axum::extract::Form;
#[cfg(not(target_arch = "wasm32"))]
use axum::response::{IntoResponse, Redirect, Response};
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::theme::Preferences;
use ferrohealth_ui::theme::{Density, Hints, Language, Motion, Preference, Theme, WeekStart};
#[cfg(not(target_arch = "wasm32"))]
use http::header::SET_COOKIE;
#[cfg(not(target_arch = "wasm32"))]
use http::{HeaderValue, StatusCode};

#[cfg(not(target_arch = "wasm32"))]
use crate::server::ConsoleState;

/// The route every preference posts to, its value under its cookie's name
/// beside `back`.
pub const PREFERENCE: &str = "/preference";

/// The route Reset to defaults posts to.
pub const RESET: &str = "/preferences/reset";

/// The route the navigation's Collapse and Expand post to.
pub const SIDEBAR: &str = "/sidebar";

/// The path of Your preferences, its Appearance tab.
pub const PREFERENCES: &str = "/preferences";

/// The page `back` names when it is a plain path of the console.
///
/// A plain path is visible ASCII with no query, fragment, backslash or `..`
/// segment; anything else is `/`, so a post never redirects off the console
/// (OWASP, Unvalidated Redirects and Forwards).
#[must_use]
pub fn return_path(back: &str) -> &str {
    let plain = back.starts_with('/')
        && !back.starts_with("//")
        && back
            .chars()
            .all(|c| c.is_ascii_graphic() && !matches!(c, '\\' | '?' | '#'))
        && !back.split('/').any(|segment| segment == "..");
    if plain { back } else { "/" }
}

/// The value of the preference `name` as its cookie keeps it, when `value`
/// is one the console offers for it, and `None` for anything else.
#[must_use]
pub fn preference_value(name: &str, value: &str) -> Option<String> {
    let kept = |choice: Option<&str>| choice.map(str::to_owned);
    if name == Theme::COOKIE {
        kept(Theme::from_cookie(value).map(Theme::key))
    } else if name == Density::COOKIE {
        kept(Density::from_cookie(value).map(Density::key))
    } else if name == Motion::COOKIE {
        kept(Motion::from_cookie(value).map(Motion::key))
    } else if name == WeekStart::COOKIE {
        kept(WeekStart::from_cookie(value).map(WeekStart::key))
    } else if name == Hints::COOKIE {
        kept(Hints::from_cookie(value).map(Hints::key))
    } else if name == Language::COOKIE {
        let offered = |tag: &str| {
            crate::locale::choices()
                .iter()
                .any(|locale| locale.tag() == tag)
        };
        Language::from_cookie(value)
            .filter(|language| language.tag().is_none_or(offered))
            .map(|language| language.cookie_value().to_owned())
    } else {
        None
    }
}

/// Where a preference post returns: the page `back` names, and on Your
/// preferences the same page saying which preference it kept.
#[must_use]
pub fn preference_return(back: &str, name: &str) -> String {
    let page = return_path(back);
    if page.starts_with(PREFERENCES) {
        format!("{page}?saved={name}")
    } else {
        page.to_owned()
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Keeps the one preference a plain form post chose in its cookie, and
/// sends the browser back to the page it came from.
///
/// The post carries `back` and one preference under its cookie's name; a
/// switch posts `off` before its `on`, and the last value wins.
pub async fn choose(
    Extension(state): Extension<ConsoleState>,
    headers: http::HeaderMap,
    Form(fields): Form<Vec<(String, String)>>,
) -> Response {
    let back = fields
        .iter()
        .rev()
        .find(|(name, _)| name == "back")
        .map_or("/", |(_, back)| back.as_str());
    let chosen = fields
        .iter()
        .filter(|(name, _)| name != "back")
        .collect::<Vec<_>>();
    let Some((name, value)) = chosen.last() else {
        return plain(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::AnswerNoPreference,
        );
    };
    if chosen.iter().any(|(other, _)| other != name) {
        return plain(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::AnswerOnePreference,
        );
    }
    let Some(value) = preference_value(name, value) else {
        return plain(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::AnswerPreferenceUnknown,
        );
    };
    let cookie = state.sessions().preference_cookie(name, &value).to_string();
    redirect_with(&preference_return(back, name), &[cookie])
}

#[cfg(not(target_arch = "wasm32"))]
/// What Reset to defaults posts.
#[derive(Debug, serde::Deserialize)]
pub struct Reset {
    /// The page to return to.
    back: String,
}

#[cfg(not(target_arch = "wasm32"))]
/// Clears every preference cookie and sends the browser back to the page
/// it came from.
pub async fn reset(
    Extension(state): Extension<ConsoleState>,
    Form(reset): Form<Reset>,
) -> Response {
    let cleared = Preferences::COOKIES
        .into_iter()
        .map(|name| state.sessions().preference_cookie_removal(name).to_string())
        .collect::<Vec<_>>();
    redirect_with(return_path(&reset.back), &cleared)
}

#[cfg(not(target_arch = "wasm32"))]
/// What the navigation's Collapse or Expand posts.
#[derive(Debug, serde::Deserialize)]
pub struct SidebarChoice {
    /// `rail` to fold the navigation, `full` to unfold it.
    sidebar: String,
    /// The page to return to.
    back: String,
}

#[cfg(not(target_arch = "wasm32"))]
/// Keeps whether the navigation is folded in its cookie and sends the
/// browser back to the page it came from.
pub async fn sidebar(
    Extension(state): Extension<ConsoleState>,
    headers: http::HeaderMap,
    Form(choice): Form<SidebarChoice>,
) -> Response {
    if !matches!(choice.sidebar.as_str(), "rail" | "full") {
        return plain(
            &state,
            &headers,
            StatusCode::BAD_REQUEST,
            Msg::AnswerSidebarUnknown,
        );
    }
    let cookie = state.sessions().sidebar_cookie(&choice.sidebar).to_string();
    redirect_with(return_path(&choice.back), &[cookie])
}

#[cfg(not(target_arch = "wasm32"))]
/// The `303` redirect to `location` that sets each of `cookies`.
#[must_use]
pub fn redirect_with(location: &str, cookies: &[String]) -> Response {
    let mut response = Redirect::to(location).into_response();
    for cookie in cookies {
        match HeaderValue::from_str(cookie) {
            Ok(value) => {
                response.headers_mut().append(SET_COOKIE, value);
            }
            Err(_invalid) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::{preference_return, preference_value, return_path};

    #[test]
    fn a_preference_post_keeps_only_a_value_the_console_offers() {
        assert_eq!(
            Some(String::from("dark")),
            preference_value("theme", "dark")
        );
        assert_eq!(None, preference_value("theme", "sepia"));
        assert_eq!(
            Some(String::from("compact")),
            preference_value("density", "compact")
        );
        assert_eq!(
            Some(String::from("reduce")),
            preference_value("motion", "reduce")
        );
        assert_eq!(
            Some(String::from("sunday")),
            preference_value("week-start", "sunday")
        );
        assert_eq!(
            Some(String::from("never")),
            preference_value("hints", "never")
        );
        assert_eq!(
            Some(String::from("browser")),
            preference_value("language", "browser")
        );
        assert_eq!(Some(String::from("en")), preference_value("language", "en"));
        assert_eq!(
            None,
            preference_value("language", "nl"),
            "a language the console does not ship"
        );
        assert_eq!(None, preference_value("sidebar", "rail"), "no preference");
    }

    #[test]
    fn a_preference_post_says_what_it_kept_only_on_your_preferences() {
        assert_eq!(
            "/preferences/keyboard?saved=hints",
            preference_return("/preferences/keyboard", "hints")
        );
        assert_eq!(
            "/organisations",
            preference_return("/organisations", "theme")
        );
        assert_eq!(
            "/",
            preference_return("//evil.example/preferences", "theme")
        );
    }

    #[test]
    fn a_return_path_never_leaves_the_console() {
        assert_eq!("/stored-queries", return_path("/stored-queries"));
        for hostile in [
            "//evil.example",
            "https://evil.example",
            "/a/../b",
            "/a?b",
            "",
            "/a b",
            "/\\evil.example",
        ] {
            assert_eq!("/", return_path(hostile), "{hostile}");
        }
    }
}
