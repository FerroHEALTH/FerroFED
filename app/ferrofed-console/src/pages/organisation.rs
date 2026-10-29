// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One organisation (T2), at `/organisations/{org}` and its Nodes tab.
//!
//! The title carries its worst state; Overview holds its facts and its
//! nodes in brief, and Nodes every node with its `system_id`, its CDR, its
//! endpoints and its state, each row opening the node. The navigation
//! nests the organisation and its nodes under Organisations (F10). A tab
//! the console has no data for is left out (T2).

use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, count_cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, card, facts_row, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};
use ferrohealth_ui::widget::action_menu::{Act, Action};
use leptos::prelude::*;

use crate::app::ORGANISATIONS;
use crate::locale::{Msg, message, tr, tr_with};
use crate::pages::{owner_tag, stopped};
use crate::read::federation::{Health, Organisation};
use ferrohealth_ui::view::state::state;

/// One organisation, on the tab the path names.
#[must_use]
pub fn page() -> AnyView {
    let id = crate::pages::param("org");
    let tab = crate::pages::param("tab");
    let federation_read = Resource::new(crate::read::changes, |_| {
        crate::read::federation::federation_now()
    });
    let body = Suspend::new(async move {
        match federation_read.await {
            Err(stop) => view! {
                {page_heading(
                    Header {
                        title: id.clone().into(),
                        mono: true,
                        crumbs: crumbs(),
                        ..Header::default()
                    },
                    ActionGroup::none(),
                )}
                {stopped(&stop)}
            }
            .into_any(),
            Ok(federation) => match federation.organisation(&id) {
                Some(organisation) => organisation_page(organisation, &tab),
                None => crate::pages::errors::not_found(),
            },
        }
    });
    view! { <Suspense fallback=|| ()>{body}</Suspense> }.into_any()
}

/// The trail of an organisation's page.
fn crumbs() -> Vec<Crumb> {
    vec![Crumb::new(tr(Msg::OrganisationsTitle), ORGANISATIONS)]
}

/// The page of `organisation` on the tab `tab` names.
fn organisation_page(organisation: &Organisation, tab: &str) -> AnyView {
    let href = organisation.href();
    let nodes_tab = tab == "nodes";
    let count = u64::try_from(organisation.nodes.len()).unwrap_or(u64::MAX);
    let tabs = TabRow {
        label: organisation.shown().to_owned(),
        tabs: vec![
            Tab::new("overview", href.clone(), tr(Msg::TabOverview)),
            Tab::new("nodes", format!("{href}/nodes"), tr(Msg::TabNodes)).count(
                count,
                tr_with(Msg::NodeCount, &[("count", Arg::from(count))]),
            ),
        ],
        current: String::from(if nodes_tab { "nodes" } else { "overview" }),
    };
    let health = organisation.health();
    let actions = if crate::pages::configuration::console_manages() {
        ActionGroup::primary(Action::new(
            Some(Glyph::PLUS),
            tr(Msg::NewNode),
            Act::Link(crate::pages::configuration::new_node(&organisation.id)),
        ))
    } else {
        ActionGroup::none()
    };
    let body = if nodes_tab {
        nodes(organisation)
    } else {
        overview(organisation)
    };
    view! {
        {crate::app::title(organisation.shown().to_owned())}
        {detail_page(DetailPage {
            header: Header {
                title: organisation.shown().to_owned().into(),
                subtitle: None,
                subtitle_view: Some(
                    crate::pages::marked(
                        Msg::OrganisationSubtitle,
                        &[("count", Arg::from(count))],
                        vec![("id", view! { <code>{organisation.id.clone()}</code> }.into_any())],
                    ),
                ),
                crumbs: crumbs(),
                mono: false,
                badge: Some(state(health.mark(), health.word(), None)),
            },
            actions,
            stepper: None,
            tabs: Some(tabs),
            body,
        })}
    }
    .into_any()
}

/// The Overview tab: the organisation's facts and its nodes in brief.
fn overview(organisation: &Organisation) -> AnyView {
    let owner = organisation
        .nodes
        .iter()
        .find_map(|node| node.owner)
        .map_or_else(|| tr(Msg::None).into_any(), owner_tag);
    let facts = facts_row(vec![
        (
            tr(Msg::ColumnId),
            view! { <code>{organisation.id.clone()}</code> }.into_any(),
        ),
        (
            tr(Msg::ColumnState),
            state(
                organisation.health().mark(),
                organisation.health().word(),
                None,
            ),
        ),
        (
            tr(Msg::ColumnNodes),
            u64::try_from(organisation.nodes.len())
                .unwrap_or(u64::MAX)
                .to_string()
                .into_any(),
        ),
        (
            tr(Msg::ColumnEndpoints),
            organisation.endpoint_count().to_string().into_any(),
        ),
        (tr(Msg::ColumnOwner), owner),
    ]);
    view! {
        {card(tr(Msg::OrganisationFacts), facts)}
        {nodes(organisation)}
    }
    .into_any()
}

/// The organisation's nodes.
fn nodes(organisation: &Organisation) -> AnyView {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("node", tr(Msg::ColumnNode))
        },
        Column {
            role: Role::Wide,
            ..Column::text("system", tr(Msg::ColumnSystemId))
        },
        Column::text("cdr", tr(Msg::ColumnCdr)),
        Column {
            numeric: true,
            role: Role::Quiet,
            ..Column::text("endpoints", tr(Msg::ColumnEndpoints))
        },
        Column::text("state", tr(Msg::ColumnState)),
    ];
    let rows = organisation
        .nodes
        .iter()
        .map(|node| {
            let href =
                (node.routable() && organisation.routable()).then(|| organisation.node_href(node));
            view! {
                <tr class:is-excluded=node.health()
                    == Health::Excluded>
                    {title_cell(href.as_deref(), view! { <code>{node.id.clone()}</code> }, None)}
                    {cell(
                        &columns[1],
                        view! { <code>{node.system_id.clone().unwrap_or_default()}</code> },
                    )} {cell(&columns[2], node.product.clone().unwrap_or_default())}
                    {count_cell(
                        &columns[3],
                        u32::try_from(node.endpoints.len()).unwrap_or(u32::MAX),
                        Noun::new(message(Msg::NounEndpoints)),
                    )} {cell(&columns[4], state(node.health().mark(), node.health().word(), None))}
                </tr>
            }
        })
        .collect_view()
        .into_any();
    data_table(DataTable {
        caption: &tr(Msg::TabNodes),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(rows),
    })
    .into_any()
}
