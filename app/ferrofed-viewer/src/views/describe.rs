// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The gateway's self-description, `OPTIONS {base}/` (§7a.2, N30): cards of
//! what it declares, and the document itself behind a disclosure.
//!
//! The page is rendered on the server ([`render`]) and the browser shows
//! the HTML it wrote, so the bundle carries none of its drawing.

use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

use crate::locale::{Msg, tr};
use crate::views::model::{Outcome, ViewError};
use crate::views::{shown, titled};
use ferrohealth_ui::view::empty::{Busy, busy};
use ferrohealth_ui::view::surface::page_header;

/// The self-description view, as the server rendered it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FederationView {
    /// The cards and the document, rendered on the server.
    pub html: String,
}

/// Loads the self-description view, `OPTIONS {base}/`.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// console's fault as its [`ViewError`]; the gateway's refusal is an
/// [`Outcome::Refused`].
#[server(endpoint = "federation")]
pub async fn federation() -> Result<Outcome<FederationView>, ViewError> {
    use crate::views::load::server;
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let description = state.gateway().self_description(&token).await?;
        Ok(render::federation(&description)?)
    })
    .await
}

/// The gateway's self-description, `OPTIONS {base}/`.
#[component]
#[expect(
    clippy::must_use_candidate,
    reason = "the component macro writes the function it returns without the attributes on the one written here (https://docs.rs/leptos/0.8/leptos/attr.component.html)"
)]
pub fn FederationPage() -> impl IntoView {
    let loaded = Resource::new_blocking(|| (), |()| federation());
    view! {
        <Title text=titled(&tr(Msg::SelfDescription)) />
        {page_header(
            tr(Msg::SelfDescription),
            Some(tr(Msg::WhatGatewayDeclaresItselfOptions)),
            (),
        )}
        <Suspense fallback=|| busy(tr(Msg::LoadingTheSelfDescription), Busy::Section)>
            {move || Suspend::new(async move {
                shown(
                    loaded.await,
                    |view| view! { <div inner_html=view.html></div> }.into_any(),
                )
            })}
        </Suspense>
    }
}

/// The server's rendering of the self-description.
///
/// Every value reaches the markup as `view!` text or an attribute value,
/// which Leptos escapes.
#[cfg(not(target_arch = "wasm32"))]
pub mod render {
    use crate::locale::{Msg, tr_served, tr_served_with};
    use leptos::prelude::*;
    use openehr_federation::options::{GatewayDescription, MemberEndpoint, OptionsRoot};
    use serde::Deserialize;

    use crate::views::describe::FederationView;
    use crate::views::model::ViewError;
    use crate::views::state_pill;
    use ferrohealth_ui::highlight::Language;
    use ferrohealth_ui::table::model::filled_columns;
    use ferrohealth_ui::table::view::Column;
    use ferrohealth_ui::view::code::code_panel;
    use ferrohealth_ui::view::layout::main_aside;
    use ferrohealth_ui::view::surface::{definitions, disclosure, stack};

    /// The columns of the declared endpoints.
    fn endpoint_columns() -> [Column; 6] {
        [
            Column::text("endpoint", tr_served(Msg::Endpoint)),
            Column::text("organisation", tr_served(Msg::Organisation)),
            Column::text("membership", tr_served(Msg::Membership)),
            Column::text("node", tr_served(Msg::Node)),
            Column::text("system_id", tr_served(Msg::SystemId)),
            Column::text("product", tr_served(Msg::Product)),
        ]
    }

    /// The top-level member that names the manufacturer, beside
    /// `federation` and `endpoints` (Regulation (EU) 2025/327 Art
    /// 30(1)(g)).
    // NOTE: §7a.2 and options-root.schema.json leave the top level open: the gateway
    // names its manufacturer there, and the console reads what it names.
    const MANUFACTURER: &str = "manufacturer";

    /// The top-level member that names the last day of the release's
    /// support period (Regulation (EU) 2024/2847 Art 13(19)).
    ///
    /// Fixed releases are published until that day.
    const SUPPORTED_UNTIL: &str = "supported_until";

    /// The manufacturer as the gateway declares it.
    #[derive(Debug, Default, Deserialize)]
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

