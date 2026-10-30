// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dashboard a signed-in operator lands on.
//!
//! It holds four stat cards (the organisations, the nodes reachable, the
//! open incidents and the running revision), the nodes that need attention,
//! the topology in brief, the open incidents and the configuration at a
//! glance.
//!
//! Summaries that link to each thing's one home; nothing is edited here.
//!
//! The self-description and the health are the dashboard's ground, so a
//! refusal of either is the page's. Every other part the gateway refuses
//! or cannot answer says so in its own card, and the rest still shows.

use ferrofed_registry::revision::Standing;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::tone::Tone;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::status::{badge, pill};
use ferrohealth_ui::view::surface::{
    Header, Stat, card_action, definitions, grid_start, page_heading, stack, stat_card_marked,
    stat_grid, titled_panel,
};
use ferrohealth_ui::view::tag::Tag;
use ferrohealth_ui::html::Html;

use crate::app::{ALL_NODES, CONFIGURATION, INTEGRITY, ORGANISATIONS, TOPOLOGY};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, opt, relative, stopped, t};
use crate::read::Stop;
use crate::read::federation::{Federation, Health};
use crate::read::incidents::Incidents;
use ferrohealth_ui::view::state::state;

/// What the dashboard reads of the gateway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reads {
    federation: Result<Federation, Stop>,
    incidents: Result<Incidents, Stop>,
    standings: Option<Vec<(String, Result<Standing, Stop>)>>,
    stored: Option<u64>,
}

/// Reads what the dashboard draws.
async fn reads() -> Reads {
    let federation = crate::read::federation::read().await;
    let incidents = crate::read::incidents::read().await;
    let mut standings = None;
    let mut stored = None;
    if let Ok((state, token)) = crate::read::signed_in() {
        let replicas = &state.settings().admin;
        if !replicas.is_empty() {
            let mut read = Vec::new();
            for replica in replicas {
                let standing = state
                    .gateway()
                    .config_standing(replica, &token)
                    .await
                    .map_err(Stop::from);
                read.push((replica.name.clone(), standing));
            }
            standings = Some(read);
        }
        stored = state
            .gateway()
            .stored_queries(&token, crate::read::stored_page(0, 1))
            .await
            .ok()
            .map(|page| page.total);
    }
    Reads {
        federation,
        incidents,
        standings,
        stored,
    }
}

/// The dashboard.
pub async fn page() -> Html {
    let reads = reads().await;
    crate::app::title(tr(Msg::DashboardTitle)).then(dashboard(reads))
}

