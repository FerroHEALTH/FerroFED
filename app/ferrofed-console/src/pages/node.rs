// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One node (T2), at `/organisations/{org}/{node}` with its Endpoints and
//! Routes tabs.
//!
//! Overview holds the node's facts and its health; Endpoints each endpoint
//! with its URL, its membership, its state and its median response; Routes
//! the creating systems whose follow-ups go to the node, from the gateway's
//! routing table (N21, §12.2), each with its ground: the node's own
//! `system_id`, a registered route or a learned one. A tab the console has
//! no data for is left out (T2).

use ferrofed_registry::operator::{CreatingSystemEntry, RouteSource};
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, card, facts_row};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::tag::Tag;
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::view::template::list::list_empty;
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;

use crate::app::ORGANISATIONS;
use crate::locale::{Msg, tr, tr_with};
use crate::pages::{median, owner_tag, stopped};
use crate::read::Stop;
use crate::read::federation::{Health, Node, Organisation};
use ferrohealth_ui::view::state::state;

/// The most pages of the routing table the Routes tab reads.
const ROUTE_PAGES: u64 = 10;

/// The tabs of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeTab {
    /// The facts and the health.
    Overview,
    /// The endpoints.
    Endpoints,
    /// The creating systems routed to the node.
    Routes,
}

impl NodeTab {
    /// The tab the path's `tab` names, `None` for one that names none.
    fn of(name: &str) -> Option<Self> {
        match name {
            "" => Some(Self::Overview),
            "endpoints" => Some(Self::Endpoints),
            "routes" => Some(Self::Routes),
            _ => None,
        }
    }

    /// Its key.
    const fn key(self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Endpoints => "endpoints",
            Self::Routes => "routes",
        }
    }
}

/// One node, on the tab the path names.
#[must_use]
pub fn page() -> AnyView {
    let org = crate::pages::param("org");
    let id = crate::pages::param("node");
    let Some(tab) = NodeTab::of(&crate::pages::param("tab")) else {
        return crate::pages::errors::not_found();
    };
    let body = Suspend::new(async move {
        let federation = match crate::read::federation::read().await {
            Ok(federation) => federation,
            Err(stop) => return stopped(&stop),
        };
        let Some((organisation, node)) = federation
            .node(&id)
            .filter(|(organisation, _)| organisation.id == org)
        else {
            return crate::pages::errors::not_found();
        };
        let routes = if tab == NodeTab::Routes {
            Some(routes_of(&node.id).await)
        } else {
            None
        };
        node_page(organisation, node, tab, routes)
    });
    view! { <Suspense fallback=|| ()>{body}</Suspense> }.into_any()
}

/// The creating systems the routing table routes to `node`.
async fn routes_of(node: &str) -> Result<Vec<CreatingSystemEntry>, Stop> {
    let (state, token) = crate::read::signed_in()?;
    let mut found = Vec::new();
    let mut offset = 0;
    for _ in 0..ROUTE_PAGES {
        let page = state
            .gateway()
            .creating_systems(
                &token,
                crate::read::stored_page(offset, crate::read::PAGE_SIZE),
            )
            .await?;
        let read = u64::try_from(page.items.len()).unwrap_or(u64::MAX);
        found.extend(
            page.items
                .into_iter()
                .filter(|entry| entry.node.as_deref() == Some(node)),
        );
        offset = offset.saturating_add(read);
        if read == 0 || offset >= page.total {
            break;
        }
    }
    Ok(found)
}

/// The page of `node` of `organisation` on `tab`.
fn node_page(
    organisation: &Organisation,
    node: &Node,
    tab: NodeTab,
    routes: Option<Result<Vec<CreatingSystemEntry>, Stop>>,
) -> AnyView {
    let href = organisation.node_href(node);
    let tabs = TabRow {
        label: node.id.clone(),
        tabs: vec![
            Tab::new("overview", href.clone(), tr(Msg::TabOverview)),
            Tab::new(
                "endpoints",
                format!("{href}/endpoints"),
                tr(Msg::TabEndpoints),
            )
            .count(
                u64::try_from(node.endpoints.len()).unwrap_or(u64::MAX),
                tr_with(
                    Msg::EndpointCount,
                    &[("count", Arg::from(node.endpoints.len()))],
                ),
            ),
            Tab::new("routes", format!("{href}/routes"), tr(Msg::TabRoutes)),
        ],
        current: String::from(tab.key()),
    };
    let actions = if crate::pages::configuration::console_manages() {
        ActionGroup::primary(Action::new(
            Some(Glyph::PENCIL),
            tr(Msg::EditNode),
            Act::Link(crate::pages::configuration::edit_member(&node.id)),
        ))
    } else {
        ActionGroup::none()
    };
    let body = match tab {
        NodeTab::Overview => overview(organisation, node),
        NodeTab::Endpoints => endpoints(node),
        NodeTab::Routes => routes_tab(node, routes),
    };
    view! {
        {crate::app::title(node.id.clone())}
        {detail_page(DetailPage {
            header: Header {
                title: node.id.clone().into(),
                subtitle: None,
                subtitle_view: Some(
                    crate::pages::marked(
                        Msg::NodeSubtitle,
                        &[],
                        vec![
                            (
                                "organisation",
                                view! {
                                    <a href=organisation
                                        .href()>{organisation.shown().to_owned()}</a>
                                }
                                    .into_any(),
                            ),
                        ],
                    ),
                ),
                crumbs: vec![
                    Crumb::new(tr(Msg::OrganisationsTitle), ORGANISATIONS),
                    Crumb::new(organisation.shown().to_owned(), organisation.href()),
                ],
                mono: true,
                badge: Some(state(node.health().mark(), node.health().word(), None)),
            },
            actions,
            stepper: None,
            tabs: Some(tabs),
            body,
        })}
    }
    .into_any()
}

