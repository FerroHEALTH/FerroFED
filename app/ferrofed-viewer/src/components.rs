// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shared pieces every page is built from.
//!
//! They are the page header, the card, the stat card, the table card, the
//! status pill and the badge, the tabs, the empty state, the form field and
//! the disclosure, painted with the classes of [`crate::styles`].
//!
//! Each is a plain function returning its typed view, never a
//! `#[component]` and never an erased one: a component writes a props type
//! and a builder the browser bundle carries for each, and an erased view
//! carries its own rendering code, while these take no reactive input. A
//! page that nests them erases its own sections, so rustc sees a shallow
//! tree.

use leptos::either::Either;
use leptos::prelude::*;

use crate::styles::{self, Tone};

/// A Lucide icon beside the word it illustrates, hidden from assistive
/// technology, which reads the word (WCAG 2.2 SC 1.1.1).
#[must_use]
pub fn icon(glyph: icondata_core::Icon, size: &'static str) -> impl IntoView {
    view! {
        <span class="inline-flex shrink-0" aria-hidden="true">
            <leptos_icons::Icon icon=glyph width=size height=size />
        </span>
    }
}

/// The header that opens a page: its `title` and one `subtitle` line, with
/// `actions` at the far end.
#[must_use]
pub fn page_header(
    title: &'static str,
    subtitle: &'static str,
    actions: impl IntoView,
) -> impl IntoView {
    view! {
        <header class=styles::PAGE_HEADER>
            <div class="min-w-0">
                <h1 class=styles::PAGE_TITLE>{title}</h1>
                <p class=styles::PAGE_SUBTITLE>{subtitle}</p>
            </div>
            <div class="flex flex-wrap items-center gap-2">{actions}</div>
        </header>
    }
}

/// A card titled `title` holding `body`.
#[must_use]
pub fn card(title: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <section class=styles::CARD_PAD>
            <h2 class=styles::CARD_TITLE>{title}</h2>
            {body}
        </section>
    }
}

/// A stat card: the `value` of a measure `label` names, beside its `glyph`,
/// leading to `href` when given.
#[must_use]
pub fn stat_card(
    glyph: icondata_core::Icon,
    value: String,
    label: &'static str,
    href: Option<&'static str>,
) -> impl IntoView {
    let inner = view! {
        <div class="flex items-center gap-3">
            <span class="flex size-10 shrink-0 items-center justify-center rounded-control bg-accent-tint text-accent">
                {icon(glyph, "20")}
            </span>
            <div class="min-w-0">
                <p class="m-0 text-2xl font-semibold tabular-nums text-ink">{value}</p>
                <p class="m-0 truncate text-sm text-muted">{label}</p>
            </div>
        </div>
    };
    match href {
        Some(href) => Either::Left(view! {
            <a
                href=href
                class="state-change block rounded-card border border-line bg-raised p-4 text-ink no-underline shadow-card hover:border-accent hover:text-ink"
            >
                {inner}
            </a>
        }),
        None => Either::Right(
            view! { <div class="rounded-card border border-line bg-raised p-4 shadow-card">{inner}</div> },
        ),
    }
}

/// A stat card that leads to `href`: the `value` of a measure `label`
/// names, beside its `glyph`, with `mark` (a pill or a badge that says
/// more) after the label.
#[must_use]
pub fn stat_card_marked(
    glyph: icondata_core::Icon,
    value: String,
    label: &'static str,
    href: &'static str,
    mark: impl IntoView,
) -> impl IntoView {
    view! {
        <a
            href=href
            class="state-change block rounded-card border border-line bg-raised p-4 text-ink no-underline shadow-card hover:border-accent hover:text-ink"
        >
            <div class="flex items-center gap-3">
                <span class="flex size-10 shrink-0 items-center justify-center rounded-control bg-accent-tint text-accent">
                    {icon(glyph, "20")}
                </span>
                <div class="min-w-0">
                    <p class="m-0 text-2xl font-semibold tabular-nums text-ink">{value}</p>
                    <p class="m-0 flex flex-wrap items-center gap-x-2 gap-y-1 text-sm text-muted">
                        <span>{label}</span>
                        {mark}
                    </p>
                </div>
            </div>
        </a>
    }
}

/// The line under a page's header that names the replica which answered
/// it; nothing when the replica has no name.
#[must_use]
pub fn replica_meta(line: Option<String>) -> impl IntoView {
    line.map(|line| view! { <p class=styles::PAGE_META>{line}</p> })
}

