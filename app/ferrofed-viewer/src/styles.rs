// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console's design vocabulary, as the class strings the shell and the
//! components paint with.
//!
//! A class names a role of `style/tailwind.css` (`surface`, `raised`, `ink`,
//! `muted`, `accent`, `line`) and a step of Tailwind's 4 px spacing scale,
//! never a palette entry, so the palette is measured once for WCAG 2.2 AA and
//! a theme is one block of that stylesheet. The views write plain semantic
//! HTML, which the stylesheet's base layer draws; the classes here are the
//! shell's, the components', and the few marks a view adds.

/// The page ground and the text on it.
pub const PAGE: &str = "flex min-h-screen flex-col bg-surface text-ink";

/// The bar across the top of every signed-in page, which stays in view from
/// `md` up.
pub const TOPBAR: &str = "border-b border-line bg-raised md:sticky md:top-0 md:z-30";

/// What the top bar holds: the drawer button, the lockup, and at the far end
/// the gateway, the theme switch and the operator.
///
/// From `md` up it is one row of the stylesheet's `--bar` height, which the
/// navigation and the scroll padding read.
pub const TOPBAR_ROW: &str = "flex min-h-14 flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2 \
                              md:h-[calc(var(--bar)-1px)] md:flex-nowrap md:px-6";

/// The far end of the top bar.
pub const TOPBAR_END: &str = "ml-auto flex flex-wrap items-center justify-end gap-2 md:flex-nowrap";

/// The link the mark and the product name make together.
pub const LOCKUP: &str = "flex items-center gap-2 text-ink no-underline hover:text-ink";

/// The product name beside the mark.
pub const LOCKUP_NAME: &str = "text-base font-semibold whitespace-nowrap";

/// The word of the product name the accent draws.
pub const LOCKUP_ACCENT: &str = "text-accent";

/// The brand mark, which the stylesheet draws for the theme.
pub const MARK: &str = "brand-mark";

/// The navigation and the page beside it.
pub const BODY: &str = "flex flex-1";

/// The navigation: a drawer below `md`, a column beside the page from it.
pub const SIDEBAR: &str = "drawer shrink-0 bg-raised p-3 text-ink";

/// The heading of a group of navigation entries.
pub const NAV_GROUP: &str = "px-3 pt-4 pb-1 text-xs font-semibold tracking-wide text-muted \
                             uppercase first:pt-1";

/// A list of navigation entries.
pub const NAV_LIST: &str = "flex flex-col gap-0.5";

/// A navigation entry: while it is the page, tinted with the accent, set in
/// bold and marked by a bar at its start, so colour is never the only mark.
pub const NAV_ENTRY: &str = "state-change flex items-center gap-3 rounded-control px-3 py-2 \
                             font-medium text-muted no-underline hover:bg-inset hover:text-ink \
                             aria-[current=page]:bg-accent-tint aria-[current=page]:text-accent \
                             aria-[current=page]:font-semibold \
                             aria-[current=page]:shadow-[inset_3px_0_0_var(--accent)]";

/// The page itself. It never scrolls sideways: a table wider than the screen
/// scrolls in its own [`TABLE_SCROLL`] box, so anything else that overflows
/// widens the document, where a reflow check sees it.
pub const MAIN: &str = "min-w-0 flex-1 px-4 py-6 md:px-8";

/// The column the page reads in.
pub const COLUMN: &str = "mx-auto max-w-7xl";

/// The one line at the foot of every page.
pub const FOOTER: &str = "border-t border-line bg-raised px-4 py-3 text-xs text-muted md:px-6";

/// A page that centres one card or one empty state: the sign-in, a page the
/// console does not serve, a fault.
pub const CENTRED: &str = "flex flex-1 items-center justify-center px-4 py-12";

/// The line under an empty state's title.
pub const EMPTY_BODY: &str = "mt-2 mb-6 text-sm text-muted";

/// A card: a raised surface with a hairline edge and a soft shadow.
pub const CARD: &str = "rounded-card border border-line bg-raised shadow-card";

/// A card with its standard padding.
pub const CARD_PAD: &str = "rounded-card border border-line bg-raised p-5 shadow-card";

/// The heading of a card.
pub const CARD_TITLE: &str = "mt-0 mb-3 text-sm font-semibold text-ink";

/// A grid of cards that reflows to one column on a narrow screen.
pub const GRID: &str = "grid grid-cols-1 gap-4 sm:grid-cols-2 xl:grid-cols-4";

/// The header that opens every page: its title and one subtitle line.
pub const PAGE_HEADER: &str = "mb-6 flex flex-wrap items-start justify-between gap-3";

