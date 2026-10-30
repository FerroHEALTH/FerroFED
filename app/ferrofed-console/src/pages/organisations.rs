// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Organisations, the home of the federation's members (T1).
//!
//! Binder tabs hold the two views: each organisation with its node count,
//! its worst node and its endpoints, and every node across the
//! organisations, worst first, with its median and 95th percentile response. An excluded
//! organisation or node is a muted row, and the row opens its page. Past
//! ten rows the find field shows, and past one page the pager; the words
//! and the page sit in the query, since they name no patient. New
//! organisation leads to the console's form where the configuration is
//! managed.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::icon::Glyph;
use ferrohealth_ui::table::filter::{Find, find_form};
use ferrohealth_ui::table::model::{Noun, Paging, StateSource, Window};
use ferrohealth_ui::table::pager::{Listing, link_pager};
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, count_cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::action_menu::{Act, Action};
use ferrohealth_ui::view::surface::Header;
use ferrohealth_ui::view::tabs::{Tab, TabRow};
use ferrohealth_ui::view::template::list::{
    ActiveFilters, FilterChip, HeadingCount, ListCard, ListPage, list_empty, list_page,
};

use crate::app::{ALL_NODES, ORGANISATIONS};
use crate::locale::{Msg, current, message, tr, tr_with};
use crate::pages::{h, stopped, t};
use crate::read::federation::{Federation, Health, Node, Organisation};
use ferrohealth_ui::view::state::state;

/// Which of the two views the page shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    /// The organisations.
    Organisations,
    /// Every node across them.
    Nodes,
}

/// What the query asks of the listing: the words to find and the page.
#[derive(Debug, Clone)]
struct Asked {
    words: Option<String>,
    paging: Paging,
}

/// The listing's state the request's query names.
fn asked() -> Asked {
    Asked {
        words: crate::pages::query_param("q")
            .map(|words| words.trim().to_owned())
            .filter(|words| !words.is_empty()),
        paging: Paging::read(
            crate::pages::query_param("page").as_deref(),
            crate::pages::query_param("size").as_deref(),
        ),
    }
}

/// The organisations.
pub async fn list() -> Html {
    page(View::Organisations).await
}

/// Every node across the organisations.
pub async fn all_nodes() -> Html {
    page(View::Nodes).await
}

/// The page of `view_of`.
async fn page(view_of: View) -> Html {
    let asked = asked();
    let managed = crate::pages::configuration::console_manages();
    let body = match crate::read::federation::read().await {
        Ok(federation) => listing(&federation, view_of, &asked, managed),
        Err(stop) => heading_only(managed).then(stopped(&stop)),
    };
    let title = match view_of {
        View::Organisations => Msg::OrganisationsTitle,
        View::Nodes => Msg::AllNodes,
    };
    crate::app::title(tr(title)).then(body)
}

/// The page's header.
fn header() -> Header {
    Header {
        title: tr(Msg::OrganisationsTitle).into(),
        subtitle: Some(tr(Msg::OrganisationsSubtitle).into()),
        ..Header::default()
    }
}

/// The title row's actions: New organisation where the console manages
/// the configuration.
fn actions(managed: bool) -> ActionGroup {
    if managed {
        ActionGroup::primary(Action::new(
            Some(Glyph::PLUS),
            tr(Msg::NewOrganisation),
            Act::Link(String::from(crate::pages::configuration::NEW_MEMBER)),
        ))
    } else {
        ActionGroup::none()
    }
}

/// The header and actions alone, over a refusal.
fn heading_only(managed: bool) -> Html {
    ferrohealth_ui::view::surface::page_heading(&current(), header(), actions(managed))
}

/// The path of the page `paging` names, with the find's `words`.
fn page_href(path: &str, words: &str, paging: Paging) -> String {
    if words.is_empty() {
        format!("{path}?page={}&size={}", paging.page, paging.size)
    } else {
        let encoded: String = url::form_urlencoded::byte_serialize(words.as_bytes()).collect();
        format!(
            "{path}?q={encoded}&page={}&size={}",
            paging.page, paging.size
        )
    }
}

