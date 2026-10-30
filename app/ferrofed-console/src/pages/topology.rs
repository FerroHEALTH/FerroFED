// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The topology (F9): the gateway at the hub, the services it asks on its
//! left and the organisations with their nodes on its right, each edge's
//! line and mark saying its state, with the List twin.
//!
//! The query names the drawing's state: `view` (`map` or `list`), `look`
//! (the card whose quick look is open), `collapse` (`all` or the groups
//! folded), `show` (the states shown) and `minimap`. Each is a link or a
//! `GET` form, so the topology works with the scripts refused; FerroUI's
//! `topology` module adds zoom, panning and the keys. Past eight organisations the healthy
//! ones start folded.

use std::collections::BTreeSet;

use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::flow::{EdgeRule, FlowRule, FlowState, rule_button, rule_dialog};
use ferrohealth_ui::view::surface::{Header, facts_row, page_heading};
use ferrohealth_ui::view::topology::{
    ClusterLink, QuickLook, StatusCount, StatusMenu, Topology, TopologyCard, TopologyGroup,
    TopologySide, TopologyView, topology,
};
use ferrohealth_ui::html::Html;

use crate::app::{SELF_DESCRIPTION, TOPOLOGY};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, latency_fact, stopped, t};
use crate::read::federation::{Federation, Health, Node, Organisation, ServiceGroup};
use ferrohealth_ui::view::state::state;

/// Every state of the Status menu, in its order.
const STATES: [FlowState; 5] = [
    FlowState::Up,
    FlowState::Degraded,
    FlowState::Unreachable,
    FlowState::NotChecked,
    FlowState::Off,
];

/// The id of the dialog that says how the edges are drawn.
const RULE_DIALOG: &str = "topology-rule";

/// How many organisations a map shows unfolded before the healthy ones
/// start folded.
const UNFOLDED: usize = 8;

/// The state of the drawing the query names.
#[derive(Debug, Clone, Default)]
struct Asked {
    view: TopologyView,
    look: Option<String>,
    collapse: BTreeSet<String>,
    all_folded: bool,
    expanded: BTreeSet<String>,
    show: Option<BTreeSet<String>>,
    minimap: bool,
}

impl Asked {
    /// The query of this state with `change` applied, as a link's.
    fn href(&self, change: impl FnOnce(&mut Self)) -> String {
        let mut next = self.clone();
        change(&mut next);
        let mut parts = Vec::new();
        match next.view {
            TopologyView::Auto => {}
            TopologyView::Map => parts.push(String::from("view=map")),
            TopologyView::List => parts.push(String::from("view=list")),
        }
        if let Some(look) = &next.look {
            parts.push(format!("look={look}"));
        }
        if next.all_folded {
            parts.push(String::from("collapse=all"));
        } else if !next.collapse.is_empty() {
            parts.push(format!(
                "collapse={}",
                next.collapse.iter().cloned().collect::<Vec<_>>().join(",")
            ));
        }
        if !next.expanded.is_empty() {
            parts.push(format!(
                "expand={}",
                next.expanded.iter().cloned().collect::<Vec<_>>().join(",")
            ));
        }
        if let Some(show) = &next.show {
            parts.extend(show.iter().map(|state| format!("show={state}")));
        }
        if next.minimap {
            parts.push(String::from("minimap=on"));
        }
        if parts.is_empty() {
            String::from(TOPOLOGY)
        } else {
            format!("{TOPOLOGY}?{}", parts.join("&"))
        }
    }

    /// Whether a card in `state` is shown.
    fn shows(&self, state: FlowState) -> bool {
        self.show
            .as_ref()
            .is_none_or(|show| show.contains(state.key()))
    }
}

