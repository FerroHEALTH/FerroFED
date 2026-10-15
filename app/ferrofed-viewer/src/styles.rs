// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's design vocabulary, as the class strings the shell paints
//! with.
//!
//! A class names a role, a type step or a spacing step of
//! `style/tailwind.css`, never a palette entry or a raw size, so the palette
//! is measured once for WCAG 2.2 AA and a theme is one block of that
//! stylesheet. The views write plain semantic HTML, which the stylesheet's
//! base layer draws; the classes here are the shell's and the few marks a
//! view adds to it.

/// The page ground and the text on it.
pub const PAGE: &str = "flex min-h-screen flex-col bg-surface text-fg";

/// The bar across the top of every page.
pub const TOPBAR: &str = "border-b border-line bg-raised";

/// What the top bar holds: the lockup, and the sign-out at the far end.
pub const TOPBAR_ROW: &str = "flex flex-wrap items-center gap-default px-loose py-default";

/// The product name beside the mark.
pub const LOCKUP_NAME: &str = "text-title font-semibold text-fg";

/// A label in small capitals, for a thing that is not a heading.
pub const EYEBROW: &str = "text-micro font-semibold tracking-wide text-muted uppercase";

/// The navigation and the page beside it, stacked on a narrow screen.
pub const BODY: &str = "flex flex-1 flex-col md:flex-row";

/// The navigation: a band that wraps on a narrow screen, a column beside the
/// page on a wide one.
pub const SIDEBAR: &str = "shrink-0 border-b border-line bg-raised p-default md:w-56 md:border-r \
                           md:border-b-0";

/// The list of navigation entries.
pub const NAV_LIST: &str = "flex flex-row flex-wrap gap-tight md:flex-col";

/// A navigation entry.
pub const NAV_ENTRY: &str = "state-change block rounded-md px-default py-tight text-small \
                             font-medium text-fg no-underline hover:bg-inset";

/// The page itself. It never scrolls sideways: a table wider than the screen
/// scrolls in its own [`TABLE_SCROLL`] box, so anything else that overflows
/// widens the document, where a reflow check sees it.
pub const MAIN: &str = "min-w-0 flex-1 px-loose py-loose";

/// The column the page reads in.
pub const COLUMN: &str = "mx-auto max-w-6xl";

/// A control that acts without standing out: the sign-out.
pub const BUTTON_QUIET: &str = "state-change border border-control bg-raised text-fg \
                                hover:bg-inset";

/// The box a table scrolls sideways in, so the page never does; it takes the
/// keyboard focus, so a keyboard can scroll it.
pub const TABLE_SCROLL: &str = "table-scroll";

/// A table cell holding a number, aligned on the right.
pub const NUM: &str = "num";

/// The brand mark, which the stylesheet draws for the theme.
pub const MARK: &str = "brand-mark";

/// The link the mark and the product name make together.
pub const LOCKUP: &str = "flex items-center gap-default no-underline";

/// The product name and the console's name beside it.
pub const LOCKUP_TEXT: &str = "flex flex-wrap items-baseline gap-x-tight";

/// The classes of the state word `word` a gateway reports for a member or a
/// dependency: the tint repeats what the word says, and a word with no tint
/// of its own reads in the neutral one.
#[must_use]
pub fn state(word: &str) -> &'static str {
    match word {
        "up" => "state state-up",
        "failing" | "down" => "state state-failing",
        "unknown" | "degraded" => "state state-unknown",
        _ => "state",
    }
}

#[cfg(test)]
mod tests {
    use super::state;

    #[test]
    fn every_state_the_gateway_reports_has_its_tint_and_any_other_word_the_neutral_one() {
        assert_eq!("state state-up", state("up"));
        assert_eq!("state state-failing", state("failing"));
        assert_eq!("state state-failing", state("down"));
        assert_eq!("state state-unknown", state("unknown"));
        assert_eq!("state state-unknown", state("degraded"));
        assert_eq!("state", state("registry-invalid"));
        assert_eq!("state", state("not reported"));
    }
}
