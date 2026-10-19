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
        <Title text=titled("Self-description") />
        {page_header(
            "Self-description",
            Some(String::from("What the gateway declares of itself in OPTIONS.")),
            (),
        )}
        <Suspense fallback=|| busy("Loading the self-description.", Busy::Section)>
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
    const ENDPOINT_COLUMNS: [Column; 6] = [
        Column::text("endpoint", "Endpoint"),
        Column::text("organisation", "Organisation"),
        Column::text("membership", "Membership"),
        Column::text("node", "Node"),
        Column::text("system_id", "system_id").mono(),
        Column::text("product", "Product"),
    ];

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
                    {card("Federation", identity(gateway, description.endpoints.len()))}
                    {card("Capabilities", capabilities(gateway))}
                }),
                stack(view! {
                    {card("Budgets and paging", budgets(gateway))}
                    {card("Manufacturer", manufacturer(description))}
                }),
            )}
            {card("ITS-REST areas", its_rest(gateway))}
            {endpoints(&description.endpoints)}
            {disclosure(
                "The OPTIONS document",
                code_panel("The OPTIONS document", Language::Json, &document),
            )}
        })
        .to_html();
        Ok(FederationView { html })
    }

    /// A card titled `title` holding the labelled values `rows`.
    fn card(title: &'static str, rows: Vec<(&'static str, String)>) -> AnyView {
        let rows = definitions(
            rows.into_iter()
                .map(|(label, value)| (label, value.into_any()))
                .collect(),
        );
        ferrohealth_ui::view::surface::card(title, rows).into_any()
    }

    /// `offered` in words.
    fn offered(offered: bool) -> String {
        String::from(if offered { "offered" } else { "not offered" })
    }

    /// Who the gateway is: its federation, the specification and its members.
    fn identity(gateway: &GatewayDescription, endpoints: usize) -> Vec<(&'static str, String)> {
        let mut rows = vec![
            ("Federation", gateway.id.to_string()),
            (
                "Specification version",
                gateway.spec_version.as_str().to_owned(),
            ),
            ("Member endpoints", endpoints.to_string()),
        ];
        if let Some(jwks) = gateway
            .auth
            .as_ref()
            .and_then(|auth| auth.jwks_uri.as_ref())
        {
            rows.push(("JWK Set", jwks.to_string()));
        }
        rows
    }

    /// How the gateway serves each ITS-REST area (§7a.2).
    fn its_rest(gateway: &GatewayDescription) -> Vec<(&'static str, String)> {
        let areas = &gateway.its_rest;
        vec![
            ("Query", areas.query.clone()),
            ("EHR", areas.ehr.clone()),
            ("Definition", areas.definition.clone()),
            ("Demographic", areas.demographic.as_str().to_owned()),
        ]
    }

    /// What the gateway offers a query (§8, §10, §11.4, §12.7).
    fn capabilities(gateway: &GatewayDescription) -> Vec<(&'static str, String)> {
        let completeness = &gateway.completeness;
        let best_effort = match completeness.opt_in() {
            Some(opt_in) if completeness.best_effort() => format!(
                "offered, by {}",
                [opt_in.header.as_deref(), opt_in.value.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(": ")
            ),
            _ => offered(completeness.best_effort()),
        };
        let definition = &gateway.definition;
        vec![
            ("AQL fan-out", offered(gateway.aql.fan_out)),
            (
                "Deduplication modes",
                gateway.dedup.modes.as_slice().join(", "),
            ),
            (
                "Deduplication header",
                gateway
                    .dedup
                    .request_header
                    .clone()
                    .unwrap_or_else(|| String::from("none declared")),
            ),
            ("Best effort", best_effort),
            (
                "Stored-query registry",
                offered(definition.stored_query_registry().unwrap_or(false)),
            ),
            (
                "Stored-query fan-out",
                offered(definition.stored_query_fan_out().unwrap_or(false)),
            ),
            (
                "Template upload fan-out",
                offered(definition.fan_out_template_upload()),
            ),
            (
                "On a localization failure",
                gateway.localization.on_failure.clone(),
            ),
        ]
    }

    /// The gateway's time budgets, its paging and its aggregates (§11.5,
    /// §11.6).
    fn budgets(gateway: &GatewayDescription) -> Vec<(&'static str, String)> {
        let aggregates = &gateway.aggregates.decomposable;
        vec![
            ("Per node", format!("{} ms", gateway.timeout.per_node_ms)),
            ("Overall", format!("{} ms", gateway.timeout.overall_ms)),
            ("Timeout policy", gateway.timeout.policy.clone()),
            ("Offset strategy", gateway.paging.offset_strategy.clone()),
            (
                "Decomposable aggregates",
                if aggregates.is_empty() {
                    String::from("none")
                } else {
                    aggregates.join(", ")
                },
            ),
        ]
    }

    /// The manufacturer and the support period the description names.
    fn manufacturer(description: &OptionsRoot) -> Vec<(&'static str, String)> {
        let not_declared = || String::from("not declared");
        let unreadable = || String::from("declared in a form the console cannot read");
        let declared = match description.extra.get(MANUFACTURER) {
            None => Ok(Manufacturer::default()),
            Some(raw) => serde_json::from_str::<Manufacturer>(raw.get()),
        };
        let supported = match description.extra.get(SUPPORTED_UNTIL) {
            None => not_declared(),
            Some(raw) => match serde_json::from_str::<Option<String>>(raw.get()) {
                Ok(Some(day)) => day,
                Ok(None) => String::from("no end date"),
                Err(_unreadable) => unreadable(),
            },
        };
        let Ok(declared) = declared else {
            return vec![("Manufacturer", unreadable()), ("Fixes until", supported)];
        };
        vec![
            ("Name", declared.name.unwrap_or_else(not_declared)),
            (
                "Postal address",
                declared.postal_address.unwrap_or_else(not_declared),
            ),
            ("Contact", declared.email.unwrap_or_else(not_declared)),
            ("Website", declared.website.unwrap_or_else(not_declared)),
            ("Fixes until", supported),
        ]
    }

    /// The member endpoints the description declares, in a table.
    fn endpoints(endpoints: &[MemberEndpoint]) -> impl IntoView + use<> {
        let columns = filled_columns(&ENDPOINT_COLUMNS, endpoints, &[], |endpoint, key| {
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
                        <td data-label="Organisation">{endpoint.organisation.clone()}</td>
                        <td data-label="Membership">{state_pill(endpoint.status.as_str())}</td>
                        <td data-label="Node" class="fui-mono">
                            {endpoint.node_id.clone().unwrap_or_default()}
                        </td>
                        <td data-label="system_id" class="fui-mono">
                            {endpoint.system_id.clone().unwrap_or_default()}
                        </td>
                        {products.then(|| view! { <td data-label="Product">{product}</td> })}
                    </tr>
                }
            })
            .collect_view();
        crate::views::table("Declared endpoints", &columns, rows.into_any())
    }
}
