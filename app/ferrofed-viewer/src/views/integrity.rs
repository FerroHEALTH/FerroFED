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
use ferrohealth_ui::icon::Glyph;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::table::view::{Column, NUM};
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::tone::Tone;
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::empty::{Busy, busy};
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::view::status::{badge, pill};
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::view::surface::{card, grid_start};
use ferrohealth_ui::view::surface::{page_header, stack};
#[cfg(not(target_arch = "wasm32"))]
use ferrohealth_ui::view::time::timestamp;
#[cfg(not(target_arch = "wasm32"))]
use leptos::either::Either;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

use crate::admin::act::ClearEhrId;
use crate::locale::{Msg, tr};
use crate::views::model::IntegrityView;
#[cfg(not(target_arch = "wasm32"))]
use crate::views::model::{ClearedRow, IncidentRow};
use crate::views::{load, shown, titled};
#[cfg(not(target_arch = "wasm32"))]
use crate::views::{replica_line, table, table_unstacked};

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
        <Title text=titled(&tr(Msg::Integrity)) />
        {page_header(tr(Msg::Integrity), Some(tr(Msg::IncidentsReplicaKeepsCollisionsCleared)), ())}
        <Transition fallback=|| busy(
            tr(Msg::LoadingTheIncidents),
            Busy::Section,
        )>
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
    stack(view! {
        <div inner_html=view.html></div>
        {clear}
    })
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

/// The columns of the counts by kind.
#[cfg(not(target_arch = "wasm32"))]
fn count_columns() -> [Column; 2] {
    [
        Column::text("kind", tr(Msg::Kind)),
        Column::text("count", tr(Msg::Count)).numeric(),
    ]
}

/// The columns of the incidents table.
#[cfg(not(target_arch = "wasm32"))]
fn incident_columns() -> [Column; 4] {
    [
        Column::text("at", tr(Msg::At)),
        Column::text("kind", tr(Msg::Kind)),
        Column::text("description", tr(Msg::Description)),
        Column::text("state", tr(Msg::State)),
    ]
}

/// The columns of the collisions cleared.
#[cfg(not(target_arch = "wasm32"))]
fn cleared_columns() -> [Column; 3] {
    [
        Column::text("at", tr(Msg::ClearedAt)),
        Column::text("ehr_id", "ehr_id").mono(),
        Column::text("claimants", tr(Msg::MembersThatClaimedIt)),
    ]
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
                    <td data-label=tr(Msg::Count) class=NUM>
                        {*count}
                    </td>
                </tr>
            }
        })
        .collect_view();
    view! {
        {replica_line(shown.answered_by.clone())}
        <div class="fui-stack">
            {filter_tabs(shown.resolved, open, closed)} {incidents_table(&rows, shown.resolved)}
            {grid_start(
                view! {
                    {table_unstacked(
                        &tr(Msg::IncidentsSinceGatewayStarted),
                        &count_columns(),
                        counts.into_any(),
                    )}
                    {cleared_section(&shown.cleared)}
                },
            )}
        </div>
    }
    .to_html()
}

/// The tabs between the open and the resolved incidents, each with its
/// count, the one shown marked as the page.
#[cfg(not(target_arch = "wasm32"))]
fn filter_tabs(resolved: bool, open: usize, closed: usize) -> impl IntoView {
    view! {
        <nav aria-label=tr(Msg::IncidentsByState) class="fui-tabs">
            <a
                href=crate::views::INTEGRITY
                class="fui-tab"
                aria-current=(!resolved).then_some("page")
            >
                {tr(Msg::Open)}
                {badge(Tone::Neutral, &open.to_string())}
            </a>
            <a
                href="/integrity?show=resolved"
                class="fui-tab"
                aria-current=resolved.then_some("page")
            >
                {tr(Msg::Resolved)}
                {badge(Tone::Neutral, &closed.to_string())}
            </a>
        </nav>
    }
}

/// The incidents `rows` of the state the filter shows, newest first, or
/// the empty state of that filter.
#[cfg(not(target_arch = "wasm32"))]
fn incidents_table(rows: &[&IncidentRow], resolved: bool) -> AnyView {
    if rows.is_empty() {
        let empty = if resolved {
            Either::Left(empty_quiet(
                Glyph::INBOX,
                tr(Msg::NoResolvedIncidents),
                tr(Msg::NoCollisionHeldHereWasCleared),
            ))
        } else {
            Either::Right(empty_quiet(
                Glyph::SHIELD_CHECK,
                tr(Msg::NoOpenIncidents),
                tr(Msg::NoneIncidentsReplicaKeepsOpen),
            ))
        };
        return view! { <div class="fui-card">{empty}</div> }.into_any();
    }
    let rows = rows
        .iter()
        .map(|row| {
            let (tone, word) = if row.resolved {
                (Tone::Ok, tr(Msg::StateResolved))
            } else {
                (Tone::Bad, tr(Msg::StateOpen))
            };
            view! {
                <tr>
                    <td data-label=tr(Msg::At)>{timestamp(&row.at)}</td>
                    <th scope="row">{row.kind.clone()}</th>
                    <td data-label=tr(Msg::Description)>{row.description.clone()}</td>
                    <td data-label=tr(Msg::State)>{pill(tone, &word)}</td>
                </tr>
            }
        })
        .collect_view();
    table(
        &tr(Msg::RecentIncidentsNewestFirst),
        &incident_columns(),
        rows.into_any(),
    )
}

/// The collisions the operator cleared recently, newest first.
#[cfg(not(target_arch = "wasm32"))]
fn cleared_section(cleared: &[ClearedRow]) -> AnyView {
    if cleared.is_empty() {
        return card(
            tr(Msg::CollisionsClearedRecently),
            view! { <p class="fui-muted">{tr(Msg::NoCollisionClearedSinceGatewayStarted)}</p> },
        )
        .into_any();
    }
    let rows = cleared
        .iter()
        .map(|cleared| {
            view! {
                <tr>
                    <td data-label=tr(Msg::ClearedAt)>{timestamp(&cleared.at)}</td>
                    <th scope="row" class="fui-mono">
                        {cleared.ehr_id.clone().unwrap_or_else(|| tr(Msg::NotABareUuid))}
                    </th>
                    <td data-label=tr(Msg::MembersThatClaimedIt)>{cleared.claimants.join(", ")}</td>
                </tr>
            }
        })
        .collect_view();
    table(
        &tr(Msg::CollisionsClearedRecentlyNewestFirst),
        &cleared_columns(),
        rows.into_any(),
    )
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