/// The dashboard of `reads`.
fn dashboard(reads: Reads) -> Html {
    let replica = reads.federation.as_ref().ok().and_then(|federation| {
        crate::read::answered_by(&[("health", federation.replica.as_deref())])
    });
    let subtitle = match &replica {
        Some(replica) => tr_with(
            Msg::DashboardSubtitleReplica,
            &[("replica", replica.clone().into())],
        ),
        None => tr(Msg::DashboardSubtitle),
    };
    let heading = page_heading(
        &current(),
        Header {
            title: tr(Msg::DashboardTitle).into(),
            subtitle: Some(subtitle.into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let federation = match reads.federation {
        Ok(federation) => federation,
        Err(stop) => return heading.then(stopped(&stop)),
    };
    if federation.organisations.is_empty() {
        return heading.then(empty_federation());
    }
    let stats = stats(
        &federation,
        reads.incidents.as_ref().ok(),
        reads.standings.as_deref(),
    );
    let attention = attention(&federation);
    let topology = topology_card(&federation);
    let incidents = incidents_card(&reads.incidents);
    let configuration = configuration_card(reads.standings.as_deref(), reads.stored);
    heading.then(stack(Html::join([
        stats,
        grid_start(attention.then(topology)),
        grid_start(incidents.then(configuration)),
    ])))
}

/// A federation with no organisation: one quiet line with the way to
/// Organisations, and the order in which a federation is set up.
fn empty_federation() -> Html {
    h(format!(
        "<section class=\"fui-card\"><p>{} <a href=\"{ORGANISATIONS}\">{}</a></p>\
         <ol class=\"fui-small\"><li>{}</li><li>{}</li><li>{}</li></ol></section>",
        t(tr(Msg::DashboardEmpty)),
        t(tr(Msg::DashboardEmptyLink)),
        t(tr(Msg::DashboardSetupOrganisation)),
        t(tr(Msg::DashboardSetupCredentials)),
        t(tr(Msg::DashboardSetupQuery)),
    ))
}

/// The four stat cards.
fn stats(
    federation: &Federation,
    incidents: Option<&Incidents>,
    standings: Option<&[(String, Result<Standing, Stop>)]>,
) -> Html {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let nodes: Vec<_> = federation.nodes().collect();
    let endpoints: usize = federation
        .organisations
        .iter()
        .map(crate::read::federation::Organisation::endpoint_count)
        .sum();
    let taking_part: Vec<_> = nodes
        .iter()
        .filter(|(_, node)| node.health() != Health::Excluded)
        .collect();
    let reachable = taking_part
        .iter()
        .filter(|(_, node)| node.health() == Health::Reachable)
        .count();
    let worst = taking_part
        .iter()
        .filter(|(_, node)| node.health().is_problem())
        .max_by_key(|(_, node)| u8::from(node.health() == Health::Unreachable));
    let organisations = stat_card_marked(
        Stat {
            glyph: Glyph::BUILDING_COMPLEX,
            value: ferrohealth_ui::i18n::number::grouped(&current(), u64::from(count(
                federation.organisations.len(),
            ))),
            label: tr_with(
                Msg::StatOrganisations,
                &[("count", Arg::from(count(federation.organisations.len())))],
            )
            .into(),
            href: Some(String::from(ORGANISATIONS)),
        },
        badge(
            Tone::Neutral,
            tr_with(
                Msg::StatNodesEndpoints,
                &[
                    ("nodes", Arg::from(count(nodes.len()))),
                    ("endpoints", Arg::from(count(endpoints))),
                ],
            ),
        ),
    );
    let reach = reach_card(worst.map(|(_, node)| *node), reachable, taking_part.len());
    let incident_card = incident_card(incidents);
    let revision = revision_card(standings);
    stat_grid(Html::join([organisations, reach, incident_card, revision]))
}

/// The card of the nodes reachable, of the `total` that take part, marked
/// with the `worst` node where one is in trouble.
fn reach_card(
    worst: Option<&crate::read::federation::Node>,
    reachable: usize,
    total: usize,
) -> Html {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let mark = worst.map(|node| {
        badge(
            node.health().tone(),
            tr_with(
                Msg::StatWorstNode,
                &[
                    ("node", node.id.clone().into()),
                    ("state", node.health().word().into()),
                ],
            ),
        )
    });
    stat_card_marked(
        Stat {
            glyph: Glyph::HEART_PULSE,
            value: tr_with(
                Msg::StatOf,
                &[
                    ("count", Arg::from(count(reachable))),
                    ("total", Arg::from(count(total))),
                ],
            ),
            label: tr(Msg::StatNodesReachable).into(),
            href: Some(String::from(ALL_NODES)),
        },
        opt(mark),
    )
}

/// The card of the open incidents, marked with the `ehr_id`s they hold.
fn incident_card(incidents: Option<&Incidents>) -> Html {
    let open = incidents.map(|incidents| incidents.open.len());
    let held = incidents.map_or(0, |incidents| incidents.held().len());
    stat_card_marked(
        Stat {
            glyph: Glyph::SHIELD_CHECK,
            value: open.map_or_else(|| String::from("\u{2013}"), |open| open.to_string()),
            label: tr(Msg::StatOpenIncidents).into(),
            href: Some(String::from(INTEGRITY)),
        },
        opt((held > 0).then(|| {
            badge(
                Tone::Warn,
                tr_with(
                    Msg::StatHeld,
                    &[("count", Arg::from(u32::try_from(held).unwrap_or(u32::MAX)))],
                ),
            )
        })),
    )
}

/// The card of the running revision, marked with the replicas in step.
fn revision_card(standings: Option<&[(String, Result<Standing, Stop>)]>) -> Html {
    let running = standings
        .and_then(|standings| {
            standings
                .iter()
                .find_map(|(_, standing)| standing.as_ref().ok())
        })
        .and_then(|standing| standing.current.as_ref().map(|current| current.revision));
    let in_step = standings.map(|standings| {
        standings
            .iter()
            .filter(|(_, standing)| standing.as_ref().is_ok_and(|standing| standing.in_step))
            .count()
    });
    stat_card_marked(
        Stat {
            glyph: Glyph::FILE_TEXT,
            value: running.map_or_else(
                || tr(Msg::StatNoRevision),
                |revision| {
                    tr_with(
                        Msg::StatRevision,
                        &[("revision", revision.to_string().into())],
                    )
                },
            ),
            label: tr(Msg::StatRunningConfiguration).into(),
            href: Some(String::from(CONFIGURATION)),
        },
        opt(in_step.filter(|_| running.is_some()).map(|in_step| {
            pill(
                Tone::Ok,
                &tr_with(
                    Msg::StatInStep,
                    &[(
                        "count",
                        Arg::from(u32::try_from(in_step).unwrap_or(u32::MAX)),
                    )],
                ),
            )
        })),
    )
}

/// The nodes that need attention: each degraded, unreachable or excluded
/// node with why, leading to every node.
fn attention(federation: &Federation) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("node", tr(Msg::ColumnNode))
        },
        Column::text("organisation", tr(Msg::ColumnOrganisation)),
        Column::text("state", tr(Msg::ColumnState)),
        Column::text("why", tr(Msg::ColumnWhy)),
    ];
    let rows: Vec<Html> = federation
        .nodes()
        .filter(|(_, node)| {
            node.health() != Health::Reachable && node.health() != Health::NotProbed
        })
        .map(|(organisation, node)| {
            let why = tr(match node.health() {
                Health::Excluded => Msg::WhyExcluded,
                Health::Unreachable => Msg::WhyUnreachable,
                _ => Msg::WhyDegraded,
            });
            let href = node.routable().then(|| organisation.node_href(node));
            let class = if node.health() == Health::Excluded {
                " class=\"is-excluded\""
            } else {
                ""
            };
            h(format!(
                "<tr{class}>{}{}{}{}</tr>",
                title_cell(
                    href.as_deref(),
                    h(format!("<code>{}</code>", t(&node.id))),
                    None
                ),
                cell(&columns[1], t(organisation.shown())),
                cell(
                    &columns[2],
                    state(node.health().mark(), node.health().word(), None)
                ),
                cell(&columns[3], t(why)),
            ))
        })
        .collect();
    let body = if rows.is_empty() {
        h(format!(
            "<p class=\"fui-panel-body fui-muted\">{}</p>",
            t(tr(Msg::AttentionNone))
        ))
    } else {
        data_table(DataTable {
            caption: &tr(Msg::AttentionTitle),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(Html::join(rows)),
        })
    };
    titled_panel(
        tr(Msg::AttentionTitle),
        card_action(ALL_NODES, tr(Msg::AttentionAll)),
        body,
    )
}