/// The state the request's query names.
fn asked() -> Asked {
    use crate::pages::{query_param, query_values};
    let list = |key: &str| -> BTreeSet<String> {
        query_param(key)
            .unwrap_or_default()
            .split(',')
            .filter(|id| !id.is_empty() && *id != "all")
            .map(str::to_owned)
            .collect()
    };
    let shown = query_values("show");
    Asked {
        view: match query_param("view").as_deref() {
            Some("map") => TopologyView::Map,
            Some("list") => TopologyView::List,
            _ => TopologyView::Auto,
        },
        look: query_param("look"),
        all_folded: query_param("collapse").as_deref() == Some("all"),
        collapse: list("collapse"),
        expanded: list("expand"),
        show: (!shown.is_empty()).then(|| shown.into_iter().collect()),
        minimap: query_param("minimap").as_deref() == Some("on"),
    }
}

/// The topology.
pub async fn page() -> Html {
    let asked = asked();
    let federation = crate::read::federation::read().await;
    let heading = page_heading(
        &current(),
        Header {
            title: tr(Msg::TopologyTitle).into(),
            subtitle: Some(tr(Msg::TopologySubtitle).into()),
            ..Header::default()
        },
        ActionGroup::none(),
    );
    let body = match federation {
        Err(stop) => Html::join([
            heading,
            stopped(&stop),
            h(format!(
                "<p class=\"fui-small fui-muted\">{}</p>",
                t(tr(Msg::TopologyNoStale))
            )),
        ]),
        Ok(federation) => Html::join([heading, map(&federation, &asked), edge_rule()]),
    };
    crate::app::title(tr(Msg::TopologyTitle)).then(body)
}

/// "How the flow is drawn": each state's line and what it means on the
/// map, in FerroUI's short dialog that the toolbar's button opens.
fn edge_rule() -> Html {
    let rule = |state, meaning| EdgeRule {
        state,
        meaning: tr(meaning),
    };
    rule_dialog(
        &current(),
        FlowRule {
        id: RULE_DIALOG.into(),
        rules: vec![
            rule(FlowState::Up, Msg::TopologyRuleUp),
            rule(FlowState::Degraded, Msg::TopologyRuleDegraded),
            rule(FlowState::Unreachable, Msg::TopologyRuleUnreachable),
            rule(FlowState::NotChecked, Msg::TopologyRuleNotChecked),
            rule(FlowState::Off, Msg::TopologyRuleOff),
        ],
        note: tr(Msg::TopologyRuleNote),
        },
    )
}

/// A card of the map.
fn card(
    id: String,
    glyph: Glyph,
    name: String,
    line: String,
    health: Health,
    href: String,
    asked: &Asked,
) -> TopologyCard {
    let look = asked.href(|next| next.look = Some(id.clone()));
    TopologyCard {
        id,
        glyph,
        name,
        line,
        state: health.flow(),
        href,
        mono: false,
        edge_label: None,
        look: Some(look),
        measure: None,
        note: None,
        action: None,
    }
}

/// Whether the group `id`, healthy or not as `quiet` says, is folded.
fn folded(asked: &Asked, id: &str, quiet: bool, many: bool) -> bool {
    asked.all_folded
        || asked.collapse.contains(id)
        || (many && quiet && !asked.expanded.contains(id))
}

/// The toggle that folds or unfolds the group `id`.
fn toggle(asked: &Asked, id: &str, collapsed: bool, by_default: bool) -> String {
    asked.href(|next| {
        next.all_folded = false;
        if collapsed {
            next.collapse.remove(id);
            if by_default {
                next.expanded.insert(id.to_owned());
            }
        } else {
            next.expanded.remove(id);
            next.collapse.insert(id.to_owned());
        }
    })
}

/// The node card of `node` of `organisation`.
fn node_card(organisation: &Organisation, node: &Node, asked: &Asked) -> TopologyCard {
    let health = node.health();
    TopologyCard {
        mono: true,
        measure: match (node.latency_ms_p50(), node.latency_ms_p95()) {
            (Some(p50), Some(p95)) => Some(tr_with(
                Msg::TopologyLatency,
                &[("p50", p50.into()), ("p95", p95.into())],
            )),
            (Some(p50), None) => Some(tr_with(Msg::TopologyMedian, &[("count", p50.into())])),
            (None, _) => None,
        },
        edge_label: health.is_problem().then(|| health.word()),
        ..card(
            node.id.clone(),
            Glyph::SERVER,
            node.id.clone(),
            health.word(),
            health,
            organisation.node_href(node),
            asked,
        )
    }
}