/// The listing of `federation` in the view `view_of`, as `asked`.
fn listing(federation: &Federation, view_of: View, asked: &Asked, managed: bool) -> Html {
    let found = |text: &str| {
        asked
            .words
            .as_deref()
            .is_none_or(|words| matches(text, words))
    };
    let organisations: Vec<&Organisation> = federation
        .organisations
        .iter()
        .filter(|organisation| found(organisation.shown()) || found(&organisation.id))
        .collect();
    let mut nodes: Vec<(&Organisation, &Node)> = federation
        .nodes()
        .filter(|(organisation, node)| found(&node.id) || found(organisation.shown()))
        .collect();
    nodes.sort_by_key(|(_, node)| std::cmp::Reverse(rank(node.health())));
    let total_organisations = u64::try_from(federation.organisations.len()).unwrap_or(u64::MAX);
    let total_nodes = u64::try_from(federation.nodes().count()).unwrap_or(u64::MAX);
    let (total, shown, path) = match view_of {
        View::Organisations => (total_organisations, organisations.len(), ORGANISATIONS),
        View::Nodes => (total_nodes, nodes.len(), ALL_NODES),
    };
    let window = Window::over(u32::try_from(shown).unwrap_or(u32::MAX), asked.paging);
    let words = asked.words.clone().unwrap_or_default();
    let body = match view_of {
        View::Organisations if organisations.is_empty() && asked.words.is_none() => list_empty(
            tr(Msg::OrganisationsNone),
            managed.then(|| {
                (
                    tr(Msg::OrganisationsAddFirst),
                    String::from(crate::pages::configuration::NEW_MEMBER),
                )
            }),
        ),
        View::Organisations if organisations.is_empty() => list_empty(
            tr(Msg::OrganisationsNoMatch),
            Some((tr(Msg::ClearFilters), String::from(path))),
        ),
        View::Organisations => organisation_table(window.slice(&organisations)),
        View::Nodes if nodes.is_empty() => list_empty(
            tr(Msg::NodesNoMatch),
            Some((tr(Msg::ClearFilters), String::from(path))),
        ),
        View::Nodes => node_table(window.slice(&nodes)),
    };
    let find_label = tr(Msg::OrganisationsFind);
    let toolbar = find_form(
        Find {
            id: "organisations-find",
            name: "q",
            label: &find_label,
            value: &words,
            source: StateSource::Url,
            action: Some(path),
            fields: &[],
        },
        Html::empty(),
    );
    let filters = filters(asked, path, shown);
    let tabs = tab_row(view_of, total_organisations, total_nodes);
    let pager = (window.pages > 1).then(|| {
        link_pager(
            &current(),
            Listing::new(tr(Msg::OrganisationsTitle)).noun(Noun::new(message(match view_of {
                View::Organisations => Msg::NounOrganisations,
                View::Nodes => Msg::NounNodes,
            }))),
            window,
            &|paging| page_href(path, &words, paging),
        )
    });
    list_page(
        &current(),
        ListPage {
            header: header(),
            actions: actions(managed),
            total: Some(total),
            summary: None,
            toolbar: Some(toolbar),
            filters,
            card: ListCard {
                title: tr(Msg::OrganisationsTitle),
                count: Some(heading_count(shown, total)),
                order: None,
                tabs: Some(tabs),
                actions: None,
                body,
                pager,
            },
        },
    )
}

/// The count the card's heading draws: every row, or the `shown` rows the
/// find keeps of `total`.
fn heading_count(shown: usize, total: u64) -> HeadingCount {
    let shown = u64::try_from(shown).unwrap_or(u64::MAX);
    if shown == total {
        HeadingCount::All(total)
    } else {
        HeadingCount::Shown { shown, total }
    }
}

/// The filters set on the listing at `path`: the find's words, with the
/// `shown` rows they keep.
fn filters(asked: &Asked, path: &str, shown: usize) -> ActiveFilters {
    ActiveFilters {
        chips: asked
            .words
            .as_ref()
            .map(|words| {
                vec![FilterChip {
                    label: tr_with(Msg::FilterWords, &[("words", words.clone().into())]),
                    remove: path.to_owned(),
                }]
            })
            .unwrap_or_default(),
        clear: path.to_owned(),
        shown: Some(u64::try_from(shown).unwrap_or(u64::MAX)),
    }
}

/// The binder tabs over the card, `view_of` current.
fn tab_row(view_of: View, organisations: u64, nodes: u64) -> TabRow {
    TabRow {
        label: tr(Msg::OrganisationsTitle),
        tabs: vec![
            Tab::new("organisations", ORGANISATIONS, tr(Msg::OrganisationsTitle)).count(
                organisations,
                tr_with(
                    Msg::OrganisationCount,
                    &[("count", Arg::from(organisations))],
                ),
            ),
            Tab::new("nodes", ALL_NODES, tr(Msg::AllNodes)).count(
                nodes,
                tr_with(Msg::NodeCount, &[("count", Arg::from(nodes))]),
            ),
        ],
        current: String::from(match view_of {
            View::Organisations => "organisations",
            View::Nodes => "nodes",
        }),
    }
}

