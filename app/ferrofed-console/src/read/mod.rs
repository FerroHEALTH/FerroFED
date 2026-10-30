// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What each page reads of the gateway, as the signed-in operator.
//!
//! Every read runs on the server as the handler draws the page, before the
//! page is sent. Each read uses the operator's own access token, which
//! never leaves the server. A read that
//! fails stops short of its view in one of two ways ([`Stop`]): the
//! gateway's refusal, an expected outcome a page shows in words with its
//! status and stable code ([`Refusal`]), or a fault of the console, its
//! session or its reach to the gateway ([`Fault`]). Neither is ever drawn
//! as an empty view. [`federation`] reads the organisations, nodes,
//! endpoints and services the dashboard, the topology and the federation's
//! pages draw. No specification governs the console's reads: our own
//! design.

pub mod federation;
pub mod incidents;

use std::collections::BTreeSet;

use ferrofed_registry::operator::{DeploymentMode, DeploymentReport};
use ferrofed_registry::replica::UNNAMED;
use serde::{Deserialize, Serialize};

use crate::gateway::{AccessToken, GatewayError};
use crate::locale::{Msg, tr, tr_with};
use crate::server::ConsoleState;

/// The most rows a paged read asks the gateway for at once.
pub const PAGE_SIZE: u64 = 100;

/// An expected refusal: the gateway's, with its status and stable code, or
/// the console's of input it cannot send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Refusal {
    /// The gateway refused the request, with its status and its stable
    /// error code.
    Gateway {
        /// The status the gateway answered with.
        status: u16,
        /// The stable error code of its body, when it carried one.
        code: Option<String>,
    },
    /// The gateway did not accept the operator's access token (`401`), so
    /// the operator signs in again.
    NotAuthenticated {
        /// The stable error code of its body, when it carried one.
        code: Option<String>,
    },
    /// The operator's input cannot be sent as it stands. The reason names
    /// the field and never quotes what was entered (N33).
    Invalid {
        /// What is wrong, by field.
        reason: String,
    },
}

/// Why the console could not serve a page: a fault of its own, of its
/// session, or of its reach to the gateway, never an expected refusal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum Fault {
    /// The request carries no live signed-in session.
    #[error("sign in to see this page")]
    SignedOut,
    /// The gateway answered with a body this console cannot read.
    #[error("the gateway answered {status} with a body this console cannot read")]
    Unreadable {
        /// The status the gateway answered with.
        status: u16,
    },
    /// The gateway gave no answer at all.
    #[error("the gateway could not be reached")]
    Unreachable,
    /// The console could not serve the page, such as when its session
    /// store cannot be read.
    #[error("the console could not serve this page")]
    Unavailable,
}

/// Why a read stops short of its view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stop {
    /// The gateway refused, or the input cannot be sent.
    Refused(Refusal),
    /// The console, its session or its reach to the gateway failed.
    Fault(Fault),
}

impl From<Refusal> for Stop {
    fn from(refusal: Refusal) -> Self {
        Self::Refused(refusal)
    }
}

impl From<Fault> for Stop {
    fn from(fault: Fault) -> Self {
        Self::Fault(fault)
    }
}

impl From<GatewayError> for Stop {
    fn from(error: GatewayError) -> Self {
        stopped(&error, Asked::View)
    }
}

/// The console's state and the operator's access token, for a request that
/// carries a live signed-in session.
///
/// # Errors
/// Returns [`Fault::SignedOut`] when it carries none, and
/// [`Fault::Unavailable`] when the console's state is missing or its
/// session store is unusable.
pub fn signed_in() -> Result<(ConsoleState, AccessToken), Fault> {
    crate::visit::with(|visit| {
        let state = visit.state().ok_or(Fault::Unavailable)?;
        signed_in_with(state, visit.headers()).map(|token| (state.clone(), token))
    })
}

/// The operator's access token for a request whose headers are `headers`,
/// on the console `state`.
///
/// # Errors
/// As [`signed_in`].
pub fn signed_in_with(
    state: &ConsoleState,
    headers: &http::HeaderMap,
) -> Result<AccessToken, Fault> {
    let id = crate::oidc::cookie_named(
        headers,
        &state.sessions().cookie_name(crate::session::COOKIE),
    )
    .ok_or(Fault::SignedOut)?;
    let token = state
        .sessions()
        .access_token(&id)
        .map_err(|_unusable| Fault::Unavailable)?
        .ok_or(Fault::SignedOut)?;
    Ok(AccessToken::new(token))
}

/// The name of the operator of the live signed-in session of a request
/// whose headers are `headers`, on the console `state`, as the provider
/// gave it.
#[must_use]
pub fn operator_of(state: &ConsoleState, headers: &http::HeaderMap) -> Option<String> {
    let id = crate::oidc::cookie_named(
        headers,
        &state.sessions().cookie_name(crate::session::COOKIE),
    )?;
    // NOTE: no specification governs this: our own design; a store that cannot be read
    // names no operator, and the session check of every page reports that fault itself.
    state.sessions().operator(&id).ok().flatten()
}

/// What a failed gateway call asked for, as its log line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Asked<'a> {
    /// A page's read of the gateway.
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

