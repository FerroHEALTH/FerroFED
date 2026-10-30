// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's self-description, `OPTIONS {base}/` (§7a.2, N30), as a
//! reference page (T3).
//!
//! Binder tabs hold Federation, Capabilities, ITS-REST areas, Limits and
//! Endpoints, each its own route; each row is read-only, with the term
//! clients read in the document as its key hint. The document itself has
//! its one home on About. The replica that answered is a row of its own.

use ferrohealth_ui::html::Html;
use ferrohealth_ui::i18n::Arg;
use ferrohealth_ui::table::view::{
    Body, Column, DataTable, Role, Sorting, cell, data_table, title_cell,
};
use ferrohealth_ui::view::action::ActionGroup;
use ferrohealth_ui::view::surface::{Header, page_heading};
use ferrohealth_ui::view::tabs::{Tab, binder};
use ferrohealth_ui::view::tag::Tag;
use ferrohealth_ui::view::template::settings::{Setting, setting_row, settings_panel};
use openehr_federation::options::{GatewayDescription, OptionsRoot};

use crate::app::{ABOUT, SELF_DESCRIPTION};
use crate::locale::{Msg, current, tr, tr_with};
use crate::pages::{h, stopped, t};

/// The tabs, each its key, its words and its intro.
const TABS: [(&str, Msg, Msg); 5] = [
    (
        "federation",
        Msg::DescribeFederation,
        Msg::DescribeFederationIntro,
    ),
    (
        "capabilities",
        Msg::DescribeCapabilities,
        Msg::DescribeCapabilitiesIntro,
    ),
    ("areas", Msg::DescribeAreas, Msg::DescribeAreasIntro),
    ("limits", Msg::DescribeLimits, Msg::DescribeLimitsIntro),
    (
        "endpoints",
        Msg::DescribeEndpoints,
        Msg::DescribeEndpointsIntro,
    ),
];

/// The self-description, on the tab the path names.
pub async fn page() -> Html {
    let tab = crate::pages::param("tab");
    let tab = if tab.is_empty() {
        String::from("federation")
    } else {
        tab
    };
    let Some((key, _, intro)) = TABS.iter().find(|(key, _, _)| *key == tab).copied() else {
        return crate::pages::errors::not_found();
    };
    let read = description_now().await;
    let body = {
        let heading = page_heading(
            &current(),
            Header {
                title: tr(Msg::DescribeTitle).into(),
                subtitle_view: Some(crate::pages::marked(
                    Msg::DescribeSubtitle,
                    &[],
                    vec![(
                        "about",
                        h(format!(
                            "<a href=\"{ABOUT}\">{}</a>",
                            t(tr(Msg::DescribeSubtitleAbout))
                        )),
                    )],
                )),
                ..Header::default()
            },
            ActionGroup::none(),
        );
        let panel = match read {
            Err(stop) => {
                return crate::app::title(tr(Msg::DescribeTitle))
                    .then(heading)
                    .then(stopped(&stop));
            }
            Ok((description, replica)) => tab_panel(key, intro, &description, replica),
        };
        let tabs = TABS
            .iter()
            .map(|(each, words, _)| {
                Tab::new(*each, format!("{SELF_DESCRIPTION}/{each}"), tr(*words))
            })
            .collect();
        heading.then(binder(&current(), tr(Msg::DescribeTitle), tabs, key, panel))
    };
    crate::app::title(tr(Msg::DescribeTitle)).then(body)
}

/// The gateway's self-description and the replica that answered, as the
/// operator of the request reads them.
///
/// # Errors
/// Returns the [`crate::read::Stop`] of a missing session, or of a
/// self-description the gateway refused or did not answer.
pub async fn description_now() -> Result<(OptionsRoot, Option<String>), crate::read::Stop> {
    let (state, token) = crate::read::signed_in()?;
    let description = state.gateway().self_description(&token).await?;
    let replica = crate::read::deployment()
        .await
        .ok()
        .and_then(|deployment| deployment.replica);
    Ok((description, replica))
}

/// One read-only row: its label, its line, its value and the term the
/// document names it by.
fn row(label: Msg, line: Msg, value: Html, term: &str) -> Html {
    setting_row(
        &current(),
        Setting {
            label: tr(label),
            description: tr(line),
            value,
            keys: vec![term.to_owned()],
            next_start: false,
            source: None,
        },
        false,
    )
}

/// A value in the code face.
fn code(text: impl Into<String>) -> Html {
    h(format!("<code>{}</code>", t(text.into())))
}

/// `offered` as a tag in words.
fn offered(offered: bool) -> Html {
    Tag::new(tr(if offered {
        Msg::Offered
    } else {
        Msg::NotOffered
    }))
    .view()
}

/// The panel of the tab `key` of `description`.
fn tab_panel(key: &str, intro: Msg, description: &OptionsRoot, replica: Option<String>) -> Html {
    let gateway = &description.federation;
    let rows = match key {
        "capabilities" => capabilities(gateway),
        "areas" => areas(gateway),
        "limits" => limits(gateway),
        "endpoints" => return endpoints_panel(intro, description),
        _ => federation(description, replica),
    };
    settings_panel(t(tr(intro)), None, rows)
}

