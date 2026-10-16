// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard's content: what the console reads of the gateway for it,
//! and its markup, rendered on the server.
//!
//! The dashboard is read-only, so the server renders its markup and the
//! page shows that HTML, the way the query console shows its answer: the
//! browser bundle carries none of its drawing. Leptos escapes every text and
//! attribute value in it, and no value is placed as markup.
//!
//! The self-description is the dashboard's ground, so a refusal of it is
//! the page's refusal. Every other part the gateway refuses or cannot
//! answer is named in its own card, with the status and the code, and the
//! rest of the page still shows.

use std::collections::BTreeMap;

use ferrofed_registry::health::DependencyReport;
use ferrofed_registry::operator::PageRequest;
use ferrofed_registry::revision::{RevisionId, Standing};
use leptos::either::Either;
use leptos::prelude::*;

use crate::components::{empty_card, icon, replica_meta, stat_card_marked, status_pill};
use crate::gateway::{AccessToken, GatewayError};
use crate::server::ViewerState;
use crate::styles::{self, Tone};
use crate::views::load::server::{self as load, Stop};
use crate::views::model::{IncidentRow, MemberRow, Refusal};

/// How many of the newest incidents the dashboard lists.
const RECENT: usize = 5;

/// A part of the dashboard, or the line that says why it is not shown.
type Part<T> = Result<T, String>;

/// The running configuration revision and the newest one, which waits to
/// be applied when it is not the one running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Revisions {
    /// The revision the store marks running.
    pub running: Option<RevisionId>,
    /// The newest revision, when it is not the one running.
    pub pending: Option<RevisionId>,
}

impl From<&Standing> for Revisions {
    fn from(standing: &Standing) -> Self {
        let running = standing.current.as_ref().map(|current| current.revision);
        Self {
            running,
            pending: standing.head.filter(|head| Some(*head) != running),
        }
    }
}

/// What the dashboard shows, as the console read it.
#[derive(Debug, Clone)]
pub struct Board {
    /// Every member endpoint with its health.
    pub members: Vec<MemberRow>,
    /// Whether the health was reported, or why not.
    pub health: Part<()>,
    /// The line naming the replica that observed the health.
    pub answered_by: Option<String>,
    /// The incidents the replica keeps, newest first.
    pub incidents: Part<Vec<IncidentRow>>,
    /// How many stored-query versions the gateway holds.
    pub stored: Part<u64>,
    /// The configuration revisions, for a console configured with a
    /// replica's admin listener.
    pub revisions: Option<Part<Revisions>>,
}

/// The line that says why a part the gateway did not answer is not shown:
/// the gateway's status and code, or the console's fault.
fn not_shown(error: &GatewayError) -> String {
    match load::refused(error) {
        Stop::Refused(Refusal::Gateway { status, code }) => format!(
            "The gateway refused this part: {status} ({}).",
            code.as_deref().unwrap_or("no code")
        ),
        Stop::Refused(Refusal::NotAuthenticated { code }) => format!(
            "The gateway did not accept your sign-in ({}).",
            code.as_deref().unwrap_or("no code")
        ),
        Stop::Refused(Refusal::Invalid { reason }) => reason,
        Stop::Fault(fault) => format!("Not shown: {fault}."),
    }
}

/// Reads what the dashboard shows from the gateway, as the operator whose
/// access token is `token`.
///
/// # Errors
/// Returns the [`Stop`] of the self-description the gateway refused or did
/// not answer.
pub async fn gather(state: &ViewerState, token: &AccessToken) -> Result<Board, Stop> {
    let gateway = state.gateway();
    let description = gateway.self_description(token).await?;
    let (dependencies, health) = match gateway.dependencies(token).await {
        Ok(report) => (report, Ok(())),
        Err(error) => (DependencyReport::default(), Err(not_shown(&error))),
    };
    let members = load::members(&description, &dependencies);
    let answered_by = if health.is_ok() {
        members.answered_by
    } else {
        None
    };
    let incidents = gateway
        .incidents(token)
        .await
        .map(|report| load::incidents(&report))
        .map_err(|error| not_shown(&error));
    let stored = gateway
        .stored_queries(
            token,
            PageRequest {
                offset: 0,
                limit: 1,
            },
        )
        .await
        .map(|page| page.total)
        .map_err(|error| not_shown(&error));
    let revisions = match state.settings().admin.first() {
        Some(replica) => Some(
            gateway
                .config_standing(replica, token)
                .await
                .map(|standing| Revisions::from(&standing))
                .map_err(|error| not_shown(&error)),
        ),
        None => None,
    };
    Ok(Board {
        members: members.members,
        health,
        answered_by,
        incidents,
        stored,
        revisions,
    })
}

