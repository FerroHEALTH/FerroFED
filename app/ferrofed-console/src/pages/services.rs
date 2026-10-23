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

use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, group_row, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Crumb, Header, card, facts_row, page_heading};
use ferrohealth_ui::view::template::list::{
    ActiveFilters, HeadingCount, ListCard, ListPage, list_empty, list_page,
};
use leptos::prelude::*;

use crate::app::SERVICES;
use crate::locale::{Msg, tr};
use crate::pages::stopped;
use crate::read::federation::{Federation, Service};
use ferrohealth_ui::view::state::state;

/// The services.
#[must_use]
pub fn list() -> AnyView {
    let body = Suspend::new(async move {
        match crate::read::federation::read().await {
            Ok(federation) => services(&federation),
            Err(stop) => view! {
                {page_heading(header(), ActionGroup::none())}
                {stopped(&stop)}
            }
            .into_any(),
        }
    });
    view! {
        {crate::app::title(tr(Msg::ServicesTitle))}
        <Suspense fallback=|| ()>{body}</Suspense>
    }
    .into_any()
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
fn services(federation: &Federation) -> AnyView {
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
            rows.push(group_row(&columns, service.group.title()));
        }
        rows.push(
            view! {
                <tr>
                    {title_cell(
                        Some(&service.href()),
                        service.name(),
                        Some(service.role().into_any()),
                    )}
                    {cell(&columns[1], state(service.health.mark(), service.health.word(), None))}
                    {cell(
                        &columns[2],
                        view! { <code>{service.fault.clone().unwrap_or_default()}</code> },
                    )}
                </tr>
            }
            .into_any(),
        );
    }
    let count = u64::try_from(federation.services.len()).unwrap_or(u64::MAX);
    let body = if rows.is_empty() {
        list_empty(tr(Msg::ServicesNone), None)
    } else {
        data_table(DataTable {
            caption: &tr(Msg::ServicesTitle),
            columns: &columns,
            sorting: Sorting::default(),
            body: Body::Rows(rows.collect_view().into_any()),
        })
        .into_any()
    };
    list_page(ListPage {
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
    })
}

/// One service.
#[must_use]
pub fn page() -> AnyView {
    let key = crate::pages::param("service");
    let body = Suspend::new(async move {
        match crate::read::federation::read().await {
            Err(stop) => stopped(&stop),
            Ok(federation) => match federation.service(&key) {
                Some(service) => service_page(service, federation.replica.as_deref()),
                None => crate::pages::errors::not_found(),
            },
        }
    });
    view! { <Suspense fallback=|| ()>{body}</Suspense> }.into_any()
}

/// The page of `service`, as `replica` observed it.
fn service_page(service: &Service, replica: Option<&str>) -> AnyView {
    let heading = page_heading(
        Header {
            title: service.name().into(),
            subtitle: Some(service.role().into()),
            crumbs: vec![Crumb::new(tr(Msg::ServicesTitle), SERVICES)],
            mono: false,
            subtitle_view: None,
            badge: Some(state(service.health.mark(), service.health.word(), None)),
        },
        ActionGroup::none(),
    )
    .into_any();
    let facts = facts_row(vec![
        (tr(Msg::ColumnGroup), service.group.title().into_any()),
        (
            tr(Msg::ColumnState),
            state(service.health.mark(), service.health.word(), None),
        ),
        (
            tr(Msg::ColumnKey),
            view! { <code>{service.key.clone()}</code> }.into_any(),
        ),
        (
            tr(Msg::ColumnFault),
            service.fault.clone().map_or_else(
                || tr(Msg::None).into_any(),
                |fault| view! { <code>{fault}</code> }.into_any(),
            ),
        ),
        (
            tr(Msg::ColumnObservedBy),
            replica.map_or_else(
                || tr(Msg::ReplicaUnnamed).into_any(),
                |name| view! { <code>{name.to_owned()}</code> }.into_any(),
            ),
        ),
    ]);
    view! {
        {crate::app::title(service.name())}
        {heading}
        {card(
            tr(Msg::ServiceHealth),
            view! {
                {facts}
                <p class="fui-small fui-muted">{tr(Msg::ServiceHealthLine)}</p>
            },
        )}
    }
    .into_any()
}
