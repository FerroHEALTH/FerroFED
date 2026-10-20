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
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::filled_columns;
use ferrohealth_ui::table::view::{Column, NUM};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::empty::empty_quiet;
use ferrohealth_ui::view::layout::main_aside;
use ferrohealth_ui::view::notice::{Announce, notice};
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::{
    Stat, card_action, panel_body, stat_card_marked, stat_grid, titled_panel,
};
use ferrohealth_ui::view::time::timestamp;
use leptos::either::Either;
use leptos::prelude::*;

use crate::gateway::{AccessToken, GatewayError};
use crate::locale::{Msg, tr, tr_with};
use crate::server::ViewerState;
use crate::views::load::server::{self as load, Stop};
use crate::views::model::{IncidentRow, MemberRow, Refusal};
use crate::views::{replica_line, state_pill};

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
        Stop::Refused(Refusal::Gateway { status, code }) => tr_with(
            Msg::GatewayRefusedPart,
            &[
                ("status", (status).to_string().into()),
                (
                    "code",
                    (code.as_deref().unwrap_or(&tr(Msg::NoCode)))
                        .to_string()
                        .into(),
                ),
            ],
        ),
        Stop::Refused(Refusal::NotAuthenticated { code }) => tr_with(
            Msg::GatewayDidNotAcceptYourSign,
            &[(
                "code",
                (code.as_deref().unwrap_or(&tr(Msg::NoCode)))
                    .to_string()
                    .into(),
            )],
        ),
        Stop::Refused(Refusal::Invalid { reason }) => reason,
        Stop::Fault(fault) => tr_with(Msg::NotShown2, &[("fault", (fault).to_string().into())]),
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
        (Tone::Neutral, tr(Msg::NotReported))
    } else if nodes.total == 0 {
        (Tone::Neutral, tr(Msg::NoNodes))
    } else if nodes.up == nodes.total {
        (Tone::Ok, tr(Msg::AllUp))
    } else if nodes.down > 0 {
        (
            Tone::Bad,
            tr_with(Msg::Down, &[("down", (nodes.down).to_string().into())]),
        )
    } else {
        (Tone::Warn, tr(Msg::NotYetObserved))
    }
}

/// The dashboard's markup.
#[must_use]
pub fn render(board: &Board) -> String {
    let stats = stats(board);
    let health = health_panel(board);
    let incidents = incidents_panel(&board.incidents);
    view! {
        {replica_line(board.answered_by.clone())}
        <div class="fui-stack">
            {stat_grid(stats)}
            {main_aside(health, incidents)}
        </div>
    }
    .to_html()
}

/// The badge of a stat whose part was not shown.
fn unknown_mark() -> impl IntoView {
    badge(Tone::Neutral, &tr(Msg::NotShown))
}

/// A stat card of `value` that `label` names, beside `glyph`, leading to
/// `href`, with `mark` after the label.
fn stat(
    glyph: Glyph,
    value: String,
    label: String,
    href: &'static str,
    mark: impl IntoView + 'static,
) -> AnyView {
    stat_card_marked(
        Stat {
            glyph,
            value,
            label: label.into(),
            href: Some(String::from(href)),
        },
        mark,
    )
    .into_any()
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
    let members = stat(
        Glyph::USERS,
        tr_with(
            Msg::Of,
            &[
                ("active", (active).to_string().into()),
                ("total", (total).to_string().into()),
            ],
        ),
        tr(Msg::MembersActive),
        crate::views::MEMBERS,
        (),
    );
    let standing = nodes(&board.members);
    let (tone, word) = nodes_mark(standing, board.health.is_ok());
    let nodes = stat(
        Glyph::HEART_PULSE,
        if board.health.is_ok() {
            tr_with(
                Msg::Of2,
                &[
                    ("up", (standing.up).to_string().into()),
                    ("total", (standing.total).to_string().into()),
                ],
            )
        } else {
            String::from("n/a")
        },
        tr(Msg::NodesUp),
        crate::views::MEMBERS,
        pill(tone, &word),
    );
    let open = match &board.incidents {
        Ok(rows) => stat(
            Glyph::SHIELD_OFF,
            rows.iter().filter(|row| !row.resolved).count().to_string(),
            tr(Msg::OpenIncidents),
            crate::views::INTEGRITY,
            (),
        ),
        Err(_) => stat(
            Glyph::SHIELD_OFF,
            String::from("n/a"),
            tr(Msg::OpenIncidents),
            crate::views::INTEGRITY,
            unknown_mark(),
        ),
    };
    let stored = match &board.stored {
        Ok(total) => stat(
            Glyph::DATABASE,
            total.to_string(),
            tr(Msg::StoredQueries),
            crate::views::STORED_QUERIES,
            (),
        ),
        Err(_) => stat(
            Glyph::DATABASE,
            String::from("n/a"),
            tr(Msg::StoredQueries),
            crate::views::STORED_QUERIES,
            unknown_mark(),
        ),
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
fn revision_stat(revisions: &Part<Revisions>) -> AnyView {
    let (value, mark) = match revisions {
        Ok(revisions) => (
            revisions
                .running
                .map_or_else(|| String::from("none"), |running| format!("r{running}")),
            revisions.pending.map(|pending| {
                badge(
                    Tone::Warn,
                    &tr_with(
                        Msg::RevisionPending,
                        &[("pending", (pending).to_string().into())],
                    ),
                )
            }),
        ),
        Err(_) => (String::from("n/a"), None),
    };
    let shown = revisions.is_ok();
    stat(
        Glyph::SETTINGS,
        value,
        tr(Msg::RunningRevision),
        crate::configure::CONFIGURATION,
        view! {
            {mark}
            {(!shown).then(unknown_mark)}
        },
    )
}

/// A panel titled `title` holding `body`, its header action leading to
/// `href` with the words `more`.
fn panel(
    title: String,
    (href, more): (&'static str, String),
    body: impl IntoView + 'static,
) -> impl IntoView {
    titled_panel(title, card_action(href, more), body)
}

/// The notice of a part the gateway did not answer.
fn not_answered(line: &str) -> impl IntoView + use<> {
    panel_body(notice(Tone::Bad, Announce::Alert, None, line.to_owned()))
}

/// The columns of the member health table.
fn health_columns() -> [Column; 4] {
    [
        Column::text("endpoint", tr(Msg::Endpoint)),
        Column::text("node", tr(Msg::Node)),
        Column::text("health", tr(Msg::Health)),
        Column::text("latency", tr(Msg::MedianLatency)).numeric(),
    ]
}

/// The columns of the recent incidents table.
fn incident_columns() -> [Column; 3] {
    [
        Column::text("at", tr(Msg::At)),
        Column::text("kind", tr(Msg::Kind)),
        Column::text("state", tr(Msg::State)),
    ]
}

/// The member health panel: each endpoint, its node, its health and its
/// median latency.
fn health_panel(board: &Board) -> impl IntoView + use<> {
    let notice = board.health.as_ref().err().map(|line| not_answered(line));
    let columns = filled_columns(&health_columns(), &board.members, &[], |member, key| {
        key != "latency" || member.latency_ms_p50.is_some()
    });
    let timed = crate::views::has(&columns, "latency");
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
                    <th scope="row" class="fui-mono">
                        {name}
                    </th>
                    <td data-label=tr(Msg::Node) class="fui-mono">
                        {member.node_id.clone().unwrap_or_default()}
                    </td>
                    <td data-label=tr(Msg::Health)>{state_pill(&member.health)}</td>
                    {timed
                        .then(|| {
                            view! {
                                <td data-label=tr(Msg::MedianLatency) class=NUM>
                                    {member
                                        .latency_ms_p50
                                        .map(|latency| tr_with(Msg::Ms, &[("latency", (latency).to_string().into())]))
                                        .unwrap_or_default()}
                                </td>
                            }
                        })}
                </tr>
            }
        })
        .collect_view();
    let table = crate::views::table(&tr(Msg::MemberHealth), &columns, rows.into_any());
    let body = if board.members.is_empty() {
        Either::Left(panel_body(empty_quiet(
            Glyph::USERS,
            tr(Msg::NoMembers),
            tr(Msg::GatewayDeclaresNoMemberEndpoint),
        )))
    } else {
        Either::Right(table)
    };
    panel(
        tr(Msg::MemberHealth),
        (crate::views::MEMBERS, tr(Msg::AllMembers)),
        view! {
            {notice}
            {body}
        },
    )
}

