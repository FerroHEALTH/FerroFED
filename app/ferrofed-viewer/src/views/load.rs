// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server functions each view loads through.
//!
//! Each is a public HTTP endpoint of the console as well as a call the
//! server makes while it renders, so each checks the operator's session
//! itself before it asks the gateway anything, and asks the gateway as the
//! operator, with the operator's own access token (`server/25_server_functions`,
//! the Leptos book). The bodies run on the server alone.
//!
//! A refusal the gateway answers is an [`Outcome::Refused`], a successful
//! answer of the function; only a fault of the console, its session or its
//! reach to the gateway is a [`ViewError`].

use leptos::prelude::*;

use crate::views::model::{
    FederationView, IntegrityView, MembersView, Outcome, StoredView, ViewError,
};

/// Loads the members view: `OPTIONS {base}/` and the health of every member
/// and service.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// console's fault as its [`ViewError`]; the gateway's refusal is an
/// [`Outcome::Refused`].
#[server(endpoint = "members")]
pub async fn members() -> Result<Outcome<MembersView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let gateway = state.gateway();
        let description = gateway.self_description(&token).await?;
        let dependencies = gateway.dependencies(&token).await?;
        Ok(server::members(&description, &dependencies))
    })
    .await
}

/// Loads the integrity view: the incidents and the page of the routing
/// table that starts at `offset`.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "integrity")]
pub async fn integrity(
    /// Where the page of the routing table starts.
    offset: u64,
) -> Result<Outcome<IntegrityView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let gateway = state.gateway();
        let incidents = gateway.incidents(&token).await?;
        let routes = gateway
            .creating_systems(&token, server::page(offset))
            .await?;
        Ok(server::integrity(&incidents, &routes))
    })
    .await
}

/// Loads the page of the stored-query view that starts at `offset`.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "stored-queries")]
pub async fn stored_queries(
    /// Where the page of held versions starts.
    offset: u64,
) -> Result<Outcome<StoredView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let report = state
            .gateway()
            .stored_queries(&token, server::page(offset))
            .await?;
        Ok(server::stored(&report))
    })
    .await
}

/// Loads the self-description view, `OPTIONS {base}/`.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "federation")]
pub async fn federation() -> Result<Outcome<FederationView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let description = state.gateway().self_description(&token).await?;
        Ok(server::federation(&description)?)
    })
    .await
}