/// The Federation tab: the federation, the specification, the key set and
/// the replica that answered.
fn federation(description: &OptionsRoot, replica: Option<String>) -> Vec<Html> {
    let gateway = &description.federation;
    let mut rows = vec![
        row(
            Msg::DescribeFederationId,
            Msg::DescribeFederationIdLine,
            code(gateway.id.to_string()),
            "federation.id",
        ),
        row(
            Msg::DescribeSpecification,
            Msg::DescribeSpecificationLine,
            t(tr_with(
                Msg::AboutSpecificationValue,
                &[("version", gateway.spec_version.as_str().to_owned().into())],
            )),
            "federation.spec_version",
        ),
    ];
    if let Some(jwks) = gateway
        .auth
        .as_ref()
        .and_then(|auth| auth.jwks_uri.as_ref())
    {
        rows.push(row(
            Msg::DescribeJwks,
            Msg::DescribeJwksLine,
            code(jwks.to_string()),
            "federation.auth.jwks_uri",
        ));
    }
    rows.extend(manufacturer(description));
    rows.push(setting_row(
        &current(),
        Setting {
            label: tr(Msg::DescribeAnsweredBy),
            description: tr(Msg::DescribeAnsweredByLine),
            value: replica.map_or_else(|| t(tr(Msg::ReplicaUnnamed)), code),
            keys: Vec::new(),
            next_start: false,
            source: None,
        },
        false,
    ));
    rows
}

// NOTE: §7a.2 and options-root.schema.json leave the top level open: the gateway
// names its manufacturer there (Regulation (EU) 2025/327 Art 30(1)(g)).
/// The top-level member that names the gateway's manufacturer.
const MANUFACTURER: &str = "manufacturer";

/// The top-level member that names the last day of the release's support
/// period (Regulation (EU) 2024/2847 Art 13(19)).
const SUPPORTED_UNTIL: &str = "supported_until";

/// The manufacturer as the gateway declares it.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct Manufacturer {
    /// The registered name.
    name: Option<String>,
    /// The postal address, on one line.
    postal_address: Option<String>,
    /// The single point of contact.
    email: Option<String>,
    /// The website.
    website: Option<String>,
}

/// The rows of the manufacturer and the support period `description`
/// declares, each said as not declared where it names none.
fn manufacturer(description: &OptionsRoot) -> Vec<Html> {
    let said = |value: Option<String>| t(value.unwrap_or_else(|| tr(Msg::DescribeNotDeclared)));
    let supported = match description.extra.get(SUPPORTED_UNTIL) {
        None => tr(Msg::DescribeNotDeclared),
        Some(raw) => match serde_json::from_str::<Option<String>>(raw.get()) {
            Ok(Some(day)) => day,
            Ok(None) => tr(Msg::DescribeNoEndDate),
            Err(_unreadable) => tr(Msg::DescribeUnreadable),
        },
    };
    let fixes = row(
        Msg::DescribeFixesUntil,
        Msg::DescribeFixesUntilLine,
        t(supported),
        SUPPORTED_UNTIL,
    );
    let declared = match description.extra.get(MANUFACTURER) {
        None => Ok(Manufacturer::default()),
        Some(raw) => serde_json::from_str::<Manufacturer>(raw.get()),
    };
    // NOTE: no specification governs this: our own design; a manufacturer the
    // console cannot read is said so in its one row, never shown as not declared.
    let Ok(declared) = declared else {
        return vec![
            row(
                Msg::DescribeManufacturer,
                Msg::DescribeManufacturerLine,
                t(tr(Msg::DescribeUnreadable)),
                MANUFACTURER,
            ),
            fixes,
        ];
    };
    vec![
        row(
            Msg::DescribeManufacturer,
            Msg::DescribeManufacturerLine,
            said(declared.name),
            "manufacturer.name",
        ),
        row(
            Msg::DescribeManufacturerAddress,
            Msg::DescribeManufacturerAddressLine,
            said(declared.postal_address),
            "manufacturer.postal_address",
        ),
        row(
            Msg::DescribeManufacturerContact,
            Msg::DescribeManufacturerContactLine,
            said(declared.email),
            "manufacturer.email",
        ),
        row(
            Msg::DescribeManufacturerWebsite,
            Msg::DescribeManufacturerWebsiteLine,
            said(declared.website),
            "manufacturer.website",
        ),
        fixes,
    ]
}