/// What stopped the gateway call `asked` that failed.
///
/// It is the gateway's refusal, or the console's fault of a body it cannot
/// read or a gateway it cannot reach, logged by what the call asked for,
/// with no token, no body and nothing the call named.
#[must_use]
pub fn stopped(error: &GatewayError, asked: Asked<'_>) -> Stop {
    if let Some(status) = error.unreadable() {
        match asked {
            Asked::View => {
                tracing::error!(%status, "the gateway answered a view with a body this console cannot read");
            }
            Asked::Status => tracing::error!(
                %status,
                "the gateway answered the status read with a body this console cannot read"
            ),
            Asked::Query => {
                tracing::error!(%status, "the gateway answered a query with a body this console cannot read");
            }
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
        return Stop::Fault(Fault::Unreadable {
            status: status.as_u16(),
        });
    }
    if let Some((status, code)) = error.refusal() {
        match asked {
            Asked::View => {
                tracing::warn!(%status, code = code.as_deref(), "the gateway refused a view");
            }
            Asked::Status => {
                tracing::warn!(%status, code = code.as_deref(), "the gateway refused the status read");
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
        return Stop::Refused(Refusal::Gateway {
            status: status.as_u16(),
            code,
        });
    }
    match asked {
        Asked::View => {
            tracing::error!(error = %error, "the gateway could not be reached for a view");
        }
        Asked::Status => {
            tracing::error!(error = %error, "the gateway could not be reached for the status read");
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
    Stop::Fault(Fault::Unreachable)
}

/// The line a page shows of the replicas that answered its `reports`, each
/// named by what it holds with the replica its body names, or `None` when
/// the one replica that answered has no name.
#[must_use]
pub fn answered_by(reports: &[(&str, Option<&str>)]) -> Option<String> {
    let named: BTreeSet<Option<&str>> = reports.iter().map(|(_, replica)| *replica).collect();
    match named.into_iter().collect::<Vec<_>>().as_slice() {
        [Some(UNNAMED)] => None,
        [Some(replica)] => Some((*replica).to_owned()),
        [None] => Some(tr(Msg::ReplicaUnnamed)),
        _ => {
            let parts: Vec<String> = reports
                .iter()
                .map(|(what, replica)| {
                    tr_with(
                        Msg::ReplicaOfPart,
                        &[
                            ("what", (*what).into()),
                            ("replica", replica.unwrap_or("?").into()),
                        ],
                    )
                })
                .collect();
            Some(parts.join(", "))
        }
    }
}

/// How the gateway that answered the console is deployed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    /// Whether the gateway runs the development profile.
    pub development: bool,
    /// The gateway's product version.
    pub version: String,
    /// The replica that answered; `None` when it has no name.
    pub replica: Option<String>,
}

impl From<&DeploymentReport> for Deployment {
    fn from(report: &DeploymentReport) -> Self {
        Self {
            development: report.mode == DeploymentMode::Development,
            version: report.version.clone(),
            replica: (report.replica != UNNAMED).then(|| report.replica.clone()),
        }
    }
}

/// Reads how the gateway is deployed, as the operator of the request.
///
/// # Errors
/// Returns the [`Stop`] of a missing session, or of a read the gateway
/// refused or did not answer.
pub async fn deployment() -> Result<Deployment, Stop> {
    let (state, token) = signed_in()?;
    let report = state
        .gateway()
        .deployment(&token)
        .await
        .map_err(|error| stopped(&error, Asked::Status))?;
    Ok(Deployment::from(&report))
}

/// The federation and the specification version the gateway's
/// self-description declares, read as the status of the gateway when its
/// deployment cannot be read.
///
/// # Errors
/// Returns the [`Stop`] of a self-description the gateway refused or did
/// not answer.
pub async fn described() -> Result<(String, String), Stop> {
    let (state, token) = signed_in()?;
    let description = state
        .gateway()
        .self_description(&token)
        .await
        .map_err(|error| stopped(&error, Asked::Status))?;
    Ok((
        description.federation.id.to_string(),
        description.federation.spec_version.to_string(),
    ))
}

/// The page of at most `limit` items that starts at `offset`, held to the
/// most the gateway answers at once.
#[must_use]
pub fn stored_page(offset: u64, limit: u64) -> ferrofed_registry::operator::PageRequest {
    ferrofed_registry::operator::PageRequest {
        offset,
        limit: limit.clamp(1, PAGE_SIZE),
    }
}

#[cfg(test)]
mod tests {
    use super::answered_by;

    #[test]
    fn a_replica_with_no_name_drops_the_line_and_two_replicas_are_each_named() {
        assert_eq!(None, answered_by(&[("health", Some("unnamed"))]));
        assert_eq!(
            Some(String::from("gateway-0")),
            answered_by(&[("health", Some("gateway-0"))])
        );
        assert_eq!(
            Some(String::from("a replica that gave no name")),
            answered_by(&[("health", None)])
        );
        assert_eq!(
            Some(String::from(
                "the incidents by gateway-0, the routing table by unnamed"
            )),
            answered_by(&[
                ("incidents", Some("gateway-0")),
                ("routing table", Some("unnamed"))
            ])
        );
    }
}