/// The server half of the views: the session check, the split of a failed
/// gateway call into a refusal or a fault, and the mapping of the gateway's
/// bodies onto the view models.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use ferrofed_registry::health::DependencyReport;
    use ferrofed_registry::operator::{
        CreatingSystemEntry, IncidentReport, MAX_PAGE, Page, PageRequest, RouteSource,
    };
    use leptos::context::use_context;
    use openehr_federation::options::OptionsRoot;
    use openehr_its::rest::generated::definition::StoredQuery;

    use crate::gateway::{AccessToken, GatewayError};
    use crate::server::ViewerState;
    use crate::views::model::{
        FederationView, IncidentRow, IntegrityView, MemberRow, MembersView, Outcome, PAGE_SIZE,
        Refusal, RouteRow, StoredRow, StoredView, ViewError,
    };

    /// Why a server function stops short of its view: an expected refusal,
    /// which it answers, or a fault of the console, which fails it.
    #[derive(Debug)]
    pub enum Stop {
        /// The gateway refused, or the input cannot be sent.
        Refused(Refusal),
        /// The console, its session or its reach to the gateway failed.
        Fault(ViewError),
    }

    impl From<Refusal> for Stop {
        fn from(refusal: Refusal) -> Self {
            Self::Refused(refusal)
        }
    }

    impl From<ViewError> for Stop {
        fn from(error: ViewError) -> Self {
            Self::Fault(error)
        }
    }

    impl From<GatewayError> for Stop {
        fn from(error: GatewayError) -> Self {
            refused(&error)
        }
    }

    /// The answer of a server function whose work is `work`: its view, or
    /// the refusal that stopped it, and a fault as the function's error.
    ///
    /// # Errors
    /// Returns the [`ViewError`] of a fault.
    pub async fn settled<T>(
        work: impl Future<Output = Result<T, Stop>>,
    ) -> Result<Outcome<T>, ViewError> {
        match work.await {
            Ok(view) => Ok(Outcome::Shown(view)),
            Err(Stop::Refused(refusal)) => Ok(Outcome::Refused(refusal)),
            Err(Stop::Fault(error)) => Err(error),
        }
    }

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
        let id = crate::oidc::cookie_named(
            &parts.headers,
            &state.sessions().cookie_name(crate::session::COOKIE),
        )
        .ok_or(ViewError::SignedOut)?;
        let token = state
            .sessions()
            .access_token(&id)
            .map_err(|_unusable| ViewError::Unavailable)?
            .ok_or(ViewError::SignedOut)?;
        Ok((state, AccessToken::new(token)))
    }

    /// What stopped a gateway call that failed, logged with no token and no
    /// body: the gateway's refusal, or the console's fault of a body it
    /// cannot read or a gateway it cannot reach.
    #[must_use]
    pub fn refused(error: &GatewayError) -> Stop {
        if let Some(status) = error.unreadable() {
            tracing::error!(%status, "the gateway answered a view with a body this console cannot read");
            return Stop::Fault(ViewError::Unreadable {
                status: status.as_u16(),
            });
        }
        if let Some((status, code)) = error.refusal() {
            tracing::warn!(%status, code = code.as_deref(), "the gateway refused a view");
            if status == http::StatusCode::UNAUTHORIZED {
                return Stop::Refused(Refusal::NotAuthenticated { code });
            }
            Stop::Refused(Refusal::Gateway {
                status: status.as_u16(),
                code,
            })
        } else {
            tracing::error!(error = %error, "the gateway could not be reached for a view");
            Stop::Fault(ViewError::Unreachable)
        }
    }

    /// The members view of `description` and `dependencies`.
    #[must_use]
    pub fn members(description: &OptionsRoot, dependencies: &DependencyReport) -> MembersView {
        let members = description
            .endpoints
            .iter()
            .map(|endpoint| {
                let id = endpoint.id.to_string();
                MemberRow {
                    health: dependencies.endpoints.get(&id).map_or_else(
                        || String::from("not reported"),
                        |observed| observed.as_str().to_owned(),
                    ),
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
                .services()
                .into_iter()
                .map(|(key, state)| (key.to_owned(), state.to_owned()))
                .collect(),
        }
    }

    /// The integrity view of `incidents` and `routes`.
    #[must_use]
    pub fn integrity(
        incidents: &IncidentReport,
        routes: &Page<CreatingSystemEntry>,
    ) -> IntegrityView {
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
            routes_offset: routes.offset,
            routes_total: routes.total,
            routes: routes
                .items
                .iter()
                .map(|entry| RouteRow {
                    creating_system_id: entry.creating_system_id.clone(),
                    source: match entry.source {
                        RouteSource::Member => "member",
                        RouteSource::Registered => "registered",
                        RouteSource::Learned => "learned",
                        RouteSource::Withdrawn => "withdrawn",
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
    pub fn stored(report: &Page<StoredQuery>) -> StoredView {
        StoredView {
            definitions: report
                .items
                .iter()
                .map(|entry| StoredRow {
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    saved: entry.saved.clone(),
                    aql: entry.q.clone(),
                })
                .collect(),
            offset: report.offset,
            total: report.total,
        }
    }

    // NOTE: no specification governs this: our own design; a view never asks
    // for more than the gateway answers in one page.
    const _: () = assert!(
        PAGE_SIZE <= MAX_PAGE && PAGE_SIZE > 0,
        "a view asks for a page the gateway answers"
    );

    /// The page of at most [`PAGE_SIZE`] items that starts at `offset`.
    #[must_use]
    pub fn page(offset: u64) -> PageRequest {
        PageRequest {
            offset,
            limit: PAGE_SIZE,
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