/// The recent incidents panel: the newest few, each with its state.
fn incidents_panel(incidents: &Part<Vec<IncidentRow>>) -> impl IntoView + use<> {
    let body = match incidents {
        Err(line) => not_answered(line).into_any(),
        Ok(rows) if rows.is_empty() => panel_body(empty_quiet(
            Glyph::SHIELD_CHECK,
            tr(Msg::NoIncidents),
            tr(Msg::NoneSinceGatewayStarted),
        ))
        .into_any(),
        Ok(rows) => {
            let rows = rows
                .iter()
                .take(RECENT)
                .map(|row| {
                    let (tone, word) = if row.resolved {
                        (Tone::Ok, tr(Msg::StateResolved))
                    } else {
                        (Tone::Bad, tr(Msg::StateOpen))
                    };
                    view! {
                        <tr>
                            <td data-label=tr(Msg::At)>
                                {timestamp(&row.at)}
                            </td>
                            <th scope="row">{row.kind.clone()}</th>
                            <td data-label=tr(Msg::State)>{pill(tone, &word)}</td>
                        </tr>
                    }
                })
                .collect_view();
            crate::views::table(
                &tr(Msg::RecentIncidents),
                &incident_columns(),
                rows.into_any(),
            )
        }
    };
    panel(
        tr(Msg::RecentIncidents),
        (crate::views::INTEGRITY, tr(Msg::AllIncidents)),
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
    fn the_latency_column_shows_only_when_a_member_reports_a_latency() {
        let timed = drawn(&board(
            vec![member("node-a-pub", Some("node-a"), "up")],
            Vec::new(),
        ));
        assert!(timed.contains("Median latency"), "{timed}");
        assert!(timed.contains("12 ms"), "{timed}");
        let mut quiet = member("node-a-pub", Some("node-a"), "unknown");
        quiet.latency_ms_p50 = None;
        let untimed = drawn(&board(vec![quiet], Vec::new()));
        assert!(!untimed.contains("Median latency"), "{untimed}");
        assert!(!untimed.contains(r#"<td class="fui-num">"#), "{untimed}");
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
            html.contains(r#"<span class="fui-pill fui-tone-ok">all up</span>"#),
            "{html}"
        );
        assert!(html.contains("revision 5 pending"), "{html}");
        assert!(html.contains("r4"), "{html}");
        assert!(
            html.contains(r#"<a href="/members/node-a">node-a-pub</a>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<span class="fui-pill fui-tone-bad">open</span>"#),
            "{html}"
        );
        assert!(
            html.contains(r#"<time datetime="2026-10-05T08:00:00Z" title="2026-10-05T08:00:00Z" class="fui-time">2026-10-05 08:00:00 UTC</time>"#),
            "the incident time reads in UTC: {html}"
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
