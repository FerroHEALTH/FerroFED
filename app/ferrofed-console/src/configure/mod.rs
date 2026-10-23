// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration: the gateway's configuration revisions, the members,
//! credentials, binding sections and stored queries with their owners, and
//! the console's changes to the ones it owns.
//!
//! The page (T3) has five binder tabs, each at its own path under
//! `/configuration`: Members, Credentials and bindings, Stored queries,
//! History, and Import and export; a member's, a credential's, a binding
//! section's and a stored query's form is a page of its own under its tab.
//! Above the tabs sits the card of the revision saved and not applied,
//! with what it changes and its apply, and of the confirmation the running
//! revision awaits. A change is saved as a revision and applied
//! separately; an apply, a roll back, a removal and a stored version are
//! each confirmed in a step of their own.
//!
//! The page is read from the first replica's admin listener with the
//! operator's own access token ([`act`]) and drawn on the server, its forms
//! whole. Every form is a plain `POST` to the page it sits on, read on the
//! server into a [`model::ConfigForm`] and run while the page is drawn
//! again, so its answer shows at the top of the page with no WebAssembly;
//! a refusal of one field names that field. No specification governs the
//! console: our own design.

pub mod act;
pub mod draft;
pub mod export;
pub mod import;
pub mod member;
pub mod model;
pub mod plan;
pub mod render;
pub mod said;
pub mod sections;
pub mod settings;

use crate::locale::{Msg, tr};

/// The path of the configuration page.
pub const CONFIGURATION: &str = "/configuration";

/// `remaining` milliseconds in words, as the countdown of a revision that
/// awaits its confirmation shows them.
#[must_use]
pub fn countdown(remaining: i64) -> String {
    if remaining <= 0 {
        return tr(Msg::TheDeadlineHasPassed);
    }
    let seconds = remaining.div_euclid(1000);
    format!(
        "{}:{:02} left",
        seconds.div_euclid(60),
        seconds.rem_euclid(60)
    )
}

#[cfg(test)]
mod tests {
    use super::countdown;

    #[test]
    fn the_countdown_reads_as_minutes_and_seconds_and_says_when_it_has_passed() {
        assert_eq!("1:43 left", countdown(103_400));
        assert_eq!("0:05 left", countdown(5_999));
        assert_eq!("The deadline has passed", countdown(0));
        assert_eq!("The deadline has passed", countdown(-3));
    }
}