/// How the nodes the members name stand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nodes {
    /// The nodes every endpoint of which is up.
    pub up: u64,
    /// The nodes an endpoint of which is down or failing.
    pub down: u64,
    /// Every node.
    pub total: u64,
}

/// Counts how the nodes the members name stand.
///
/// A node is up when every endpoint of it is, down when one of them is down
/// or failing, and unknown otherwise; an endpoint that names no node counts
/// as a node of its own.
#[must_use]
pub fn nodes(members: &[MemberRow]) -> Nodes {
    let mut nodes: BTreeMap<&str, (bool, bool)> = BTreeMap::new();
    for member in members {
        let key = member.node_id.as_deref().unwrap_or(&member.endpoint_id);
        let (up, down) = nodes.entry(key).or_insert((true, false));
        *up = *up && member.health == "up";
        *down = *down || matches!(member.health.as_str(), "down" | "failing");
    }
    let count = |pick: fn(&(bool, bool)) -> bool| {
        u64::try_from(nodes.values().filter(|node| pick(node)).count()).unwrap_or(u64::MAX)
    };
    Nodes {
        up: count(|node| node.0),
        down: count(|node| node.1),
        total: u64::try_from(nodes.len()).unwrap_or(u64::MAX),
    }
}

/// The tone and the word of `nodes`, or of health that was not reported.
fn nodes_mark(nodes: Nodes, reported: bool) -> (Tone, String) {
    if !reported {
        (Tone::Neutral, String::from("not reported"))
    } else if nodes.total == 0 {
        (Tone::Neutral, String::from("no nodes"))
    } else if nodes.up == nodes.total {
        (Tone::Ok, String::from("all up"))
    } else if nodes.down > 0 {
        (Tone::Danger, format!("{} down", nodes.down))
    } else {
        (Tone::Warn, String::from("not yet observed"))
    }
}

/// The dashboard's markup.
#[must_use]
pub fn render(board: &Board) -> String {
    let stats = stats(board);
    let health = health_panel(board);
    let incidents = incidents_panel(&board.incidents);
    view! {
        {replica_meta(board.answered_by.clone())}
        <div class="mb-6 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5">
            {stats}
        </div>
        <div class="grid grid-cols-1 gap-4 xl:grid-cols-2">{health} {incidents}</div>
    }
    .to_html()
}

/// The badge of a stat whose part was not shown.
fn unknown_mark() -> impl IntoView {
    view! { <span class=Tone::Neutral.badge()>"not shown"</span> }
}

/// The stat cards.
fn stats(board: &Board) -> impl IntoView + use<> {
    let total = u64::try_from(board.members.len()).unwrap_or(u64::MAX);
    let active = u64::try_from(
        board
            .members
            .iter()
            .filter(|member| member.status == "active")
            .count(),
    )
    .unwrap_or(u64::MAX);
    let members = stat_card_marked(
        icondata_lu::LuUsers,
        format!("{active} of {total}"),
        "Members active",
        crate::views::MEMBERS,
        (),
    );
    let standing = nodes(&board.members);
    let (tone, word) = nodes_mark(standing, board.health.is_ok());
    let nodes = stat_card_marked(
        icondata_lu::LuHeartPulse,
        if board.health.is_ok() {
            format!("{} of {}", standing.up, standing.total)
        } else {
            String::from("n/a")
        },
        "Nodes up",
        crate::views::MEMBERS,
        view! { <span class=tone.pill()>{word}</span> },
    );
    let open = match &board.incidents {
        Ok(rows) => Either::Left(stat_card_marked(
            icondata_lu::LuShieldAlert,
            rows.iter().filter(|row| !row.resolved).count().to_string(),
            "Open incidents",
            crate::views::INTEGRITY,
            (),
        )),
        Err(_) => Either::Right(stat_card_marked(
            icondata_lu::LuShieldAlert,
            String::from("n/a"),
            "Open incidents",
            crate::views::INTEGRITY,
            unknown_mark(),
        )),
    };
    let stored = match &board.stored {
        Ok(total) => Either::Left(stat_card_marked(
            icondata_lu::LuDatabase,
            total.to_string(),
            "Stored queries",
            crate::views::STORED_QUERIES,
            (),
        )),
        Err(_) => Either::Right(stat_card_marked(
            icondata_lu::LuDatabase,
            String::from("n/a"),
            "Stored queries",
            crate::views::STORED_QUERIES,
            unknown_mark(),
        )),
    };
    let revision = board.revisions.as_ref().map(revision_stat);
    view! {
        {members}
        {nodes}
        {open}
        {stored}
        {revision}
    }
}