/// A quiet empty state inside a card: a `glyph`, a `title` under a level-2
/// heading, and one `line`.
#[must_use]
pub fn empty_card(
    glyph: icondata_core::Icon,
    title: &'static str,
    line: &'static str,
) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center px-4 py-8 text-center">
            <span class="mb-3 flex size-10 items-center justify-center rounded-full bg-inset text-muted">
                {icon(glyph, "20")}
            </span>
            <h2 class="m-0 text-sm font-semibold text-ink">{title}</h2>
            <p class="mt-1 mb-0 text-sm text-muted">{line}</p>
        </div>
    }
}

/// A table in its card, `label` naming the box it scrolls in.
///
/// The header row stays while the rows scroll, and a table wider than the
/// screen scrolls sideways in the box, never the page (WCAG 2.2 SC 1.4.10).
#[must_use]
pub fn table_card(label: &'static str, table: impl IntoView) -> impl IntoView {
    view! {
        <div class=styles::TABLE_SCROLL role="region" aria-label=label tabindex="0">
            {table}
        </div>
    }
}

/// A status pill: a dot and the state `word`, in the tone the word has.
#[must_use]
pub fn status_pill(word: String) -> impl IntoView {
    let class = Tone::of(&word).pill();
    view! { <span class=class>{word}</span> }
}

/// A badge: the word `text`, in `tone`, with no dot.
#[must_use]
pub fn badge(text: &'static str, tone: Tone) -> impl IntoView {
    view! { <span class=tone.badge()>{text}</span> }
}

/// One tab: where it leads and what it says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tab {
    /// The path the tab leads to.
    pub href: &'static str,
    /// What the tab says.
    pub label: &'static str,
}

/// Tabs that lead between the pages of one place, `label` naming them, the
/// one at `current` marked as the page.
#[must_use]
pub fn tabs(label: &'static str, items: &[Tab], current: &str) -> impl IntoView {
    let links = items
        .iter()
        .map(|tab| {
            let here = (tab.href == current).then_some("page");
            view! {
                <a href=tab.href class=styles::TAB aria-current=here>
                    {tab.label}
                </a>
            }
        })
        .collect_view();
    view! {
        <nav aria-label=label class=styles::TABS>
            {links}
        </nav>
    }
}

/// An empty state: a `glyph`, a `title`, the line `body` (a paragraph of
/// [`styles::EMPTY_BODY`]), and one `action`, centred in what holds it.
#[must_use]
pub fn empty_state(
    glyph: icondata_core::Icon,
    title: &'static str,
    body: impl IntoView,
    action: impl IntoView,
) -> impl IntoView {
    view! {
        <div class="mx-auto flex max-w-md flex-col items-center px-4 py-10 text-center">
            <span class="mb-4 flex size-12 items-center justify-center rounded-full bg-inset text-muted">
                {icon(glyph, "24")}
            </span>
            <h1 class="m-0 text-xl font-semibold text-ink">{title}</h1>
            {body}
            {action}
        </div>
    }
}

/// A form field: the `label` of the control `id` names, the control, and its
/// one line of `help`, whose id is `id` and `-help`, which the control names
/// in its `aria-describedby`.
#[must_use]
pub fn field(
    id: &'static str,
    label: &'static str,
    help: &'static str,
    control: impl IntoView,
) -> impl IntoView {
    let help_id = format!("{id}-help");
    view! {
        <div class=styles::FIELD>
            <label for=id>{label}</label>
            {control}
            <p id=help_id class=styles::HELP>
                {help}
            </p>
        </div>
    }
}

/// A disclosure: `summary`, which opens `body` (HTML `details`, which the
/// browser opens and closes from the keyboard with no script).
#[must_use]
pub fn disclosure(summary: &'static str, body: impl IntoView) -> impl IntoView {
    view! {
        <details class=styles::DISCLOSURE>
            <summary>{summary}</summary>
            <div class=styles::DISCLOSURE_BODY>{body}</div>
        </details>
    }
}

/// A link styled as a button of `variant`, with `glyph` beside its `label`.
#[must_use]
pub fn link_button(
    href: &'static str,
    variant: styles::Button,
    glyph: icondata_core::Icon,
    label: &'static str,
) -> impl IntoView {
    view! {
        <a href=href class=styles::button(variant)>
            {icon(glyph, "16")}
            {label}
        </a>
    }
}
