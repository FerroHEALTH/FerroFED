// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One organisation (T2), at `/organisations/{org}` and its Nodes tab.
//!
//! The title carries its worst state; Overview holds its facts and its
//! nodes in brief, and Nodes every node with its `system_id`, its CDR, its
//! endpoints and its state, each row opening the node. The navigation
//! nests the organisation and its nodes under Organisations (F10). A tab
//! the console has no data for is left out (T2).

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::model::Noun;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, count_cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::state::state;
use ferrohealth_ui::view::surface::{Crumb, Header, card, facts_row, page_heading};
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::detail::{DetailPage, detail_page};

use crate::app::ORGANISATIONS;
use crate::locale::{Msg, current, message, tr, tr_with};
use crate::pages::{h, owner_tag, stopped, t};
use crate::read::federation::{Health, Organisation};

/// One organisation, on the tab the path names.
pub async fn page() -> Html {
    let id = crate::pages::param("org");
    let tab = crate::pages::param("tab");
    match crate::read::federation::read().await {
        Err(stop) => page_heading(
            &current(),
            Header {
                title: id.clone().into(),
                mono: true,
                crumbs: crumbs(),
                ..Header::default()
            },
            ActionGroup::none(),
        )
        .then(stopped(&stop)),
        Ok(federation) => match federation.organisation(&id) {
            Some(organisation) => organisation_page(organisation, &tab),
            None => crate::pages::errors::not_found(),
        },
    }
}

/// The trail of an organisation's page.
fn crumbs() -> Vec<Crumb> {
    vec![Crumb::new(tr(Msg::OrganisationsTitle), ORGANISATIONS)]
}

/// The page of `organisation` on the tab `tab` names.
fn organisation_page(organisation: &Organisation, tab: &str) -> Html {
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
    crate::app::title(organisation.shown().to_owned()).then(detail_page(
        &current(),
        DetailPage {
            header: Header {
                title: organisation.shown().to_owned().into(),
                subtitle: None,
                subtitle_view: Some(crate::pages::marked(
                    Msg::OrganisationSubtitle,
                    &[("count", Arg::from(count))],
                    vec![("id", h(format!("<code>{}</code>", t(&organisation.id))))],
                )),
                crumbs: crumbs(),
                mono: false,
                badge: Some(state(health.mark(), health.word(), None)),
            },
            actions,
            stepper: None,
            tabs: Some(tabs),
            body,
        },
    ))
}

/// The Overview tab: the organisation's facts and its nodes in brief.
fn overview(organisation: &Organisation) -> Html {
    let owner = organisation
        .nodes
        .iter()
        .find_map(|node| node.owner)
        .map_or_else(|| t(tr(Msg::None)), owner_tag);
    let facts = facts_row(vec![
        (
            tr(Msg::ColumnId),
            h(format!("<code>{}</code>", t(&organisation.id))),
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
            t(u64::try_from(organisation.nodes.len())
                .unwrap_or(u64::MAX)
                .to_string()),
        ),
        (
            tr(Msg::ColumnEndpoints),
            t(organisation.endpoint_count().to_string()),
        ),
        (tr(Msg::ColumnOwner), owner),
    ]);
    card(tr(Msg::OrganisationFacts), facts).then(nodes(organisation))
}

/// The organisation's nodes.
fn nodes(organisation: &Organisation) -> Html {
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
    let locale = current();
    let rows = organisation.nodes.iter().map(|node| {
        let href =
            (node.routable() && organisation.routable()).then(|| organisation.node_href(node));
        let class = if node.health() == Health::Excluded {
            " class=\"is-excluded\""
        } else {
            ""
        };
        h(format!(
            "<tr{class}>{}{} {}{} {}</tr>",
            title_cell(
                href.as_deref(),
                h(format!("<code>{}</code>", t(&node.id))),
                None
            ),
            cell(
                &columns[1],
                h(format!(
                    "<code>{}</code>",
                    t(node.system_id.clone().unwrap_or_default())
                )),
            ),
            cell(&columns[2], t(node.product.clone().unwrap_or_default())),
            count_cell(
                &locale,
                &columns[3],
                u32::try_from(node.endpoints.len()).unwrap_or(u32::MAX),
                Noun::new(message(Msg::NounEndpoints)),
            ),
            cell(
                &columns[4],
                state(node.health().mark(), node.health().word(), None)
            ),
        ))
    });
    data_table(DataTable {
        caption: &tr(Msg::TabNodes),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(Html::join(rows)),
    })
}