/// The stat card of the running configuration revision, with the revision
/// that waits to be applied.
fn revision_stat(revisions: &Part<Revisions>) -> impl IntoView + use<> {
    let (value, mark) = match revisions {
        Ok(revisions) => (
            revisions
                .running
                .map_or_else(|| String::from("none"), |running| format!("r{running}")),
            revisions.pending.map(|pending| {
                view! { <span class=Tone::Warn.badge()>{format!("revision {pending} pending")}</span> }
            }),
        ),
        Err(_) => (String::from("n/a"), None),
    };
    let shown = revisions.is_ok();
    stat_card_marked(
        icondata_lu::LuSettings,
        value,
        "Running revision",
        crate::configure::CONFIGURATION,
        view! {
            {mark}
            {(!shown).then(unknown_mark)}
        },
    )
}

/// A panel titled `title` with a link to `href` that says `more`, holding
/// `body`.
fn panel(
    id: &'static str,
    title: &'static str,
    (href, more): (&'static str, &'static str),
    body: impl IntoView,
) -> impl IntoView {
    view! {
        <section class=styles::PANEL aria-labelledby=id>
            <div class=styles::PANEL_HEAD>
                <h2 id=id class="m-0 text-sm font-semibold text-ink">
                    {title}
                </h2>
                <a href=href class="inline-flex items-center gap-1 text-sm font-medium">
                    {more}
                    {icon(icondata_lu::LuArrowRight, "14")}
                </a>
            </div>
            {body}
        </section>
    }
}

/// The notice of a part the gateway did not answer.
fn notice(line: &str) -> impl IntoView + use<> {
    let line = line.to_owned();
    view! {
        <div class="p-4">
            <p role="alert" class="m-0">
                {line}
            </p>
        </div>
    }
}

/// The box a panel's table scrolls sideways in, `label` naming it.
fn scroller(label: &'static str, table: impl IntoView) -> impl IntoView {
    view! {
        <div class="overflow-x-auto" role="region" aria-label=label tabindex="0">
            {table}
        </div>
    }
}

/// The member health panel: each endpoint, its node, its health and its
/// median latency.
fn health_panel(board: &Board) -> impl IntoView + use<> {
    let notice = board.health.as_ref().err().map(|line| notice(line));
    let rows = board
        .members
        .iter()
        .map(|member| {
            let endpoint = member.endpoint_id.clone();
            let name = match member.detail.clone() {
                Some(href) => Either::Left(view! { <a href=href>{endpoint}</a> }),
                None => Either::Right(endpoint),
            };
            view! {
                <tr>
                    <th scope="row" class=styles::ID>
                        {name}
                    </th>
                    <td class=styles::MONO>{member.node_id.clone().unwrap_or_default()}</td>
                    <td>{status_pill(member.health.clone())}</td>
                    <td class=styles::NUM>
                        {member
                            .latency_ms_p50
                            .map(|latency| format!("{latency} ms"))
                            .unwrap_or_default()}
                    </td>
                </tr>
            }
        })
        .collect_view();
    let table = view! {
        <table>
            <thead>
                <tr>
                    <th scope="col">"Endpoint"</th>
                    <th scope="col">"Node"</th>
                    <th scope="col">"Health"</th>
                    <th scope="col" class=styles::NUM>
                        "Median latency"
                    </th>
                </tr>
            </thead>
            <tbody>{rows}</tbody>
        </table>
    };
    let body = if board.members.is_empty() {
        Either::Left(empty_card(
            icondata_lu::LuUsers,
            "No members",
            "The gateway declares no member endpoint.",
        ))
    } else {
        Either::Right(scroller("Member health", table))
    };
    panel(
        "dashboard-health",
        "Member health",
        (crate::views::MEMBERS, "All members"),
        view! {
            {notice}
            {body}
        },
    )
}

/// The recent incidents panel: the newest few, each with its state.
fn incidents_panel(incidents: &Part<Vec<IncidentRow>>) -> impl IntoView + use<> {
    let body = match incidents {
        Err(line) => Either::Left(notice(line)),
        Ok(rows) if rows.is_empty() => Either::Right(Either::Left(empty_card(
            icondata_lu::LuShieldCheck,
            "No incidents",
            "None since the gateway started.",
        ))),
        Ok(rows) => {
            let rows = rows
                .iter()
                .take(RECENT)
                .map(|row| {
                    let (tone, word) = if row.resolved {
                        (Tone::Ok, "resolved")
                    } else {
                        (Tone::Danger, "open")
                    };
                    view! {
                        <tr>
                            <td class=format!(
                                "{} whitespace-nowrap",
                                styles::MONO,
                            )>{row.at.clone()}</td>
                            <th scope="row">{row.kind.clone()}</th>
                            <td>
                                <span class=tone.pill()>{word}</span>
                            </td>
                        </tr>
                    }
                })
                .collect_view();
            Either::Right(Either::Right(scroller(
                "Recent incidents",
                view! {
                    <table>
                        <thead>
                            <tr>
                                <th scope="col">"At"</th>
                                <th scope="col">"Kind"</th>
                                <th scope="col">"State"</th>
                            </tr>
                        </thead>
                        <tbody>{rows}</tbody>
                    </table>
                },
            )))
        }
    };
    panel(
        "dashboard-incidents",
        "Recent incidents",
        (crate::views::INTEGRITY, "All incidents"),
        body,
    )
}