/// The title of a page.
pub const PAGE_TITLE: &str = "m-0 text-2xl font-semibold text-ink";

/// The one line under a page's title.
pub const PAGE_SUBTITLE: &str = "mt-1 mb-0 text-sm text-muted";

/// The one action a screen is for.
pub const BUTTON_PRIMARY: &str = "state-change inline-flex items-center justify-center gap-2 \
                                  rounded-control px-3.5 py-2 text-sm font-medium no-underline \
                                  bg-accent text-accent-fg hover:bg-accent-hover \
                                  hover:text-accent-fg";

/// An action beside the primary one.
pub const BUTTON_SECONDARY: &str = "state-change inline-flex items-center justify-center gap-2 \
                                    rounded-control px-3.5 py-2 text-sm font-medium \
                                    no-underline border border-line-strong bg-raised text-ink \
                                    hover:bg-inset hover:text-ink";

/// An action that removes or ends something.
pub const BUTTON_DANGER: &str = "state-change inline-flex items-center justify-center gap-2 \
                                 rounded-control px-3.5 py-2 text-sm font-medium no-underline \
                                 border border-danger-fg bg-raised text-danger-fg \
                                 hover:bg-danger-bg hover:text-danger-fg";

/// An action that does not stand out: a menu entry, a quiet toggle.
pub const BUTTON_GHOST: &str = "state-change inline-flex items-center justify-center gap-2 \
                                rounded-control px-3.5 py-2 text-sm font-medium no-underline \
                                bg-transparent text-muted hover:bg-inset hover:text-ink";

/// The theme switch: three buttons in one segmented control.
pub const SEGMENTED: &str = "inline-flex items-center gap-0.5 rounded-control border \
                             border-line bg-surface p-0.5";

/// One button of the theme switch: while it is the theme in force, raised,
/// edged at 3:1 against the switch's ground, and checked.
pub const SEGMENT: &str = "state-change inline-flex items-center gap-1.5 rounded-[0.25rem] \
                           border border-transparent bg-transparent px-2 py-1 text-xs \
                           font-medium text-muted \
                           hover:bg-inset hover:text-ink aria-pressed:bg-raised \
                           aria-pressed:border-control aria-pressed:text-ink \
                           aria-pressed:shadow-card";

/// The operator's menu button in the top bar.
pub const MENU_BUTTON: &str = "state-change inline-flex items-center gap-2 rounded-control \
                               border border-line bg-raised px-2.5 py-1.5 text-sm font-medium \
                               text-ink hover:bg-inset";

/// The button that opens the navigation drawer, below `md` alone.
pub const DRAWER_BUTTON: &str = "state-change inline-flex items-center gap-2 rounded-control \
                                 border border-line bg-raised px-2.5 py-1.5 text-sm \
                                 font-medium text-ink hover:bg-inset md:hidden";

/// The operator's menu: a card that opens under the top bar.
pub const MENU: &str = "menu rounded-card border border-line bg-raised p-3 text-ink \
                        shadow-card";

/// The tabs of a page or a card.
pub const TABS: &str = "mb-4 flex gap-1 overflow-x-auto border-b border-line";

/// One tab, underlined in the accent while it is the one shown.
pub const TAB: &str = "state-change -mb-px border-b-2 border-transparent px-3 py-2 text-sm \
                       font-medium whitespace-nowrap text-muted no-underline hover:text-ink \
                       aria-[current=page]:border-accent aria-[current=page]:text-accent \
                       aria-selected:border-accent aria-selected:text-accent";

/// A form row: the label, the field and its one line of help.
pub const FIELD: &str = "mb-4 flex flex-col gap-1";

/// The one line of help under a field.
pub const HELP: &str = "m-0 text-xs text-muted";

/// A disclosure: a summary that opens what it holds.
pub const DISCLOSURE: &str = "rounded-card border border-line bg-raised shadow-card \
                              [&>summary]:cursor-pointer [&>summary]:px-4 [&>summary]:py-3 \
                              [&>summary]:font-medium";

/// What a disclosure holds once open.
pub const DISCLOSURE_BODY: &str = "border-t border-line px-4 py-3";

// TODO(#1096): the banner under the top bar once the gateway reports its deployment mode.
/// The sandbox banner under the top bar: amber, as the family's viewers
/// draw it.
pub const BANNER: &str = "tone-warn border-b border-line bg-warn-bg px-4 py-2 text-sm \
                          text-warn-fg md:px-6";

/// The box a table scrolls in, so the page never does; it takes the keyboard
/// focus, so a keyboard can scroll it. It is drawn as a card with a header
/// row that stays while the rows scroll.
pub const TABLE_SCROLL: &str = "table-scroll";

