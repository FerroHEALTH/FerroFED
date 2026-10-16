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
    DeploymentView, GatewayView, IntegrityView, Outcome, RoutingView, ViewError,
};

/// Loads the members view, rendered on the server: `OPTIONS {base}/` and
/// the health of every member and service.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, and the
/// console's fault as its [`ViewError`]; the gateway's refusal is an
/// [`Outcome::Refused`].
#[server(endpoint = "members")]
pub async fn members() -> Result<Outcome<String>, ViewError> {
    server::settled(async {
        let view = server::members_read().await?;
        Ok(crate::views::members::members_html(view))
    })
    .await
}

/// Loads the page of the member keyed `key`, its `node_id` or the
/// `endpoint_id` of an endpoint that names no node, rendered on the server;
/// `None` when the gateway declares no such member.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "member")]
pub async fn member(
    /// The member's key.
    key: String,
) -> Result<Outcome<Option<String>>, ViewError> {
    server::settled(async {
        let view = server::members_read().await?;
        Ok(crate::views::members::member_html(&key, view))
    })
    .await
}

/// Loads the integrity view: the incidents in the state `show` names
/// (`resolved`, or the open ones for any other word), rendered on the
/// server, and the collision clear's fields.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "integrity")]
pub async fn integrity(
    /// Which incidents to show.
    show: String,
) -> Result<Outcome<IntegrityView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let incidents = state.gateway().incidents(&token).await?;
        Ok(server::integrity(
            &incidents,
            &state.settings().admin,
            show == "resolved",
        ))
    })
    .await
}

/// Loads the page of the `creating_system_id` routing table that starts at
/// `offset`.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "routing")]
pub async fn routing(
    /// Where the page of the routing table starts.
    offset: u64,
) -> Result<Outcome<RoutingView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let routes = state
            .gateway()
            .creating_systems(&token, server::page(offset))
            .await?;
        Ok(server::routing(&routes))
    })
    .await
}

/// Loads the dashboard, rendered on the server: the counts of the members,
/// the nodes, the open incidents, the stored queries and the running
/// revision, each endpoint's health and the recent incidents.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "dashboard")]
pub async fn dashboard() -> Result<Outcome<String>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let board = crate::dashboard::gather(&state, &token).await?;
        Ok(crate::dashboard::render(&board))
    })
    .await
}

/// Loads what the top bar says of the gateway: its self-description,
/// `OPTIONS {base}/`, read as the operator.
///
/// # Errors
/// As [`members`].
#[server(endpoint = "gateway")]
pub async fn gateway_status() -> Result<Outcome<GatewayView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let description = state
            .gateway()
            .self_description(&token)
            .await
            .map_err(|error| server::stopped(&error, server::Asked::Status))?;
        Ok(server::gateway(&description))
    })
    .await
}

/// Loads how the gateway is deployed, `GET {base}/operator/deployment`,
/// read as the operator: the development banner and the version in the
/// top bar's pill.
///
/// # Errors
/// As [`members`]; an operator without the operator scope gets the
/// gateway's refusal, which the top bar shows as no banner and no version.
#[server(endpoint = "deployment")]
pub async fn deployment() -> Result<Outcome<DeploymentView>, ViewError> {
    server::settled(async {
        let (state, token) = server::signed_in()?;
        let report = state
            .gateway()
            .deployment(&token)
            .await
            .map_err(|error| server::stopped(&error, server::Asked::Status))?;
        Ok(server::deployment(&report))
    })
    .await
}