#[cfg(test)]
mod tests {
    use super::{Board, Nodes, Revisions, nodes, render};
    use crate::views::model::{IncidentRow, MemberRow};
    use ferrofed_registry::revision::RevisionId;

    /// A value that breaks out of a text and out of an attribute.
    const HOSTILE: &str = r#""><script>alert(1)</script><img src=x onerror=alert(2)>"#;

    fn member(endpoint: &str, node: Option<&str>, health: &str) -> MemberRow {
        MemberRow {
            endpoint_id: endpoint.to_owned(),
            organisation: String::from("org-a"),
            status: String::from("active"),
            node_id: node.map(str::to_owned),
            system_id: None,
            product: None,
            latency_ms_p50: Some(12),
            health: health.to_owned(),
            detail: crate::views::load::server::detail(node, endpoint),
        }
    }

    /// The markup of `board`, rendered under the reactive owner a request
    /// provides.
    fn drawn(board: &Board) -> String {
        leptos::prelude::Owner::new().with(|| render(board))
    }

    fn board(members: Vec<MemberRow>, incidents: Vec<IncidentRow>) -> Board {
        Board {
            members,
            health: Ok(()),
            answered_by: Some(String::from("Answered by replica gateway-0.")),
            incidents: Ok(incidents),
            stored: Ok(3),
            revisions: Some(Ok(Revisions {
                running: RevisionId::new(4),
                pending: RevisionId::new(5),
            })),
        }
    }

    #[test]
    fn a_node_is_up_only_when_every_endpoint_of_it_is() {
        let members = [
            member("a-1", Some("node-a"), "up"),
            member("a-2", Some("node-a"), "down"),
            member("b-1", Some("node-b"), "up"),
            member("loose", None, "up"),
        ];
        assert_eq!(
            Nodes {
                up: 2,
                down: 1,
                total: 3
            },
            nodes(&members)
        );
    }

    #[test]
    fn the_dashboard_shows_each_count_the_health_and_the_pending_revision() {
        let html = drawn(&board(
            vec![member("node-a-pub", Some("node-a"), "up")],
            vec![IncidentRow {
                at: String::from("2026-10-05T08:00:00Z"),
                kind: String::from("EhrIdCollision"),
                description: String::from("synthetic"),
                resolved: false,
            }],
        ));
        assert!(html.contains("1 of 1"), "{html}");
        assert!(
            html.contains(r#"<span class="pill tone-ok">all up</span>"#),
            "{html}"
        );
        assert!(html.contains("revision 5 pending"), "{html}");
        assert!(html.contains("r4"), "{html}");
        assert!(
            html.contains(r#"<a href="/members/node-a">node-a-pub</a>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<span class="pill tone-danger">open</span>"#),
            "{html}"
        );
        assert!(html.contains("Answered by replica gateway-0."), "{html}");
    }

    #[test]
    fn a_part_the_gateway_refused_is_named_and_the_rest_still_shows() {
        let mut refused = board(vec![member("node-a-pub", Some("node-a"), "up")], Vec::new());
        refused.incidents = Err(String::from(
            "The gateway refused this part: 403 (scope-insufficient).",
        ));
        let html = drawn(&refused);
        assert!(
            html.contains("The gateway refused this part: 403 (scope-insufficient)."),
            "{html}"
        );
        assert!(html.contains("node-a-pub"), "{html}");
    }

    // The markup reaches the page through `inner_html`, so every value the
    // gateway names arrives escaped, in a text and in an attribute alike.
    #[test]
    fn hostile_values_render_escaped() {
        let mut hostile = member(HOSTILE, Some(HOSTILE), HOSTILE);
        hostile.latency_ms_p50 = None;
        let html = drawn(&board(
            vec![hostile],
            vec![IncidentRow {
                at: HOSTILE.to_owned(),
                kind: HOSTILE.to_owned(),
                description: HOSTILE.to_owned(),
                resolved: false,
            }],
        ));
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.contains("<img"), "{html}");
        assert!(
            html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
            "{html}"
        );
        assert!(
            !html.contains("href=\"/members/\""),
            "an id outside the registry's form gets no page: {html}"
        );
    }
}