    /// The self-description view of `description`.
    ///
    /// # Errors
    /// Returns [`ViewError::Unavailable`] when the description cannot be
    /// written back as JSON.
    pub fn federation(description: &OptionsRoot) -> Result<FederationView, ViewError> {
        let document = serde_json::to_string_pretty(description)
            .map_err(|_unwritable| ViewError::Unavailable)?;
        let gateway = &description.federation;
        let html = stack(view! {
            {main_aside(
                stack(view! {
                    {card(tr_served(Msg::Federation), identity(gateway, description.endpoints.len()))}
                    {card(tr_served(Msg::Capabilities), capabilities(gateway))}
                }),
                stack(view! {
                    {card(tr_served(Msg::BudgetsAndPaging), budgets(gateway))}
                    {card(tr_served(Msg::Manufacturer), manufacturer(description))}
                }),
            )}
            {card(tr_served(Msg::ItsRestAreas), its_rest(gateway))}
            {endpoints(&description.endpoints)}
            {disclosure(
                tr_served(Msg::TheOptionsDocument),
                code_panel(&tr_served(Msg::TheOptionsDocument), Language::Json, &document),
            )}
        })
        .to_html();
        Ok(FederationView { html })
    }

    /// A card titled `title` holding the labelled values `rows`.
    fn card(title: String, rows: Vec<(String, String)>) -> AnyView {
        let rows = definitions(
            rows.into_iter()
                .map(|(label, value)| (label, value.into_any()))
                .collect(),
        );
        ferrohealth_ui::view::surface::card(title, rows).into_any()
    }

    /// `offered` in words.
    fn offered(offered: bool) -> String {
        if offered {
            tr_served(Msg::Offered)
        } else {
            tr_served(Msg::NotOffered)
        }
    }

    /// Who the gateway is: its federation, the specification and its members.
    fn identity(gateway: &GatewayDescription, endpoints: usize) -> Vec<(String, String)> {
        let mut rows = vec![
            (tr_served(Msg::Federation), gateway.id.to_string()),
            (
                tr_served(Msg::SpecificationVersion),
                gateway.spec_version.as_str().to_owned(),
            ),
            (tr_served(Msg::MemberEndpoints), endpoints.to_string()),
        ];
        if let Some(jwks) = gateway
            .auth
            .as_ref()
            .and_then(|auth| auth.jwks_uri.as_ref())
        {
            rows.push((tr_served(Msg::JwkSet), jwks.to_string()));
        }
        rows
    }

    /// How the gateway serves each ITS-REST area (§7a.2).
    fn its_rest(gateway: &GatewayDescription) -> Vec<(String, String)> {
        let areas = &gateway.its_rest;
        vec![
            (tr_served(Msg::Query), areas.query.clone()),
            (tr_served(Msg::Ehr), areas.ehr.clone()),
            (tr_served(Msg::Definition), areas.definition.clone()),
            (
                tr_served(Msg::Demographic),
                areas.demographic.as_str().to_owned(),
            ),
        ]
    }