/// The group of `organisation`, its nodes those whose state `asked` shows.
fn organisation_group(
    organisation: &Organisation,
    asked: &Asked,
    many: bool,
) -> Option<TopologyGroup> {
    let children: Vec<TopologyCard> = organisation
        .nodes
        .iter()
        .filter(|node| node.routable() && asked.shows(node.health().flow()))
        .map(|node| node_card(organisation, node, asked))
        .collect();
    if children.is_empty() || !organisation.routable() {
        return None;
    }
    let health = organisation.health();
    let quiet = !health.is_problem();
    let collapsed = folded(asked, &organisation.id, quiet, many);
    let label = children
        .iter()
        .find(|child| child.state.is_problem())
        .and_then(|child| child.edge_label.clone());
    let count = u32::try_from(organisation.nodes.len()).unwrap_or(u32::MAX);
    Some(TopologyGroup {
        card: TopologyCard {
            edge_label: label,
            ..card(
                organisation.id.clone(),
                Glyph::BUILDING_COMPLEX,
                organisation.shown().to_owned(),
                tr_with(
                    Msg::TopologyOrgLine,
                    &[("count", count.into()), ("state", health.word().into())],
                ),
                health,
                organisation.href(),
                asked,
            )
        },
        children,
        collapsed,
        toggle: Some(toggle(asked, &organisation.id, collapsed, many && quiet)),
    })
}

/// The service groups on the hub's left.
fn services(federation: &Federation, asked: &Asked) -> Option<TopologySide> {
    let mut groups = Vec::new();
    for group in [
        ServiceGroup::Identity,
        ServiceGroup::Localization,
        ServiceGroup::Consent,
        ServiceGroup::Directory,
        ServiceGroup::Audit,
    ] {
        let children: Vec<TopologyCard> = federation
            .services
            .iter()
            .filter(|service| service.group == group && asked.shows(service.health.flow()))
            .map(|service| TopologyCard {
                edge_label: service.health.is_problem().then(|| service.health.word()),
                ..card(
                    service.key.clone(),
                    Glyph::PLUG,
                    service.name(),
                    service.health.word(),
                    service.health,
                    service.href(),
                    asked,
                )
            })
            .collect();
        if children.is_empty() {
            continue;
        }
        let health = Health::worst(
            federation
                .services
                .iter()
                .filter(|service| service.group == group && asked.shows(service.health.flow()))
                .map(|service| service.health),
        );
        let collapsed = folded(asked, group.key(), false, false);
        groups.push(TopologyGroup {
            card: card(
                String::from(group.key()),
                Glyph::LAYERS,
                group.title(),
                health.word(),
                health,
                String::from(crate::app::SERVICES),
                asked,
            ),
            children,
            collapsed,
            toggle: Some(toggle(asked, group.key(), collapsed, false)),
        });
    }
    (!groups.is_empty()).then(|| TopologySide {
        title: tr(Msg::TopologyServices),
        line: tr(Msg::TopologyServicesLine),
        groups,
    })
}

