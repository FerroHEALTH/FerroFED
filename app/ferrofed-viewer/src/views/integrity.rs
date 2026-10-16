// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The integrity view: the incidents the replica keeps, filtered between
//! the open and the resolved ones, the counts since the gateway started,
//! and the collisions the operator cleared.
//!
//! An incident is an event, and a clear removes none: an incident reads as
//! resolved once the operator cleared the `ehr_id` it is about after it was
//! emitted, and open otherwise. The filter is the URL's `show` parameter,
//! which names no patient. The read-only part is rendered on the server and
//! shown as the HTML it is; the collision clear beside it is hydrated.

#[cfg(not(target_arch = "wasm32"))]
use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::admin::act::ClearEhrId;
use crate::components::page_header;
#[cfg(not(target_arch = "wasm32"))]
use crate::components::{empty_card, replica_meta, table_card};
#[cfg(not(target_arch = "wasm32"))]
use crate::styles::{self, Tone};
use crate::views::model::IntegrityView;
#[cfg(not(target_arch = "wasm32"))]
use crate::views::model::{ClearedRow, IncidentRow};
use crate::views::{load, shown, titled};

/// The integrity incidents and the collisions cleared.
#[component]
pub fn IntegrityPage() -> impl IntoView {
    let query = use_query_map();
    // NOTE: no specification governs this: our own design; any `show` but
    // `resolved` lists the open incidents, as a link with none does.
    let show = move || query.read().get("show").unwrap_or_default();
    let loaded = Resource::new_blocking(show, load::integrity);
    let clear = ServerAction::<ClearEhrId>::new();
    view! {
        <Title text=titled("Integrity") />
        {page_header(
            "Integrity",
            "The incidents this replica keeps, and the collisions cleared.",
            (),
        )}
        <Transition fallback=|| {
            view! { <p>"Loading the incidents."</p> }
        }>
            {move || Suspend::new(async move {
                shown(loaded.await, move |view| integrity_section(view, clear))
            })}
        </Transition>
    }
}

/// The part the server rendered, and the collision clear when an admin
/// listener is configured.
fn integrity_section(view: IntegrityView, clear: ServerAction<ClearEhrId>) -> AnyView {
    let clear = crate::admin::clear_section(clear, view.clear_form);
    view! {
        <div class="contents" inner_html=view.html></div>
        {clear}
    }
    .into_any()
}

/// What the read-only part of the integrity view shows.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct Incidents {
    /// How many incidents of each kind were emitted since the gateway
    /// started.
    pub counts: Vec<(String, u64)>,
    /// The most recent incidents, newest first.
    pub recent: Vec<IncidentRow>,
    /// The collisions the operator cleared most recently, newest first.
    pub cleared: Vec<ClearedRow>,
    /// Which replica answered, as the view says it; `None` when the replica
    /// has no name.
    pub answered_by: Option<String>,
    /// Whether the resolved incidents are shown, or the open ones.
    pub resolved: bool,
}

/// The markup of the read-only part of the integrity view of `shown`.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn integrity_html(shown: &Incidents) -> String {
    let closed = shown.recent.iter().filter(|row| row.resolved).count();
    let open = shown.recent.len().saturating_sub(closed);
    let rows: Vec<&IncidentRow> = shown
        .recent
        .iter()
        .filter(|row| row.resolved == shown.resolved)
        .collect();
    let counts = shown
        .counts
        .iter()
        .map(|(kind, count)| {
            view! {
                <tr>
                    <th scope="row">{kind.clone()}</th>
                    <td class=styles::NUM>{*count}</td>
                </tr>
            }
        })
        .collect_view();
    view! {
        {replica_meta(shown.answered_by.clone())}
        {filter_tabs(shown.resolved, open, closed)}
        {incidents_table(&rows, shown.resolved)}
        <div class="grid grid-cols-1 gap-x-4 xl:grid-cols-2">
            <div class="min-w-0">
                {table_card(
                    "Incidents since the gateway started",
                    view! {
                        <table>
                            <caption>"Incidents since the gateway started"</caption>
                            <thead>
                                <tr>
                                    <th scope="col">"Kind"</th>
                                    <th scope="col" class=styles::NUM>
                                        "Count"
                                    </th>
                                </tr>
                            </thead>
                            <tbody>{counts}</tbody>
                        </table>
                    },
                )}
            </div>
            <div class="min-w-0">{cleared_section(&shown.cleared)}</div>
        </div>
    }
    .to_html()
}