/// The Overview tab: the node's facts and the state of each endpoint.
fn overview(organisation: &Organisation, node: &Node) -> AnyView {
    let facts = facts_row(vec![
        (
            tr(Msg::ColumnSystemId),
            view! { <code>{node.system_id.clone().unwrap_or_default()}</code> }.into_any(),
        ),
        (
            tr(Msg::ColumnCdr),
            node.product
                .clone()
                .unwrap_or_else(|| tr(Msg::None))
                .into_any(),
        ),
        (
            tr(Msg::ColumnOrganisation),
            view! { <a href=organisation.href()>{organisation.shown().to_owned()}</a> }.into_any(),
        ),
        (tr(Msg::ColumnMedian), median(node.latency_ms_p50())),
        (
            tr(Msg::ColumnOwner),
            node.owner
                .map_or_else(|| tr(Msg::None).into_any(), owner_tag),
        ),
    ]);
    let health = node
        .endpoints
        .iter()
        .map(|endpoint| {
            view! {
                <li>
                    <code>{endpoint.id.clone()}</code>
                    " "
                    {state(endpoint.health.mark(), endpoint.health.word(), None)}
                </li>
            }
        })
        .collect_view();
    view! {
        {card(tr(Msg::NodeFacts), facts)}
        {card(
            tr(Msg::NodeHealth),
            view! {
                <p class="fui-small fui-muted">{tr(Msg::NodeHealthLine)}</p>
                <ul class="fui-stack">{health}</ul>
            },
        )}
    }
    .into_any()
}

/// The Endpoints tab.
fn endpoints(node: &Node) -> AnyView {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("endpoint", tr(Msg::ColumnEndpoint))
        },
        Column {
            role: Role::Wide,
            ..Column::text("url", tr(Msg::ColumnUrl))
        },
        Column::text("membership", tr(Msg::ColumnMembership)),
        Column::text("state", tr(Msg::ColumnState)),
        Column {
            numeric: true,
            role: Role::Quiet,
            ..Column::text("median", tr(Msg::ColumnMedian)).titled(tr(Msg::ColumnMedianTitle))
        },
    ];
    let rows = node
        .endpoints
        .iter()
        .map(|endpoint| {
            view! {
                <tr class:is-excluded=endpoint.health
                    == Health::Excluded>
                    {title_cell(None, view! { <code>{endpoint.id.clone()}</code> }, None)}
                    {cell(
                        &columns[1],
                        view! { <code>{endpoint.url.clone().unwrap_or_default()}</code> },
                    )} {cell(&columns[2], Tag::new(endpoint.status.clone()).view())}
                    {cell(&columns[3], state(endpoint.health.mark(), endpoint.health.word(), None))}
                    {cell(&columns[4], median(endpoint.latency_ms_p50))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabEndpoints),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}

/// The words of a route's ground.
fn ground(source: RouteSource) -> String {
    tr(match source {
        RouteSource::Member => Msg::RouteOwn,
        RouteSource::Registered => Msg::RouteRegistered,
        RouteSource::Learned => Msg::RouteLearned,
        RouteSource::Withdrawn => Msg::RouteWithdrawn,
    })
}

/// The Routes tab: the creating systems the routing table routes to `node`.
fn routes_tab(node: &Node, routes: Option<Result<Vec<CreatingSystemEntry>, Stop>>) -> AnyView {
    let routes = match routes {
        Some(Ok(routes)) => routes,
        Some(Err(stop)) => return stopped(&stop),
        None => Vec::new(),
    };
    if routes.is_empty() {
        return list_empty(
            tr_with(Msg::RoutesNone, &[("node", node.id.clone().into())]),
            None,
        );
    }
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("system", tr(Msg::ColumnCreatingSystem))
        },
        Column::text("ground", tr(Msg::ColumnGround)),
        Column::text("endpoint", tr(Msg::ColumnEndpoint)),
    ];
    let rows = routes
        .into_iter()
        .map(|route| {
            view! {
                <tr class:is-excluded=route.source
                    == RouteSource::Withdrawn>
                    {title_cell(
                        None,
                        view! { <code>{route.creating_system_id.clone()}</code> },
                        None,
                    )} {cell(&columns[1], Tag::new(ground(route.source)).view())}
                    {cell(
                        &columns[2],
                        view! { <code>{route.endpoint.clone().unwrap_or_default()}</code> },
                    )}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    view! {
        <p class="fui-small fui-muted">{tr(Msg::RoutesLine)}</p>
        {data_table(DataTable {
            caption: &tr(Msg::TabRoutes),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows),
        })}
    }
    .into_any()
}
