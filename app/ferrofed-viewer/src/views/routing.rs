// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routing view: one page of the `creating_system_id` routing table
//! (N21, §12.2), with a filter over the rows the page loaded.
//!
//! The filter runs in the browser over the rows already on the page and
//! asks the gateway nothing; it names no patient, so it needs no URL of its
//! own.

use leptos::prelude::*;
use leptos_meta::Title;

use crate::components::{page_header, replica_meta, table_card};
use crate::styles::{self, Tone};
use crate::views::model::{RouteRow, RoutingView};
use crate::views::{ROUTING, load, offset_in_url, pager, shown, titled};

/// The `creating_system_id` routing table.
#[component]
pub fn RoutingPage() -> impl IntoView {
    let loaded = Resource::new_blocking(offset_in_url(), load::routing);
    let filter = RwSignal::new(String::new());
    view! {
        <Title text=titled("Routing") />
        {page_header("Routing", "Where each creating_system_id routes, and on what ground.", ())}
        <Transition fallback=|| {
            view! { <p>"Loading the routing table."</p> }
        }>
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
    let count = move || {
        let kept = kept.with(Vec::len);
        format!("{kept} of {on_page} rows on this page.")
    };
    let none = move || kept.with(Vec::is_empty);
    view! {
        {replica_meta(view.answered_by)}
        <div class=styles::FILTER>
            <div class="flex w-full max-w-sm flex-col gap-1">
                <label for="routing-filter">"Filter"</label>
                <input
                    type="search"
                    id="routing-filter"
                    name="filter"
                    autocomplete="off"
                    spellcheck="false"
                    aria-describedby="routing-filter-help"
                    prop:value=move || filter.get()
                    on:input:target=move |event| filter.set(event.target().value())
                />
            </div>
            <p id="routing-filter-help" role="status" class="m-0 pb-2 text-xs text-muted">
                {count}
            </p>
        </div>
        {table_card(
            "The creating_system_id routing table",
            view! {
                <table>
                    <caption>"The creating_system_id routing table"</caption>
                    <thead>
                        <tr>
                            <th scope="col">"creating_system_id"</th>
                            <th scope="col">"Source"</th>
                            <th scope="col">"Node"</th>
                            <th scope="col">"Endpoint"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <For
                            each=move || kept.get()
                            key=|route| route.creating_system_id.clone()
                            let(route)
                        >
                            <tr>
                                <th scope="row" class=styles::ID>
                                    {route.creating_system_id}
                                </th>
                                <td>
                                    <span class=source_tone(&route.source)
                                        .badge()>{route.source.clone()}</span>
                                </td>
                                <td class=styles::MONO>{route.node.unwrap_or_default()}</td>
                                <td class=styles::MONO>{route.endpoint.unwrap_or_default()}</td>
                            </tr>
                        </For>
                        <Show when=none>
                            <tr>
                                <td colspan="4" class="text-muted">
                                    "No row on this page matches the filter."
                                </td>
                            </tr>
                        </Show>
                    </tbody>
                </table>
            },
        )}
        {routes_pager}
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