/// The tabs between the open and the resolved incidents, each with its
/// count, the one shown marked as the page.
#[cfg(not(target_arch = "wasm32"))]
fn filter_tabs(resolved: bool, open: usize, closed: usize) -> impl IntoView {
    view! {
        <nav aria-label="Incidents by state" class=styles::TABS>
            <a
                href=crate::views::INTEGRITY
                class=styles::TAB
                aria-current=(!resolved).then_some("page")
            >
                "Open "
                <span class=Tone::Neutral.badge()>{open}</span>
            </a>
            <a
                href="/integrity?show=resolved"
                class=styles::TAB
                aria-current=resolved.then_some("page")
            >
                "Resolved "
                <span class=Tone::Neutral.badge()>{closed}</span>
            </a>
        </nav>
    }
}

/// The incidents `rows` of the state the filter shows, newest first, or
/// the empty state of that filter.
#[cfg(not(target_arch = "wasm32"))]
fn incidents_table(rows: &[&IncidentRow], resolved: bool) -> impl IntoView + use<> {
    if rows.is_empty() {
        let empty = if resolved {
            Either::Left(empty_card(
                icondata_lu::LuInbox,
                "No resolved incidents",
                "No collision held here was cleared since it was found.",
            ))
        } else {
            Either::Right(empty_card(
                icondata_lu::LuShieldCheck,
                "No open incidents",
                "None of the incidents this replica keeps is open.",
            ))
        };
        return Either::Left(view! { <div class=format!("mb-6 {}", styles::CARD)>{empty}</div> });
    }
    let rows = rows
        .iter()
        .map(|row| {
            let (tone, word) = if row.resolved {
                (Tone::Ok, "resolved")
            } else {
                (Tone::Danger, "open")
            };
            view! {
                <tr>
                    <td class=format!("{} whitespace-nowrap", styles::MONO)>{row.at.clone()}</td>
                    <th scope="row">{row.kind.clone()}</th>
                    <td>{row.description.clone()}</td>
                    <td>
                        <span class=tone.pill()>{word}</span>
                    </td>
                </tr>
            }
        })
        .collect_view();
    Either::Right(table_card(
        "Recent incidents",
        view! {
            <table>
                <caption>"Recent incidents, newest first"</caption>
                <thead>
                    <tr>
                        <th scope="col">"At"</th>
                        <th scope="col">"Kind"</th>
                        <th scope="col">"Description"</th>
                        <th scope="col">"State"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        },
    ))
}

/// The collisions the operator cleared recently, newest first.
#[cfg(not(target_arch = "wasm32"))]
fn cleared_section(cleared: &[ClearedRow]) -> impl IntoView + use<> {
    if cleared.is_empty() {
        return Either::Left(view! {
            <div class=format!("mt-2 mb-6 {}", styles::CARD_PAD)>
                <p class="m-0 text-sm text-muted">
                    "No collision cleared since the gateway started."
                </p>
            </div>
        });
    }
    let rows = cleared
        .iter()
        .map(|cleared| {
            view! {
                <tr>
                    <td class=format!(
                        "{} whitespace-nowrap",
                        styles::MONO,
                    )>{cleared.at.clone()}</td>
                    <th scope="row" class=styles::ID>
                        {cleared.ehr_id.clone().unwrap_or_else(|| String::from("not a bare UUID"))}
                    </th>
                    <td>{cleared.claimants.join(", ")}</td>
                </tr>
            }
        })
        .collect_view();
    Either::Right(table_card(
        "Collisions cleared recently",
        view! {
            <table>
                <caption>"Collisions cleared recently, newest first"</caption>
                <thead>
                    <tr>
                        <th scope="col">"Cleared at"</th>
                        <th scope="col">"ehr_id"</th>
                        <th scope="col">"Members that claimed it"</th>
                    </tr>
                </thead>
                <tbody>{rows}</tbody>
            </table>
        },
    ))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::{Incidents, integrity_html};
    use crate::views::model::{ClearedRow, IncidentRow};

    /// A value that breaks out of a text and out of an attribute.
    const HOSTILE: &str = r#""><script>alert(1)</script><img src=x onerror=alert(2)>"#;

    // The markup reaches the page through `inner_html`, so every value the
    // gateway names arrives escaped, in either filter.
    #[test]
    fn hostile_values_render_escaped() {
        leptos::prelude::Owner::new().with(|| {
            for resolved in [false, true] {
                let html = integrity_html(&Incidents {
                    counts: vec![(HOSTILE.to_owned(), 1)],
                    recent: vec![IncidentRow {
                        at: HOSTILE.to_owned(),
                        kind: HOSTILE.to_owned(),
                        description: HOSTILE.to_owned(),
                        resolved,
                    }],
                    cleared: vec![ClearedRow {
                        at: HOSTILE.to_owned(),
                        ehr_id: Some(HOSTILE.to_owned()),
                        claimants: vec![HOSTILE.to_owned()],
                    }],
                    answered_by: Some(HOSTILE.to_owned()),
                    resolved,
                });
                assert!(
                    html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
                    "{html}"
                );
                assert!(!html.contains("<script"), "{html}");
                assert!(!html.contains("<img"), "{html}");
            }
        });
    }
}
