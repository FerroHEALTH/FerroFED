// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The services the gateway asks, and one service's page.
//!
//! One table holds them in group rows (Identity, Localization, Consent,
//! Directory, Audit): each service and its role in plain words, its state
//! and the class of a fault the gateway names; the row opens the service.
//! The states are the replica's own observations, read from the gateway's
//! operator surface; a service the deployment configures none of is not
//! listed.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, group_row, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::state::state;
use ferrohealth_ui::view::surface::{Crumb, Header, card, facts_row, page_heading};
use ferrohealth_ui::view::template::list::{
    ActiveFilters, HeadingCount, ListCard, ListPage, list_empty, list_page,
};

use crate::app::SERVICES;
use crate::locale::{Msg, current, tr};
use crate::pages::{h, stopped, t};
use crate::read::federation::{Federation, Service};

/// The services.
pub async fn list() -> Html {
    let body = match crate::read::federation::read().await {
        Ok(federation) => services(&federation),
        Err(stop) => page_heading(&current(), header(), ActionGroup::none()).then(stopped(&stop)),
    };
    crate::app::title(tr(Msg::ServicesTitle)).then(body)
}

/// The page's header.
fn header() -> Header {
    Header {
        title: tr(Msg::ServicesTitle).into(),
        subtitle: Some(tr(Msg::ServicesSubtitle).into()),
        ..Header::default()
    }
}

/// The services of `federation`, in their group rows.
fn services(federation: &Federation) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("service", tr(Msg::ColumnService))
        },
        Column::text("state", tr(Msg::ColumnState)),
        Column {
            role: Role::Quiet,
            ..Column::text("fault", tr(Msg::ColumnFault))
        },
    ];
    let mut rows = Vec::new();
    let mut group = None;
    for service in &federation.services {
        if group != Some(service.group) {
            group = Some(service.group);
            rows.push(group_row(&columns, t(service.group.title())));
        }
        rows.push(h(format!(
            "<tr>{}{}{}</tr>",
            title_cell(
                Some(&service.href()),
                t(service.name()),
                Some(t(service.role())),
            ),
            cell(
                &columns[1],
                state(service.health.mark(), service.health.word(), None)
            ),
            cell(
                &columns[2],
                h(format!(
                    "<code>{}</code>",
                    t(service.fault.clone().unwrap_or_default())
                )),
            ),
        )));
    }
    let count = u64::try_from(federation.services.len()).unwrap_or(u64::MAX);
    let body = if rows.is_empty() {
        list_empty(tr(Msg::ServicesNone), None)
    } else {
        data_table(DataTable {
            caption: &tr(Msg::ServicesTitle),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(Html::join(rows)),
        })
    };
    list_page(
        &current(),
        ListPage {
            header: header(),
            actions: ActionGroup::none(),
            total: Some(count),
            summary: None,
            toolbar: None,
            filters: ActiveFilters::default(),
            card: ListCard {
                title: tr(Msg::ServicesTitle),
                count: Some(HeadingCount::All(count)),
                order: None,
                tabs: None,
                actions: None,
                body,
                pager: None,
            },
        },
    )
}

/// One service.
pub async fn page() -> Html {
    let key = crate::pages::param("service");
    match crate::read::federation::read().await {
        Err(stop) => stopped(&stop),
        Ok(federation) => match federation.service(&key) {
            Some(service) => service_page(service, federation.replica.as_deref()),
            None => crate::pages::errors::not_found(),
        },
    }
}

/// The page of `service`, as `replica` observed it.
fn service_page(service: &Service, replica: Option<&str>) -> Html {
    let heading = page_heading(
        &current(),
        Header {
            title: service.name().into(),
            subtitle: Some(service.role().into()),
            crumbs: vec![Crumb::new(tr(Msg::ServicesTitle), SERVICES)],
            mono: false,
            subtitle_view: None,
            badge: Some(state(service.health.mark(), service.health.word(), None)),
        },
        ActionGroup::none(),
    );
    let code = |text: &str| h(format!("<code>{}</code>", t(text)));
    let facts = facts_row(vec![
        (tr(Msg::ColumnGroup), t(service.group.title())),
        (
            tr(Msg::ColumnState),
            state(service.health.mark(), service.health.word(), None),
        ),
        (tr(Msg::ColumnKey), code(&service.key)),
        (
            tr(Msg::ColumnFault),
            service
                .fault
                .as_deref()
                .map_or_else(|| t(tr(Msg::None)), code),
        ),
        (
            tr(Msg::ColumnObservedBy),
            replica.map_or_else(|| t(tr(Msg::ReplicaUnnamed)), code),
        ),
    ]);
    Html::join([
        crate::app::title(service.name()),
        heading,
        card(
            tr(Msg::ServiceHealth),
            facts.then(h(format!(
                "<p class=\"fui-small fui-muted\">{}</p>",
                t(tr(Msg::ServiceHealthLine))
            ))),
        ),
    ])
}
