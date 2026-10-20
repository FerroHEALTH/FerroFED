// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The results card: the answer of the last query run, its table and its
//! raw result set on two tabs, the table's pages, or what kept the query
//! from running.
//!
//! The answer is the HTML the server rendered ([`super::answer`]), so the
//! browser carries only this frame: the tabs, the paging and the move of the
//! focus to the card once an answer arrives (WCAG 2.2 SC 2.4.3).

use ferrohealth_ui::icon::{Glyph, icon};
use ferrohealth_ui::view::button::Variant;
use ferrohealth_ui::view::empty::empty_quiet;
use leptos::prelude::*;

use super::tabs::{TabPair, tab_list};
use crate::locale::{Msg, tr, tr_with};
use crate::query::model::{PAGE_ROWS, RenderedAnswer};
use crate::query::run::RunQuery;
use crate::views::fault;

/// The results card of `action`.
pub(super) fn results_section(action: ServerAction<RunQuery>) -> AnyView {
    let value = action.value();
    let pending = action.pending();
    let version = action.version();
    let heading = NodeRef::<leptos::html::H2>::new();
    // NOTE: WCAG 2.2 SC 2.4.3: once a run has an answer or a refusal, the
    // focus moves to the card that shows it.
    Effect::new(move |_| {
        let answered = value.with(Option::is_some);
        if let Some(heading) = heading.get()
            && answered
            && heading.focus().is_err()
        {
            leptos::logging::warn!("the browser refused the focus on the results");
        }
    });
    let raw = RwSignal::new(false);
    let paged = Paged {
        turned: RwSignal::new((0, 0)),
        version,
        rows: NodeRef::new(),
    };
    let tabs = move || {
        value.with(|answer| {
            matches!(answer, Some(Ok(answer)) if !answer.refused).then(|| {
                tab_list(
                    TabPair {
                        label: tr(Msg::Answer),
                        tabs: ["query-tab-table", "query-tab-raw"],
                        panels: ["query-panel-table", "query-panel-raw"],
                        names: [tr(Msg::Table), tr(Msg::Raw)],
                    },
                    raw,
                )
            })
        })
    };
    view! {
        <section
            class="fui-card fui-panel"
            aria-labelledby="query-results-title"
            aria-busy=move || pending.get().to_string()
        >
            <div class="fui-panel-head">
                <h2
                    id="query-results-title"
                    tabindex="-1"
                    node_ref=heading
                    class="fui-card-title"
                >
                    {tr(Msg::Results)}
                </h2>
                {tabs}
            </div>
            <div class="fui-panel-body fui-stack">
                <p role="status" class="fui-help">
                    {move || if pending.get() { tr(Msg::RunningTheQuery) } else { String::new() }}
                </p>
                {move || {
                    value
                        .with(|answer| match answer {
                            None => waiting(),
                            Some(Ok(rendered)) if rendered.refused => refusal(rendered),
                            Some(Ok(rendered)) => shown(rendered, raw, paged),
                            Some(Err(error)) => fault(error),
                        })
                }}
            </div>
        </section>
    }
    .into_any()
}

/// What the card shows before any query has run.
fn waiting() -> AnyView {
    empty_quiet(
        Glyph::SQUARE_TERMINAL,
        tr(Msg::NoQueryHasRunYet),
        tr(Msg::WriteAqlNameStoredQueryThen),
    )
    .into_any()
}

/// The refusal `rendered` holds, which the server rendered as the error
/// state of the card.
fn refusal(rendered: &RenderedAnswer) -> AnyView {
    let html = rendered.html.clone();
    view! { <div inner_html=html></div> }.into_any()
}

/// The paging of an answer's rows.
#[derive(Clone, Copy)]
struct Paged {
    /// The page turned to, and the run of the action it was turned for.
    turned: RwSignal<(usize, u32)>,
    /// The run of the action that is shown.
    version: RwSignal<usize>,
    /// What holds the answer the server rendered.
    rows: NodeRef<leptos::html::Div>,
}

impl Paged {
    /// The page shown: the one turned to for this run, or the first.
    fn current(self) -> u32 {
        let (at, page) = self.turned.get();
        if at == self.version.get() { page } else { 0 }
    }