/// The quick look of the card `id`, when it is a node or an organisation.
fn quick_look(federation: &Federation, id: &str, asked: &Asked) -> Option<QuickLook> {
    let close = asked.href(|next| next.look = None);
    if let Some((organisation, node)) = federation.node(id) {
        let endpoints = node
            .endpoints
            .iter()
            .map(|endpoint| {
                h(format!(
                    "<li><code>{}</code> {}</li>",
                    t(&endpoint.id),
                    state(endpoint.health.mark(), endpoint.health.word(), None)
                ))
            });
        let endpoints = Html::join(endpoints);
        return Some(QuickLook {
            card: id.to_owned(),
            title: node.id.clone(),
            mono: true,
            line: Some(tr_with(
                Msg::NodeSubtitle,
                &[("organisation", organisation.shown().to_owned().into())],
            )),
            close,
            body: h(format!(
                "{}<ul class=\"fui-stack\">{endpoints}</ul><p><a href=\"{}\">{}</a></p>",
                facts_row(vec![
                    (
                        tr(Msg::ColumnState),
                        state(node.health().mark(), node.health().word(), None),
                    ),
                    (
                        tr(Msg::ColumnMedian),
                        latency_fact(node.latency_ms_p50(), tr(Msg::ColumnMedianTitle)),
                    ),
                    (
                        tr(Msg::ColumnP95),
                        latency_fact(node.latency_ms_p95(), tr(Msg::ColumnP95Title)),
                    ),
                ]),
                t(organisation.node_href(node)),
                t(tr(Msg::TopologyOpenPage)),
            )),
        });
    }
    federation.organisation(id).map(|organisation| QuickLook {
        card: id.to_owned(),
        title: organisation.shown().to_owned(),
        mono: false,
        line: Some(organisation.id.clone()),
        close,
        body: h(format!(
            "{}<p><a href=\"{}\">{}</a></p>",
            facts_row(vec![(
                tr(Msg::ColumnState),
                state(organisation.health().mark(), organisation.health().word(), None),
            )]),
            t(organisation.href()),
            t(tr(Msg::TopologyOpenPage)),
        )),
    })
}

/// The map of `federation` in the state `asked`.
fn map(federation: &Federation, asked: &Asked) -> Html {
    let many = federation.organisations.len() > UNFOLDED;
    let groups: Vec<TopologyGroup> = federation
        .organisations
        .iter()
        .filter_map(|organisation| organisation_group(organisation, asked, many))
        .collect();
    let states: Vec<FlowState> = federation
        .nodes()
        .map(|(_, node)| node.health().flow())
        .chain(
            federation
                .services
                .iter()
                .map(|service| service.health.flow()),
        )
        .collect();
    let counted = |state: FlowState| {
        u32::try_from(states.iter().filter(|each| **each == state).count()).unwrap_or(u32::MAX)
    };
    let hub = TopologyCard {
        look: None,
        ..card(
            String::from("gateway"),
            Glyph::NETWORK,
            tr(Msg::TopologyGateway),
            federation.id.clone(),
            Health::Reachable,
            String::from(SELF_DESCRIPTION),
            asked,
        )
    };
    let locale = current();
    topology(
        &locale,
        Topology {
        id: String::from("federation-map"),
        label: tr(Msg::TopologyMapLabel),
        list_label: tr(Msg::TopologyListLabel),
        hub,
        callers: None,
        start: services(federation, asked),
        end: Some(TopologySide {
            title: tr(Msg::TopologyOrganisations),
            line: tr(Msg::TopologyOrganisationsLine),
            groups,
        }),
        view: asked.view,
        views: (
            asked.href(|next| next.view = TopologyView::Map),
            asked.href(|next| next.view = TopologyView::List),
        ),
        status: StatusMenu {
            action: String::from(TOPOLOGY),
            name: String::from("show"),
            states: STATES
                .into_iter()
                .map(|state| StatusCount {
                    state,
                    count: counted(state),
                    shown: asked.shows(state),
                })
                .collect(),
        },
        tools: Some(rule_button(&locale, RULE_DIALOG)),
        updated: None,
        collapse_all: Some(ClusterLink {
            href: asked.href(|next| {
                next.all_folded = !next.all_folded;
                next.collapse.clear();
                next.expanded.clear();
            }),
            on: asked.all_folded,
        }),
        minimap: Some(ClusterLink {
            href: asked.href(|next| next.minimap = !next.minimap),
            on: asked.minimap,
        }),
        look: asked
            .look
            .as_deref()
            .and_then(|id| quick_look(federation, id, asked)),
        },
    )
}