/// The topology in brief: each organisation with the states of its nodes,
/// the services, and one line of counts, leading to the topology.
fn topology_card(federation: &Federation) -> Html {
    let rows = federation
        .organisations
        .iter()
        .map(|organisation| {
            let dots = organisation
                .nodes
                .iter()
                .map(|node| state(node.health().mark(), node.health().word(), None));
            let dots = Html::join(dots);
            let name = organisation.shown();
            let link = if organisation.routable() {
                h(format!("<a href=\"{}\">{}</a>", t(organisation.href()), t(name)))
            } else {
                t(name)
            };
            (link, h(format!("<span class=\"fui-cluster\">{dots}</span>")))
        })
        .collect::<Vec<_>>();
    let nodes: Vec<_> = federation.nodes().map(|(_, node)| node.health()).collect();
    let tally = |health: Health| {
        u32::try_from(nodes.iter().filter(|each| **each == health).count()).unwrap_or(u32::MAX)
    };
    let line = tr_with(
        Msg::TopologyTally,
        &[
            (
                "organisations",
                Arg::from(u32::try_from(federation.organisations.len()).unwrap_or(u32::MAX)),
            ),
            (
                "nodes",
                Arg::from(u32::try_from(nodes.len()).unwrap_or(u32::MAX)),
            ),
            ("reachable", Arg::from(tally(Health::Reachable))),
            ("degraded", Arg::from(tally(Health::Degraded))),
            ("unreachable", Arg::from(tally(Health::Unreachable))),
            ("excluded", Arg::from(tally(Health::Excluded))),
            (
                "services",
                Arg::from(u32::try_from(federation.services.len()).unwrap_or(u32::MAX)),
            ),
        ],
    );
    let body = h(format!(
        "<div class=\"fui-panel-body fui-stack\">{} <p class=\"fui-small fui-muted\">{}</p></div>",
        definitions(
            rows.into_iter()
                .map(|(name, dots)| (String::new(), h(format!("{name} {dots}"))))
                .collect::<Vec<_>>(),
        ),
        t(line)
    ));
    titled_panel(
        tr(Msg::TopologyTitle),
        card_action(TOPOLOGY, tr(Msg::TopologyTitle)),
        body,
    )
}