/// The Capabilities tab: what the gateway offers a query (§8, §10,
/// §11.4, §12.7).
fn capabilities(gateway: &GatewayDescription) -> Vec<Html> {
    let completeness = &gateway.completeness;
    let definition = &gateway.definition;
    vec![
        row(
            Msg::DescribeFanOut,
            Msg::DescribeFanOutLine,
            offered(gateway.aql.fan_out),
            "federation.aql.fan_out",
        ),
        row(
            Msg::DescribeDedup,
            Msg::DescribeDedupLine,
            code(gateway.dedup.modes.as_slice().join(", ")),
            "federation.dedup.modes",
        ),
        row(
            Msg::DescribeBestEffort,
            Msg::DescribeBestEffortLine,
            offered(completeness.best_effort()),
            "federation.completeness.best_effort",
        ),
        row(
            Msg::DescribeRegistry,
            Msg::DescribeRegistryLine,
            offered(definition.stored_query_registry().unwrap_or(false)),
            "federation.definition.stored_query_registry",
        ),
        row(
            Msg::DescribeStoredFanOut,
            Msg::DescribeStoredFanOutLine,
            offered(definition.stored_query_fan_out().unwrap_or(false)),
            "federation.definition.stored_query_fan_out",
        ),
        row(
            Msg::DescribeTemplateFanOut,
            Msg::DescribeTemplateFanOutLine,
            offered(definition.fan_out_template_upload()),
            "federation.definition.fan_out_template_upload",
        ),
        row(
            Msg::DescribeLocalization,
            Msg::DescribeLocalizationLine,
            code(gateway.localization.on_failure.clone()),
            "federation.localization.on_failure",
        ),
    ]
}

/// The ITS-REST areas tab: how each area of the openEHR REST API is served
/// (§7a.2).
fn areas(gateway: &GatewayDescription) -> Vec<Html> {
    let areas = &gateway.its_rest;
    vec![
        row(
            Msg::DescribeQuery,
            Msg::DescribeQueryLine,
            code(areas.query.clone()),
            "federation.its_rest.query",
        ),
        row(
            Msg::DescribeEhr,
            Msg::DescribeEhrLine,
            code(areas.ehr.clone()),
            "federation.its_rest.ehr",
        ),
        row(
            Msg::DescribeDefinition,
            Msg::DescribeDefinitionLine,
            code(areas.definition.clone()),
            "federation.its_rest.definition",
        ),
        row(
            Msg::DescribeDemographic,
            Msg::DescribeDemographicLine,
            code(areas.demographic.as_str().to_owned()),
            "federation.its_rest.demographic",
        ),
    ]
}

/// The Limits tab: the budgets, the paging and the aggregates (§11.5,
/// §11.6).
fn limits(gateway: &GatewayDescription) -> Vec<Html> {
    let aggregates = &gateway.aggregates.decomposable;
    vec![
        row(
            Msg::DescribePerNode,
            Msg::DescribePerNodeLine,
            t(tr_with(
                Msg::Milliseconds,
                &[("count", Arg::from(gateway.timeout.per_node_ms))],
            )),
            "federation.timeout.per_node_ms",
        ),
        row(
            Msg::DescribeOverall,
            Msg::DescribeOverallLine,
            t(tr_with(
                Msg::Milliseconds,
                &[("count", Arg::from(gateway.timeout.overall_ms))],
            )),
            "federation.timeout.overall_ms",
        ),
        row(
            Msg::DescribePolicy,
            Msg::DescribePolicyLine,
            code(gateway.timeout.policy.clone()),
            "federation.timeout.policy",
        ),
        row(
            Msg::DescribePaging,
            Msg::DescribePagingLine,
            code(gateway.paging.offset_strategy.clone()),
            "federation.paging.offset_strategy",
        ),
        row(
            Msg::DescribeAggregates,
            Msg::DescribeAggregatesLine,
            if aggregates.is_empty() {
                t(tr(Msg::None))
            } else {
                code(aggregates.join(", "))
            },
            "federation.aggregates.decomposable",
        ),
    ]
}

/// The Endpoints tab: every member endpoint the document lists, with its
/// status.
fn endpoints_panel(intro: Msg, description: &OptionsRoot) -> Html {
    let columns = [
        Column {
            role: Role::Title,
            ..Column::text("endpoint", tr(Msg::ColumnEndpoint))
        },
        Column::text("organisation", tr(Msg::ColumnOrganisation)),
        Column::text("node", tr(Msg::ColumnNode)),
        Column::text("status", tr(Msg::ColumnMembership)),
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
    let rows = description.endpoints.iter().map(|endpoint| {
        let active = endpoint.status.as_str() == "active";
        h(format!(
            "<tr{}>{}{}{}{}{}{}</tr>",
            if active { "" } else { " class=\"is-excluded\"" },
            title_cell(None, code(endpoint.id.to_string()), None),
            cell(&columns[1], code(endpoint.organisation.clone())),
            cell(
                &columns[2],
                code(endpoint.node_id.clone().unwrap_or_default())
            ),
            cell(
                &columns[3],
                Tag::new(endpoint.status.as_str().to_owned()).view()
            ),
            cell(&columns[4], crate::pages::latency(endpoint.latency_ms_p50)),
            cell(
                &columns[5],
                crate::pages::latency(crate::read::federation::p95(endpoint)),
            ),
        ))
    });
    settings_panel(t(tr(intro)), None, Vec::new()).then(data_table(DataTable {
        caption: &tr(Msg::DescribeEndpoints),
        columns: &columns,
        sorting: Sorting::default(),
        body: Body::Rows(Html::join(rows)),
    }))
}