/// The server half of the views: the session check, the split of a failed
/// gateway call into a refusal or a fault, and the mapping of the gateway's
/// bodies onto the view models.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use ferrofed_registry::health::DependencyReport;
    use ferrofed_registry::id::{EndpointId, NodeId};
    use ferrofed_registry::operator::{
        CreatingSystemEntry, DeploymentMode, DeploymentReport, IncidentReport, MAX_PAGE, Page,
        PageRequest, RecordedClear, RecordedIncident, RouteSource,
    };
    use ferrofed_registry::replica::UNNAMED;
    use leptos::context::use_context;
    use openehr_federation::options::OptionsRoot;

    use std::collections::BTreeSet;

    use crate::config::settings::ReplicaSettings;
    use crate::gateway::{AccessToken, GatewayError};
    use crate::server::ViewerState;
    use crate::views::integrity::Incidents;
    use crate::views::model::{
        ClearedRow, DeploymentView, GatewayView, IncidentRow, IntegrityView, MemberRow,
        MembersView, Outcome, PAGE_SIZE, Refusal, RouteRow, RoutingView, ViewError,
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

    /// The name of the operator of the request's live signed-in session, as
    /// the provider gave it.
    #[must_use]
    pub fn operator() -> Option<String> {
        let state = use_context::<ViewerState>()?;
        let parts = use_context::<http::request::Parts>()?;
        let id = crate::oidc::cookie_named(
            &parts.headers,
            &state.sessions().cookie_name(crate::session::COOKIE),
        )?;
        // NOTE: no specification governs this: our own design; a store that
        // cannot be read names no operator, and the session check of every
        // page and server function reports that fault itself.
        state.sessions().operator(&id).ok().flatten()
    }

    /// What a failed gateway call asked for, as its log line names it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Asked<'a> {
        /// A view's read of the gateway.
        View,
        /// The top bar's read of the gateway's status.
        Status,
        /// An operator's AQL or stored query through the gateway.
        Query,
        /// An operator's action at a replica's admin listener.
        Action {
            /// Which page's action it is.
            kind: ActionKind,
            /// The action, such as `clear` or `member save`.
            what: &'a str,
            /// The replica's name.
            replica: &'a str,
        },
    }

    /// Which page an operator's action at a replica's admin listener comes
    /// from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ActionKind {
        /// An admin action: a collision clear or a drift repair.
        Admin,
        /// A change of the gateway's configuration.
        Configuration,
    }

    impl ActionKind {
        /// The action's kind, as a log line names it.
        #[must_use]
        pub const fn noun(self) -> &'static str {
            match self {
                Self::Admin => "admin action",
                Self::Configuration => "configuration action",
            }
        }
    }

    /// What stopped a view's gateway call that failed, as [`stopped`] logs
    /// and splits it.
    #[must_use]
    pub fn refused(error: &GatewayError) -> Stop {
        stopped(error, Asked::View)
    }

    /// What stopped the gateway call `asked` that failed.
    ///
    /// It is the gateway's refusal, or the console's fault of a body it
    /// cannot read or a gateway it cannot reach, logged by what the call
    /// asked for, with no token, no body and nothing the call named.
    #[must_use]
    pub fn stopped(error: &GatewayError, asked: Asked<'_>) -> Stop {
        if let Some(status) = error.unreadable() {
            match asked {
                Asked::View => tracing::error!(
                    %status,
                    "the gateway answered a view with a body this console cannot read"
                ),
                Asked::Status => tracing::error!(
                    %status,
                    "the gateway answered the status read with a body this console cannot read"
                ),
                Asked::Query => tracing::error!(
                    %status,
                    "the gateway answered a query with a body this console cannot read"
                ),
                Asked::Action {
                    kind,
                    what,
                    replica,
                } => tracing::error!(
                    %status,
                    replica,
                    action = what,
                    "a replica answered an operator's {} with a body this console cannot read",
                    kind.noun()
                ),
            }
            return Stop::Fault(ViewError::Unreadable {
                status: status.as_u16(),
            });
        }
        if let Some((status, code)) = error.refusal() {
            match asked {
                Asked::View => {
                    tracing::warn!(%status, code = code.as_deref(), "the gateway refused a view");
                }
                Asked::Status => {
                    tracing::warn!(
                        %status,
                        code = code.as_deref(),
                        "the gateway refused the status read"
                    );
                }
                Asked::Query => {
                    tracing::warn!(%status, code = code.as_deref(), "the gateway refused a query");
                }
                Asked::Action {
                    kind,
                    what,
                    replica,
                } => tracing::warn!(
                    %status,
                    code = code.as_deref(),
                    replica,
                    action = what,
                    "a replica refused an operator's {}",
                    kind.noun()
                ),
            }
            if status == http::StatusCode::UNAUTHORIZED {
                return Stop::Refused(Refusal::NotAuthenticated { code });
            }
            Stop::Refused(Refusal::Gateway {
                status: status.as_u16(),
                code,
            })
        } else {
            match asked {
                Asked::View => {
                    tracing::error!(error = %error, "the gateway could not be reached for a view");
                }
                Asked::Status => {
                    tracing::error!(
                        error = %error,
                        "the gateway could not be reached for the status read"
                    );
                }
                Asked::Query => {
                    tracing::error!(error = %error, "the gateway could not be reached for a query");
                }
                Asked::Action {
                    kind,
                    what,
                    replica,
                } => tracing::error!(
                    error = %error,
                    replica,
                    action = what,
                    "a replica's admin listener could not be reached for an operator's {}",
                    kind.noun()
                ),
            }
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
                    detail: detail(endpoint.node_id.as_deref(), endpoint.id.as_str()),
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
            answered_by: answered_by(&[("health", dependencies.replica.as_deref())]),
        }
    }

    /// The path of the page of the member `node`, or of the endpoint
    /// `endpoint` that names no node, when the id is one a path carries as
    /// it stands: the registry's own form of a `node_id` or an
    /// `endpoint_id`.
    #[must_use]
    pub fn detail(node: Option<&str>, endpoint: &str) -> Option<String> {
        // NOTE: no specification governs this: our own design; an id outside the
        // registry's form gets no page, so no path is ever escaped or guessed.
        let key = match node {
            Some(node) => NodeId::new(node).ok()?.to_string(),
            None => EndpointId::new(endpoint).ok()?.to_string(),
        };
        Some(format!("{}/{key}", crate::views::MEMBERS))
    }

    /// The incidents of `report`, newest first, each marked resolved when
    /// the operator cleared the `ehr_id` it is about after it was emitted.
    #[must_use]
    pub fn incidents(report: &IncidentReport) -> Vec<IncidentRow> {
        report
            .recent
            .iter()
            .rev()
            .map(|recorded| IncidentRow {
                at: recorded.at.clone(),
                kind: recorded.kind.clone(),
                description: recorded.description.clone(),
                resolved: resolved(recorded, &report.cleared),
            })
            .collect()
    }

    /// Whether one of `cleared` clears the `ehr_id` `incident` is about, at
    /// or after the time it was emitted.
    fn resolved(incident: &RecordedIncident, cleared: &[RecordedClear]) -> bool {
        let Some(ehr_id) = incident.ehr_id.as_deref() else {
            return false;
        };
        // NOTE: no specification governs this: our own design; a time that does not
        // parse proves no clear came after, so the incident stays open.
        let Ok(emitted) = incident.at.parse::<jiff::Timestamp>() else {
            return false;
        };
        cleared.iter().any(|clear| {
            clear.ehr_id.as_deref() == Some(ehr_id)
                && clear
                    .at
                    .parse::<jiff::Timestamp>()
                    .is_ok_and(|at| at >= emitted)
        })
    }

    /// The integrity view of `incidents`.
    ///
    /// It lists the incidents and the collisions cleared beside them, and
    /// offers the collision clear at `replicas`.
    #[must_use]
    pub fn integrity(
        incidents: &IncidentReport,
        replicas: &[ReplicaSettings],
        resolved: bool,
    ) -> IntegrityView {
        let ehr_ids: BTreeSet<String> = incidents
            .recent
            .iter()
            .filter_map(|recorded| recorded.ehr_id.clone())
            .collect();
        let ehr_ids: Vec<String> = ehr_ids.into_iter().collect();
        let shown = Incidents {
            counts: incidents
                .counts
                .iter()
                .map(|(kind, count)| (kind.clone(), *count))
                .collect(),
            recent: self::incidents(incidents),
            cleared: incidents
                .cleared
                .iter()
                .rev()
                .map(|cleared| ClearedRow {
                    at: cleared.at.clone(),
                    ehr_id: cleared.ehr_id.clone(),
                    claimants: cleared.claimants.clone(),
                })
                .collect(),
            answered_by: answered_by(&[("incidents", incidents.replica.as_deref())]),
            resolved,
        };
        IntegrityView {
            html: crate::views::integrity::integrity_html(&shown),
            clear_form: (!replicas.is_empty())
                .then(|| crate::admin::fields::clear(replicas, &ehr_ids)),
        }
    }

    /// The members view, read as the operator of the request's session.
    ///
    /// # Errors
    /// Returns the [`Stop`] of a missing session, or of a read the gateway
    /// refused or did not answer.
    pub async fn members_read() -> Result<MembersView, Stop> {
        let (state, token) = signed_in()?;
        let gateway = state.gateway();
        let description = gateway.self_description(&token).await?;
        let dependencies = gateway.dependencies(&token).await?;
        Ok(members(&description, &dependencies))
    }

    /// The routing view of the page `routes`.
    #[must_use]
    pub fn routing(routes: &Page<CreatingSystemEntry>) -> RoutingView {
        RoutingView {
            offset: routes.offset,
            total: routes.total,
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
            answered_by: answered_by(&[("routing table", routes.replica.as_deref())]),
        }
    }

    /// The line a view shows of the replicas that answered its `reports`,
    /// each named by what it holds with the replica its body names, or
    /// `None` when the one replica that answered has no name.
    ///
    /// The console reads one replica through the address it is configured
    /// with, and a view that reads two reports can be answered by two
    /// replicas, so the line names each when they differ, and says so when
    /// a report names none.
    #[must_use]
    pub fn answered_by(reports: &[(&str, Option<&str>)]) -> Option<String> {
        let named: BTreeSet<Option<&str>> = reports.iter().map(|(_, replica)| *replica).collect();
        match named.into_iter().collect::<Vec<_>>().as_slice() {
            [Some(UNNAMED)] => None,
            [Some(replica)] => Some(format!("Answered by replica {replica}.")),
            [None] => Some(String::from(
                "The gateway did not name the replica that answered.",
            )),
            _ => {
                let parts: Vec<String> = reports
                    .iter()
                    .map(|(what, replica)| match replica {
                        Some(UNNAMED) => format!("the {what} by a replica that has no name"),
                        Some(replica) => format!("the {what} by replica {replica}"),
                        None => format!("the {what} by a replica that gave no name"),
                    })
                    .collect();
                Some(format!(
                    "Each part names its replica: {}.",
                    parts.join(", ")
                ))
            }
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

    /// What the top bar and its banner say of the deployment `report`.
    #[must_use]
    pub fn deployment(report: &DeploymentReport) -> DeploymentView {
        DeploymentView {
            development: report.mode == DeploymentMode::Development,
            development_crossref: report.development_crossref,
            version: report.version.clone(),
            replica: (report.replica != UNNAMED).then(|| report.replica.clone()),
        }
    }

    /// What the top bar says of the gateway that answered `description`.
    #[must_use]
    pub fn gateway(description: &OptionsRoot) -> GatewayView {
        GatewayView {
            spec_version: description.federation.spec_version.to_string(),
            federation: description.federation.id.to_string(),
        }
    }
}
