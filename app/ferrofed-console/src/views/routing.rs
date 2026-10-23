// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routing view: one page of the `creating_system_id` routing table
//! (N21, §12.2), with a filter over the rows the page loaded.
//!
//! The filter runs in the browser over the rows already on the page and
//! asks the gateway nothing; it names no patient, so it needs no URL of its
//! own.

use ferrohealth_ui::table::view::Column;
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::status::badge;
use ferrohealth_ui::view::surface::page_header;
use ferrohealth_ui::widget::filter::table_filter;
use leptos::prelude::*;
use leptos_meta::Title;

use crate::locale::{Msg, tr, tr_with};
use crate::views::model::{RouteRow, RoutingView};
use crate::views::{ROUTING, load, offset_in_url, pager, replica_line, shown, table, titled};

/// The `creating_system_id` routing table.
#[component]
pub fn RoutingPage() -> impl IntoView {
    let loaded = Resource::new_blocking(offset_in_url(), load::routing);
    let filter = RwSignal::new(String::new());
    view! {
        <Title text=titled(&tr(Msg::Routing)) />
        {page_header(tr(Msg::Routing), Some(tr(Msg::WhereEachCreatingSystemIdRoutes)), ())}
        <Transition fallback=|| busy(
            tr(Msg::LoadingTheRoutingTable),
            Busy::Section,
        )>
            {move || Suspend::new(async move {
                shown(loaded.await, move |view| routing_section(view, filter))
            })}
        </Transition>
    }
}

/// Whether `route` matches the filter `text`: the text, in any case, in
/// its `creating_system_id`, source, node or endpoint.
fn matches(route: &RouteRow, text: &str) -> bool {
    let wanted = text.trim().to_lowercase();
    wanted.is_empty()
        || [
            Some(route.creating_system_id.as_str()),
            Some(route.source.as_str()),
            route.node.as_deref(),
            route.endpoint.as_deref(),
        ]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&wanted))
}

/// The tone of the route source `source`.
fn source_tone(source: &str) -> Tone {
    match source {
        "member" => Tone::Accent,
        "withdrawn" => Tone::Warn,
        _ => Tone::Neutral,
    }
}

/// The columns of the routing table.
fn columns() -> [Column; 4] {
    [
        Column::text("creating_system_id", "creating_system_id").mono(),
        Column::text("source", tr(Msg::Source)),
        Column::text("node", tr(Msg::Node)),
        Column::text("endpoint", tr(Msg::Endpoint)),
    ]
}

/// The filter, the page of the table it filters, and the pager.
fn routing_section(view: RoutingView, filter: RwSignal<String>) -> AnyView {
    let routes_pager = pager(ROUTING, view.offset, view.routes.len(), view.total);
    let on_page = view.routes.len();
    let rows = StoredValue::new(view.routes);
    let kept = Memo::new(move |_| {
        rows.with_value(|rows| {
            filter.with(|text| {
                rows.iter()
                    .filter(|route| matches(route, text))
                    .cloned()
                    .collect::<Vec<_>>()
            })
        })
    });
    let count = Signal::derive(move || {
        let kept = kept.with(Vec::len);
        tr_with(
            Msg::RowsPage,
            &[
                ("kept", (kept).to_string().into()),
                ("on_page", (on_page).to_string().into()),
            ],
        )
    });
    let none = move || kept.with(Vec::is_empty);
    let answered_by = replica_line(view.answered_by);
    let body = view! {
        <For each=move || kept.get() key=|route| route.creating_system_id.clone() let(route)>
            <tr>
                <th scope="row" class="fui-mono">
                    {route.creating_system_id}
                </th>
                <td data-label=tr(
                    Msg::Source,
                )>{badge(source_tone(&route.source), &route.source)}</td>
                <td data-label=tr(Msg::Node) class="fui-mono">
                    {route.node.unwrap_or_default()}
                </td>
                <td data-label=tr(Msg::Endpoint) class="fui-mono">
                    {route.endpoint.unwrap_or_default()}
                </td>
            </tr>
        </For>
        <Show when=none>
            <tr>
                <td colspan="4" class="fui-muted">
                    {tr(Msg::NoRowPageMatchesFilter)}
                </td>
            </tr>
        </Show>
    }
    .into_any();
    let table = table(&tr(Msg::CreatingSystemIdRoutingTable), &columns(), body);
    let filtered = table_filter("routing-filter", tr(Msg::Filter), filter, count).into_any();
    view! {
        {answered_by}
        <div class="fui-stack">{filtered} {table} {routes_pager}</div>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::matches;
    use crate::views::model::RouteRow;

    #[test]
    fn the_filter_matches_any_field_in_any_case_and_an_empty_one_matches_all() {
        let route = RouteRow {
            creating_system_id: String::from("Legacy-A.example.org"),
            source: String::from("learned"),
            node: Some(String::from("node-a")),
            endpoint: None,
        };
        assert!(matches(&route, ""));
        assert!(matches(&route, "  "));
        assert!(matches(&route, "legacy-a"));
        assert!(matches(&route, "LEARNED"));
        assert!(matches(&route, "node-a"));
        assert!(!matches(&route, "node-b"));
    }
}