    /// What the gateway offers a query (§8, §10, §11.4, §12.7).
    fn capabilities(gateway: &GatewayDescription) -> Vec<(String, String)> {
        let completeness = &gateway.completeness;
        let best_effort = match completeness.opt_in() {
            Some(opt_in) if completeness.best_effort() => tr_served_with(
                Msg::OfferedBy,
                &[(
                    "value0",
                    ([opt_in.header.as_deref(), opt_in.value.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(": "))
                    .clone()
                    .into(),
                )],
            ),
            _ => offered(completeness.best_effort()),
        };
        let definition = &gateway.definition;
        vec![
            (tr_served(Msg::AqlFanOut), offered(gateway.aql.fan_out)),
            (
                tr_served(Msg::DeduplicationModes),
                gateway.dedup.modes.as_slice().join(", "),
            ),
            (
                tr_served(Msg::DeduplicationHeader),
                gateway
                    .dedup
                    .request_header
                    .clone()
                    .unwrap_or_else(|| tr_served(Msg::NoneDeclared)),
            ),
            (tr_served(Msg::BestEffort), best_effort),
            (
                tr_served(Msg::StoredQueryRegistry),
                offered(definition.stored_query_registry().unwrap_or(false)),
            ),
            (
                tr_served(Msg::StoredQueryFanOut),
                offered(definition.stored_query_fan_out().unwrap_or(false)),
            ),
            (
                tr_served(Msg::TemplateUploadFanOut),
                offered(definition.fan_out_template_upload()),
            ),
            (
                tr_served(Msg::OnALocalizationFailure),
                gateway.localization.on_failure.clone(),
            ),
        ]
    }

    /// The gateway's time budgets, its paging and its aggregates (§11.5,
    /// §11.6).
    fn budgets(gateway: &GatewayDescription) -> Vec<(String, String)> {
        let aggregates = &gateway.aggregates.decomposable;
        vec![
            (
                tr_served(Msg::PerNode),
                tr_served_with(
                    Msg::Ms5,
                    &[(
                        "per_node_ms",
                        (gateway.timeout.per_node_ms).to_string().into(),
                    )],
                ),
            ),
            (
                tr_served(Msg::Overall),
                tr_served_with(
                    Msg::Ms6,
                    &[(
                        "overall_ms",
                        (gateway.timeout.overall_ms).to_string().into(),
                    )],
                ),
            ),
            (
                tr_served(Msg::TimeoutPolicy),
                gateway.timeout.policy.clone(),
            ),
            (
                tr_served(Msg::OffsetStrategy),
                gateway.paging.offset_strategy.clone(),
            ),
            (
                tr_served(Msg::DecomposableAggregates),
                if aggregates.is_empty() {
                    tr_served(Msg::None)
                } else {
                    aggregates.join(", ")
                },
            ),
        ]
    }

    /// The manufacturer and the support period the description names.
    fn manufacturer(description: &OptionsRoot) -> Vec<(String, String)> {
        let not_declared = || tr_served(Msg::NotDeclared);
        let unreadable = || tr_served(Msg::DeclaredFormConsoleCannotRead);
        let declared = match description.extra.get(MANUFACTURER) {
            None => Ok(Manufacturer::default()),
            Some(raw) => serde_json::from_str::<Manufacturer>(raw.get()),
        };
        let supported = match description.extra.get(SUPPORTED_UNTIL) {
            None => not_declared(),
            Some(raw) => match serde_json::from_str::<Option<String>>(raw.get()) {
                Ok(Some(day)) => day,
                Ok(None) => tr_served(Msg::NoEndDate),
                Err(_unreadable) => unreadable(),
            },
        };
        let Ok(declared) = declared else {
            return vec![
                (tr_served(Msg::Manufacturer), unreadable()),
                (tr_served(Msg::FixesUntil), supported),
            ];
        };
        vec![
            (
                tr_served(Msg::Name),
                declared.name.unwrap_or_else(not_declared),
            ),
            (
                tr_served(Msg::PostalAddress),
                declared.postal_address.unwrap_or_else(not_declared),
            ),
            (
                tr_served(Msg::Contact),
                declared.email.unwrap_or_else(not_declared),
            ),
            (
                tr_served(Msg::Website),
                declared.website.unwrap_or_else(not_declared),
            ),
            (tr_served(Msg::FixesUntil), supported),
        ]
    }

    /// The member endpoints the description declares, in a table.
    fn endpoints(endpoints: &[MemberEndpoint]) -> impl IntoView + use<> {
        let columns = filled_columns(&endpoint_columns(), endpoints, &[], |endpoint, key| {
            key != "product" || endpoint.product.is_some()
        });
        let products = crate::views::has(&columns, "product");
        let rows = endpoints
            .iter()
            .map(|endpoint| {
                let product = match (&endpoint.product, &endpoint.version) {
                    (Some(product), Some(version)) => format!("{product} {version}"),
                    (Some(product), None) => product.clone(),
                    (None, _) => String::new(),
                };
                view! {
                    <tr>
                        <th scope="row" class="fui-mono">
                            {endpoint.id.to_string()}
                        </th>
                        <td data-label=tr_served(Msg::Organisation)>{endpoint.organisation.clone()}</td>
                        <td data-label=tr_served(Msg::Membership)>{state_pill(endpoint.status.as_str())}</td>
                        <td data-label=tr_served(Msg::Node) class="fui-mono">
                            {endpoint.node_id.clone().unwrap_or_default()}
                        </td>
                        <td data-label=tr_served(Msg::SystemId) class="fui-mono">
                            {endpoint.system_id.clone().unwrap_or_default()}
                        </td>
                        {products.then(|| view! { <td data-label=tr_served(Msg::Product)>{product}</td> })}
                    </tr>
                }
            })
            .collect_view();
        crate::views::table(
            &tr_served(Msg::DeclaredEndpoints),
            &columns,
            rows.into_any(),
        )
    }
}
