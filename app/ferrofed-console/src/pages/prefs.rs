// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Your preferences, on FerroUI's three tabs.
//!
//! Appearance holds the theme and the density as preview cards and reduced
//! motion, Language and region the language, each in its own name, and the
//! first day of the week, and Keyboard the shortcut hints.
//!
//! Each is kept in a cookie of its own, only for this operator in this browser
//! (family rule 9); every choice is a plain form post to
//! [`crate::prefs::PREFERENCE`], which returns here saying what it kept.

use ferrohealth_ui::calendar::Weekday;
use ferrohealth_ui::view::preferences::{
    PreferencesPage, Section, SectionRoutes, preferences_page,
};
use ferrohealth_ui::html::Html;

use crate::locale::{Msg, tr};
use crate::prefs::{PREFERENCE, PREFERENCES, RESET};

/// The tab the route's `section` names, `None` for one that names none.
fn section(name: &str) -> Option<Section> {
    match name {
        "" | "appearance" => Some(Section::Appearance),
        "language" => Some(Section::Language),
        "keyboard" => Some(Section::Keyboard),
        _ => None,
    }
}

/// Your preferences, on the tab the route names.
#[must_use]
pub fn page() -> Html {
    let Some(section) = section(&crate::pages::param("section")) else {
        return crate::pages::errors::not_found();
    };
    let routes = SectionRoutes {
        appearance: String::from(PREFERENCES),
        language: format!("{PREFERENCES}/language"),
        keyboard: format!("{PREFERENCES}/keyboard"),
    };
    let saved = crate::pages::query_param("saved");
    let visit = crate::visit::visit();
    let page = preferences_page(
        &visit.locale,
        PreferencesPage {
        section,
        routes,
        reading: false,
        preferences: visit.chosen().preferences,
        action: String::from(PREFERENCE),
        saved,
        accent: tr(Msg::AccentAzure),
        languages: crate::locale::choices(),
        browser: visit.browser.clone(),
        week_from_language: Weekday::Monday,
        shortcuts: None,
        reset: String::from(RESET),
        own: Vec::new(),
        },
    );
    crate::app::title(tr(Msg::PreferencesTitle)).then(page)
}