/// Whether `text` holds `words`, ignoring case.
fn matches(text: &str, words: &str) -> bool {
    text.to_lowercase().contains(&words.to_lowercase())
}

/// How far a node's state lists it toward the top: worst first.
const fn rank(health: Health) -> u8 {
    match health {
        Health::Unreachable => 4,
        Health::Degraded => 3,
        Health::NotProbed => 2,
        Health::Reachable => 1,
        Health::Excluded => 0,
    }
}

/// The table of `organisations`.
fn organisation_table(organisations: &[&Organisation]) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("organisation", tr(Msg::ColumnOrganisation))
        },
        Column {
            role: Role::Sub,
            ..Column::text("id", tr(Msg::ColumnId))
        },
        Column {
            numeric: true,
            ..Column::text("nodes", tr(Msg::ColumnNodes))
        },
        Column {
            numeric: true,
            role: Role::Quiet,
            ..Column::text("endpoints", tr(Msg::ColumnEndpoints))
        },
        Column::text("worst", tr(Msg::ColumnWorstNode)),
    ];
    let rows = organisations.iter().map(|organisation| {
        let href = organisation.routable().then(|| organisation.href());
        let health = organisation.health();
        let locale = current();
        h(format!(
            "<tr{}>{}{}{}{} {}</tr>",
            excluded(health),
            title_cell(href.as_deref(), t(organisation.shown()), None),
            cell(
                &columns[1],
                h(format!("<code>{}</code>", t(&organisation.id)))
            ),
            count_cell(
                &locale,
                &columns[2],
                u32::try_from(organisation.nodes.len()).unwrap_or(u32::MAX),
                Noun::new(message(Msg::NounNodes)),
            ),
            count_cell(
                &locale,
                &columns[3],
                u32::try_from(organisation.endpoint_count()).unwrap_or(u32::MAX),
                Noun::new(message(Msg::NounEndpoints)),
            ),
            cell(&columns[4], state(health.mark(), health.word(), None)),
        ))
    });
    data_table(DataTable {
        caption: &tr(Msg::OrganisationsTitle),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(Html::join(rows)),
    })
}

/// The class attribute of a row in `health`: muted when it is excluded.
fn excluded(health: Health) -> &'static str {
    if health == Health::Excluded {
        " class=\"is-excluded\""
    } else {
        ""
    }
}

/// The table of every node with its organisation, worst first.
fn node_table(nodes: &[(&Organisation, &Node)]) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("node", tr(Msg::ColumnNode))
        },
        Column::text("organisation", tr(Msg::ColumnOrganisation)),
        Column {
            role: Role::Wide,
            ..Column::text("system", tr(Msg::ColumnSystemId))
        },
        Column::text("state", tr(Msg::ColumnState)),
        Column {
            numeric: true,
            role: Role::Quiet,
            ..Column::text("median", tr(Msg::ColumnMedian)).titled(tr(Msg::ColumnMedianTitle))
        },
        Column {
            numeric: true,
            role: Role::Quiet,
            ..Column::text("p95", tr(Msg::ColumnP95)).titled(tr(Msg::ColumnP95Title))
        },
    ];
    let rows = nodes.iter().map(|(organisation, node)| {
        let href =
            (node.routable() && organisation.routable()).then(|| organisation.node_href(node));
        let health = node.health();
        h(format!(
            "<tr{}>{}{}{} {}{}{}</tr>",
            excluded(health),
            title_cell(
                href.as_deref(),
                h(format!("<code>{}</code>", t(&node.id))),
                None
            ),
            cell(&columns[1], t(organisation.shown())),
            cell(
                &columns[2],
                h(format!(
                    "<code>{}</code>",
                    t(node.system_id.clone().unwrap_or_default())
                )),
            ),
            cell(&columns[3], state(health.mark(), health.word(), None)),
            cell(&columns[4], crate::pages::latency(node.latency_ms_p50())),
            cell(&columns[5], crate::pages::latency(node.latency_ms_p95())),
        ))
    });
    data_table(DataTable {
        caption: &tr(Msg::AllNodes),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(Html::join(rows)),
    })
}