/// A table cell holding a number, aligned on the right.
pub const NUM: &str = "num";

/// An identifier, a `system_id`, AQL or a key, in the monospace stack.
pub const MONO: &str = "font-mono text-[0.8125rem]";

/// The classes of the state word `word` a gateway reports for a member or a
/// dependency: the dot and the colour repeat what the word says, and a word
/// with no tone of its own reads in the neutral one.
#[must_use]
pub fn state(word: &str) -> &'static str {
    match word {
        "up" => "state state-up",
        "failing" | "down" => "state state-failing",
        "unknown" | "degraded" => "state state-unknown",
        _ => "state",
    }
}

/// The tone of a status a pill or a badge shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Up, complete, done: green.
    Ok,
    /// Unknown, degraded, partial: amber.
    Warn,
    /// Down, failing, refused: red.
    Danger,
    /// Not a state: slate.
    Neutral,
    /// A mark of the console's own, such as a selection: the accent.
    Accent,
}

impl Tone {
    /// The tone of the state word `word`, as the gateway and the console
    /// write it in either case.
    #[must_use]
    pub fn of(word: &str) -> Self {
        match word.to_ascii_lowercase().as_str() {
            "up" | "complete" | "ok" | "active" => Self::Ok,
            "down" | "failing" | "refused" | "error" => Self::Danger,
            "unknown" | "degraded" | "partial" | "pending" => Self::Warn,
            _ => Self::Neutral,
        }
    }

    /// The classes of a pill, a dot and a word, in this tone.
    #[must_use]
    pub const fn pill(self) -> &'static str {
        match self {
            Self::Ok => "pill tone-ok",
            Self::Warn => "pill tone-warn",
            Self::Danger => "pill tone-danger",
            Self::Neutral => "pill tone-neutral",
            Self::Accent => "pill tone-accent",
        }
    }

    /// The classes of a badge, a word with no dot, in this tone.
    #[must_use]
    pub const fn badge(self) -> &'static str {
        match self {
            Self::Ok => "badge tone-ok",
            Self::Warn => "badge tone-warn",
            Self::Danger => "badge tone-danger",
            Self::Neutral => "badge tone-neutral",
            Self::Accent => "badge tone-accent",
        }
    }
}

/// The classes of a button of `variant`.
#[must_use]
pub const fn button(variant: Button) -> &'static str {
    match variant {
        Button::Primary => BUTTON_PRIMARY,
        Button::Secondary => BUTTON_SECONDARY,
        Button::Danger => BUTTON_DANGER,
        Button::Ghost => BUTTON_GHOST,
    }
}

/// The four buttons of the button set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    /// The one action a screen is for.
    Primary,
    /// An action beside it.
    Secondary,
    /// An action that removes or ends something.
    Danger,
    /// An action that does not stand out.
    Ghost,
}

#[cfg(test)]
mod tests {
    use super::{Button, Tone, button, state};

    #[test]
    fn every_state_the_gateway_reports_has_its_tone_and_any_other_word_the_neutral_one() {
        assert_eq!("state state-up", state("up"));
        assert_eq!("state state-failing", state("failing"));
        assert_eq!("state state-failing", state("down"));
        assert_eq!("state state-unknown", state("unknown"));
        assert_eq!("state state-unknown", state("degraded"));
        assert_eq!("state", state("registry-invalid"));
        assert_eq!("state", state("not reported"));
    }

    #[test]
    fn a_pill_names_its_tone_by_the_word_in_either_case() {
        assert_eq!(Tone::Ok, Tone::of("UP"));
        assert_eq!(Tone::Ok, Tone::of("complete"));
        assert_eq!(Tone::Danger, Tone::of("DOWN"));
        assert_eq!(Tone::Danger, Tone::of("refused"));
        assert_eq!(Tone::Warn, Tone::of("partial"));
        assert_eq!(Tone::Warn, Tone::of("UNKNOWN"));
        assert_eq!(Tone::Neutral, Tone::of("withdrawn"));
        assert_eq!("pill tone-ok", Tone::Ok.pill());
        assert_eq!("badge tone-neutral", Tone::Neutral.badge());
    }

    /// The shape every button shares; a variant adds its colours.
    const SHAPE: &str = "state-change inline-flex items-center justify-center gap-2";

    #[test]
    fn every_button_shares_the_one_shape() {
        for variant in [
            Button::Primary,
            Button::Secondary,
            Button::Danger,
            Button::Ghost,
        ] {
            assert!(
                button(variant).starts_with(SHAPE),
                "{variant:?}: {}",
                button(variant)
            );
        }
    }
}