/// The open incidents, leading to Integrity.
fn incidents_card(incidents: &Result<Incidents, Stop>) -> Html {
    let body = match incidents {
        Err(stop) => h(format!("<div class=\"fui-panel-body\">{}</div>", stopped(stop))),
        Ok(incidents) if incidents.open.is_empty() => h(format!(
            "<p class=\"fui-panel-body fui-muted\">{}</p>",
            t(tr(Msg::IncidentsNoneOpen))
        )),
        Ok(incidents) => {
            let columns = [
                Column {
                    role: Role::Title,
                    ..Column::text("what", tr(Msg::ColumnWhatHappened))
                },
                Column::text("nodes", tr(Msg::ColumnNodes)),
                Column::text("detected", tr(Msg::ColumnDetected)),
            ];
            let rows = incidents
                .open
                .iter()
                .take(5)
                .map(|incident| {
                    let sub = h(format!("<code>{}</code>", t(&incident.kind)));
                    h(format!(
                        "<tr>{} {}{}</tr>",
                        title_cell(
                            None,
                            t(crate::pages::integrity::what(&incident.kind)),
                            Some(sub),
                        ),
                        cell(&columns[1], t(incident.nodes.join(", "))),
                        cell(&columns[2], relative(&incident.at)),
                    ))
                });
            data_table(DataTable {
                caption: &tr(Msg::IncidentsOpenTitle),
                columns: &columns,
                sorting: Sorting::default(),
                body: Body::Rows(Html::join(rows)),
            })
        }
    };
    titled_panel(
        tr(Msg::IncidentsOpenTitle),
        card_action(INTEGRITY, tr(Msg::NavIntegrity)),
        body,
    )
}

/// The configuration at a glance: the mode, the running revision, each
/// replica in step or not, and the stored queries.
fn configuration_card(
    standings: Option<&[(String, Result<Standing, Stop>)]>,
    stored: Option<u64>,
) -> Html {
    let mut rows: Vec<(String, Html)> = Vec::new();
    match standings {
        None => rows.push((
            tr(Msg::ConfigRevisions),
            h(format!(
                "<span class=\"fui-muted\">{}</span>",
                t(tr(Msg::ConfigNoAdmin))
            )),
        )),
        Some(standings) => {
            let running = standings
                .iter()
                .find_map(|(_, standing)| standing.as_ref().ok());
            rows.push((
                tr(Msg::ConfigRevisions),
                Tag::new(tr(Msg::ModeManaged)).glyph(Glyph::MONITOR).view(),
            ));
            if let Some(current) = running.and_then(|standing| standing.current.as_ref()) {
                rows.push((
                    tr(Msg::ConfigRunning),
                    t(tr_with(
                        Msg::ConfigRunningValue,
                        &[
                            ("revision", current.revision.to_string().into()),
                            (
                                "at",
                                current
                                    .applied_at
                                    .strftime("%Y-%m-%d %H:%M")
                                    .to_string()
                                    .into(),
                            ),
                            ("by", crate::pages::author(&current.applied_by).into()),
                        ],
                    )),
                ));
            }
            let replicas = standings
                .iter()
                .map(|(name, standing)| {
                    let mark = match standing {
                        Ok(standing) if standing.in_step => pill(Tone::Ok, &tr(Msg::InStep)),
                        Ok(_) => pill(Tone::Warn, &tr(Msg::NotInStep)),
                        Err(_) => pill(Tone::Bad, &tr(Msg::NoAnswer)),
                    };
                    h(format!("<code>{}</code> {mark} ", t(name)))
                });
            let replicas = Html::join(replicas);
            rows.push((
                tr(Msg::ConfigReplicas),
                h(format!("<span class=\"fui-cluster\">{replicas}</span>")),
            ));
        }
    }
    if let Some(stored) = stored {
        rows.push((
            tr(Msg::ConfigStored),
            t(tr_with(Msg::ConfigStoredValue, &[("count", Arg::from(stored))])),
        ));
    }
    titled_panel(
        tr(Msg::ConfigurationTitle),
        card_action(CONFIGURATION, tr(Msg::ConfigurationTitle)),
        h(format!("<div class=\"fui-panel-body\">{}</div>", definitions(rows))),
    )
}
