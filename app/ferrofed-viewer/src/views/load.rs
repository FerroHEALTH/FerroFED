// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server functions each view loads through.
//!
//! Each is a public HTTP endpoint of the console as well as a call the
//! server makes while it renders, so each checks the operator's session
//! itself before it asks the gateway anything, and asks the gateway as the
//! operator, with the operator's own access token (`server/25_server_functions`,
//! the Leptos book). The bodies run on the server alone.

use leptos::prelude::*;

use crate::views::model::{FederationView, IntegrityView, MembersView, StoredView, ViewError};

/// Loads the members view: `OPTIONS {base}/` and the health of every member
/// and service.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// gateway's refusal or failure as its [`ViewError`].
#[server]
pub async fn members() -> Result<MembersView, ViewError> {
    let (state, token) = server::signed_in()?;
    let gateway = state.gateway();
    let description = gateway
        .self_description(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    let dependencies = gateway
        .dependencies(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    Ok(server::members(&description, &dependencies))
}

/// Loads the integrity view: the incidents and the routing table.
///
/// # Errors
/// As [`members`].
#[server]
pub async fn integrity() -> Result<IntegrityView, ViewError> {
    let (state, token) = server::signed_in()?;
    let gateway = state.gateway();
    let incidents = gateway
        .incidents(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    let routes = gateway
        .creating_systems(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    Ok(server::integrity(&incidents, &routes))
}

/// Loads the stored-query view.
///
/// # Errors
/// As [`members`].
#[server]
pub async fn stored_queries() -> Result<StoredView, ViewError> {
    let (state, token) = server::signed_in()?;
    let report = state
        .gateway()
        .stored_queries(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    Ok(server::stored(&report))
}

/// Loads the self-description view, `OPTIONS {base}/`.
///
/// # Errors
/// As [`members`].
#[server]
pub async fn federation() -> Result<FederationView, ViewError> {
    let (state, token) = server::signed_in()?;
    let description = state
        .gateway()
        .self_description(&token)
        .await
        .map_err(|error| server::refused(&error))?;
    server::federation(&description)
}

/// The server half of the views: the session check and the mapping of the
/// gateway's bodies onto the view models.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use ferrofed_registry::operator::{CreatingSystemReport, IncidentReport, StoredQueryReport};
    use leptos::context::use_context;
    use openehr_federation::options::OptionsRoot;

    use crate::gateway::{AccessToken, Dependencies, GatewayError};
    use crate::server::ViewerState;
    use crate::views::model::{
        FederationView, IncidentRow, IntegrityView, MemberRow, MembersView, RouteRow, StoredRow,
        StoredView, ViewError,
    };

    /// The console's state and the operator's access token, for a request
    /// that carries a live signed-in session.
    ///
    /// # Errors
    /// Returns [`ViewError::SignedOut`] when it carries none, and
    /// [`ViewError::Unavailable`] when the console's state is missing or its
    /// session store is unusable.
    pub fn signed_in() -> Result<(ViewerState, AccessToken), ViewError> {
        let state = use_context::<ViewerState>().ok_or(ViewError::Unavailable)?;
        let parts = use_context::<http::request::Parts>().ok_or(ViewError::SignedOut)?;
        let id = crate::oidc::cookie_named(&parts.headers, crate::session::COOKIE)
            .ok_or(ViewError::SignedOut)?;
        let token = state
            .sessions()
            .access_token(&id)
            .map_err(|_unusable| ViewError::Unavailable)?
            .ok_or(ViewError::SignedOut)?;
        Ok((state, AccessToken::new(token)))
    }

    /// The view error of a gateway call that failed, logged with no token
    /// and no body.
    #[must_use]
    pub fn refused(error: &GatewayError) -> ViewError {
        if let Some((status, code)) = error.refusal() {
            tracing::warn!(%status, code = code.as_deref(), "the gateway refused a view");
            ViewError::Refused {
                status: status.as_u16(),
                code,
            }
        } else {
            tracing::error!(error = %error, "the gateway could not be reached for a view");
            ViewError::Unreachable
        }
    }

    /// The members view of `description` and `dependencies`.
    #[must_use]
    pub fn members(description: &OptionsRoot, dependencies: &Dependencies) -> MembersView {
        let members = description
            .endpoints
            .iter()
            .map(|endpoint| {
                let id = endpoint.id.to_string();
                MemberRow {
                    health: dependencies
                        .endpoints
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| String::from("not reported")),
                    endpoint_id: id,
                    organisation: endpoint.organisation.clone(),
                    status: endpoint.status.as_str().to_owned(),
                    node_id: endpoint.node_id.clone(),
                    system_id: endpoint.system_id.clone(),
                    product: match (&endpoint.product, &endpoint.version) {
                        (Some(product), Some(version)) => Some(format!("{product} {version}")),
                        (Some(product), None) => Some(product.clone()),
                        (None, _) => None,
                    },
                    latency_ms_p50: endpoint.latency_ms_p50,
                }
            })
            .collect();
        MembersView {
            members,
            services: dependencies
                .services
                .iter()
                .map(|(key, state)| (key.clone(), state.clone()))
                .collect(),
        }
    }

    /// The integrity view of `incidents` and `routes`.
    #[must_use]
    pub fn integrity(incidents: &IncidentReport, routes: &CreatingSystemReport) -> IntegrityView {
        IntegrityView {
            counts: incidents
                .counts
                .iter()
                .map(|(kind, count)| (kind.clone(), *count))
                .collect(),
            recent: incidents
                .recent
                .iter()
                .rev()
                .map(|recorded| IncidentRow {
                    at: recorded.at.clone(),
                    kind: recorded.kind.clone(),
                    description: recorded.description.clone(),
                })
                .collect(),
            routes: routes
                .entries
                .iter()
                .map(|entry| RouteRow {
                    creating_system_id: entry.creating_system_id.clone(),
                    source: match entry.source {
                        ferrofed_registry::operator::RouteSource::Member => "member",
                        ferrofed_registry::operator::RouteSource::Registered => "registered",
                        ferrofed_registry::operator::RouteSource::Learned => "learned",
                        ferrofed_registry::operator::RouteSource::Withdrawn => "withdrawn",
                    }
                    .to_owned(),
                    node: entry.node.clone(),
                    endpoint: entry.endpoint.clone(),
                })
                .collect(),
        }
    }

    /// The stored-query view of `report`.
    #[must_use]
    pub fn stored(report: &StoredQueryReport) -> StoredView {
        StoredView {
            definitions: report
                .definitions
                .iter()
                .map(|entry| StoredRow {
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    saved: entry.saved.clone(),
                    aql: entry.aql.clone(),
                })
                .collect(),
        }
    }

    /// The self-description view of `description`.
    ///
    /// # Errors
    /// Returns [`ViewError::Unavailable`] when the description cannot be
    /// written back as JSON.
    pub fn federation(description: &OptionsRoot) -> Result<FederationView, ViewError> {
        let document = serde_json::to_string_pretty(&description.federation)
            .map_err(|_unwritable| ViewError::Unavailable)?;
        Ok(FederationView {
            id: description.federation.id.to_string(),
            spec_version: description.federation.spec_version.to_string(),
            endpoints: u32::try_from(description.endpoints.len()).unwrap_or(u32::MAX),
            document,
        })
    }
}