    /// Shows the page `to` of the rows and hides every other.
    fn turn(self, to: u32) {
        self.turned.set((self.version.get_untracked(), to));
        show_page(self.rows, to);
    }
}

/// An answer `rendered` holds: its table, which the server rendered, and its
/// raw result set, on the tab `raw` chooses.
fn shown(rendered: &RenderedAnswer, raw: RwSignal<bool>, paged: Paged) -> AnyView {
    let html = rendered.html.clone();
    let text = rendered.raw.clone();
    let paging = (rendered.rows > PAGE_ROWS).then(|| pages_section(rendered.rows, paged));
    view! {
        <div
            role="tabpanel"
            id="query-panel-table"
            aria-labelledby="query-tab-table"
            hidden=move || raw.get()
        >
            <div inner_html=html node_ref=paged.rows></div>
            {paging}
        </div>
        <div
            role="tabpanel"
            id="query-panel-raw"
            aria-labelledby="query-tab-raw"
            hidden=move || !raw.get()
        >
            <div
                class="fui-code-panel"
                role="region"
                tabindex="0"
                aria-label=tr(Msg::FederatedResultSetJson)
            >
                <pre>{text}</pre>
            </div>
        </div>
    }
    .into_any()
}

/// The pages of `rows` rows: which rows are shown, and the buttons that
/// turn to the page before and the page after.
fn pages_section(rows: u32, paged: Paged) -> AnyView {
    let last = rows.saturating_sub(1).checked_div(PAGE_ROWS).unwrap_or(0);
    let at_last = move || paged.current() >= last;
    let shown = move || {
        let first = paged.current().saturating_mul(PAGE_ROWS);
        let end = first.saturating_add(PAGE_ROWS).min(rows);
        tr_with(
            Msg::RowsToOf,
            &[
                ("first", (first.saturating_add(1)).to_string().into()),
                ("end", (end).to_string().into()),
                ("rows", (rows).to_string().into()),
            ],
        )
    };
    view! {
        <nav aria-label=tr(Msg::PagesOfRows) class="fui-pager">
            <p class="fui-num" aria-live="polite">
                {shown}
            </p>
            <div class="fui-pager-steps">
                <button
                    type="button"
                    class=Variant::Secondary.small()
                    disabled=move || paged.current() == 0
                    on:click=move |_| paged.turn(paged.current().saturating_sub(1))
                >
                    {icon(Glyph::CHEVRON_LEFT, "14")}
                    {tr(Msg::Previous)}
                </button>
                <button
                    type="button"
                    class=Variant::Secondary.small()
                    disabled=at_last
                    on:click=move |_| paged.turn(paged.current().saturating_add(1).min(last))
                >
                    {tr(Msg::Next)}
                    {icon(Glyph::CHEVRON_RIGHT, "14")}
                </button>
            </div>
        </nav>
    }
    .into_any()
}

/// Shows the `tbody` of page `to` in the rows table `rows` holds and hides
/// the others, and scrolls the table back to its first row.
fn show_page(rows: NodeRef<leptos::html::Div>, to: u32) {
    #[cfg(target_arch = "wasm32")]
    {
        let Some(rows) = rows.get_untracked() else {
            return;
        };
        let selector = tr_with(
            Msg::Table2,
            &[(
                "value0",
                (crate::query::model::ROWS_TABLE).to_string().into(),
            )],
        );
        let Ok(Some(table)) = rows.query_selector(&selector) else {
            return;
        };
        let mut index = 0_u32;
        let mut child = table.first_element_child();
        while let Some(element) = child {
            if element.tag_name().eq_ignore_ascii_case("tbody") {
                if element
                    .toggle_attribute_with_force("hidden", index != to)
                    .is_err()
                {
                    leptos::logging::warn!("the browser refused to turn the page");
                }
                index = index.saturating_add(1);
            }
            child = element.next_element_sibling();
        }
        if let Ok(Some(region)) = rows.query_selector("[role=region]") {
            region.set_scroll_top(0);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _: (NodeRef<leptos::html::Div>, u32) = (rows, to);
}
