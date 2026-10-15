// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The server functions of the admin actions: the clear of a held `ehr_id`
//! and the stored-query drift repair.
//!
//! Each is a public HTTP endpoint of the console, so each checks the
//! operator's session itself, then that the request came from the hydrated
//! page, then the form and its confirmation, before any replica is asked
//! (`server/25_server_functions`, the Leptos book). Each replica is called
//! with the operator's own access token, which never leaves the server, and
//! the answer names every replica the action went to.

use leptos::prelude::*;

use crate::admin::model::{ActionAnswer, AdminForm};
use crate::views::model::ViewError;

/// Clears the `ehr_id` `form` names at the replica it names, or at every
/// replica one after another, as the operator.
///
/// # Errors
/// Returns [`ViewError::SignedOut`] without a live session, the console's
/// fault as its [`ViewError`], and [`ViewError::PlainPost`] for a plain form
/// post, before any replica is asked. A form that cannot be sent and a
/// replica's refusal are answers.
#[server(endpoint = "clear-ehr-id")]
pub async fn clear_ehr_id(
    /// The collision clear's form.
    form: AdminForm,
) -> Result<ActionAnswer, ViewError> {
    let (state, token) = crate::views::load::server::signed_in()?;
    crate::query::run::server::from_the_hydrated_page()?;
    let planned = match server::clear(&form, &state.settings().admin) {
        Ok(planned) => planned,
        Err(refused) => return Ok(crate::admin::answer::refused(&refused)),
    };
    let mut answers = Vec::with_capacity(planned.replicas.len());
    for replica in planned.replicas {
        let answered = state
            .gateway()
            .clear_ehr_id(replica, &token, &planned.ehr_id)
            .await;
        answers.push(server::acted(
            &replica.name,
            "clear",
            answered,
            server::cleared,
        ));
    }
    Ok(crate::admin::answer::cleared(&answers))
}

/// Sends the stored-query version `form` names again to the members it
/// names, through the first replica, as the operator (§12.7
/// stored-query-drift).
///
/// # Errors
/// As [`clear_ehr_id`].
#[server(endpoint = "repair-drift")]
pub async fn repair_drift(
    /// The drift repair's form.
    form: AdminForm,
) -> Result<ActionAnswer, ViewError> {
    let (state, token) = crate::views::load::server::signed_in()?;
    crate::query::run::server::from_the_hydrated_page()?;
    let (replica, sent) = match server::repair(&form, &state.settings().admin) {
        Ok(planned) => planned,
        Err(refused) => return Ok(crate::admin::answer::refused(&refused)),
    };
    let answered = state.gateway().repair_drift(replica, &token, &sent).await;
    let answer = server::acted(&replica.name, "drift repair", answered, server::repaired);
    Ok(crate::admin::answer::repaired(&answer))
}

/// The server half of the admin actions: the form read into what is sent
/// and where, and each replica's answer read into what the console renders.
#[cfg(not(target_arch = "wasm32"))]
pub mod server {
    use ferrofed_registry::operator::{ClearedEhrId, HeldBefore};
    use http::HeaderValue;

    use crate::admin::model::AdminForm;
    use crate::config::settings::ReplicaSettings;
    use crate::gateway::GatewayError;
    use crate::gateway::admin::{Distribution, Redistribution};
    use crate::query::model::EndpointLine;
    use crate::views::load::server::{Asked, Stop, stopped};
    use crate::views::model::{Refusal, ViewError};

    /// The value of the replica field that names every replica.
    pub const EVERY_REPLICA: &str = "*";

    /// What one replica did with an action: its report, or why it gave none.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Acted<T> {
        /// The replica ran the action and reported what it did.
        Done(T),
        /// The replica refused the action, with its status and stable code.
        Refused(Refusal),
        /// The console could not reach the replica or read its answer.
        Fault(ViewError),
    }

    /// One replica's answer to an action.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ReplicaAnswer<T> {
        /// The replica's name.
        pub replica: String,
        /// What it did.
        pub acted: Acted<T>,
    }

    /// What a replica reports of a clear.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ClearLine {
        /// The `ehr_id` cleared, only when it is a bare UUID.
        pub ehr_id: Option<String>,
        /// What the replica held before.
        pub held: HeldBefore,
        /// Every member the index or a binding named, by `node_id`.
        pub claimants: Vec<String>,
        /// Whether the `ehr_id` index held an entry.
        pub indexed: bool,
        /// How many client sessions held a binding.
        pub sessions: u64,
    }

    /// What a replica reports of a drift repair (§12.7).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RepairLine {
        /// The status the replica answered with.
        pub status: u16,
        /// Whether every member took the version (§11.4, N37).
        pub complete: bool,
        /// The qualified name of the version sent.
        pub name: String,
        /// The version sent.
        pub version: String,
        /// What the registry did with it.
        pub registry: Option<String>,
        /// Each member's outcome.
        pub endpoints: Vec<EndpointLine>,
    }

    /// The `ehr_id` a clear names and the replicas it goes to.
    #[derive(Debug)]
    pub struct ClearPlan<'a> {
        /// The `ehr_id` to clear.
        pub ehr_id: String,
        /// The replicas to clear it at, in the order configured.
        pub replicas: Vec<&'a ReplicaSettings>,
    }

    /// A refusal of the form, naming the field and never its value.
    fn invalid(reason: &str) -> Refusal {
        Refusal::Invalid {
            reason: reason.to_owned(),
        }
    }

    /// The text of `field`, trimmed, or `None` when it is empty.
    fn given(field: &str) -> Option<&str> {
        let text = field.trim();
        (!text.is_empty()).then_some(text)
    }

    /// Refuses an action on a console configured with no admin listener,
    /// and one the operator did not confirm.
    fn offered(form: &AdminForm, replicas: &[ReplicaSettings], what: &str) -> Result<(), Refusal> {
        if replicas.is_empty() {
            return Err(invalid(
                "This console is configured with no admin listener, so it offers no admin action.",
            ));
        }
        if given(&form.confirm).is_none() {
            return Err(Refusal::Invalid {
                reason: format!("Tick the confirmation to run the {what}."),
            });
        }
        Ok(())
    }

    /// The clear `form` asks for among `replicas`.
    ///
    /// # Errors
    /// Returns [`Refusal::Invalid`] for a console with no admin listener, a
    /// form not confirmed, naming no `ehr_id`, or naming a replica the
    /// console does not know.
    pub fn clear<'a>(
        form: &AdminForm,
        replicas: &'a [ReplicaSettings],
    ) -> Result<ClearPlan<'a>, Refusal> {
        offered(form, replicas, "clear")?;
        let ehr_id = given(&form.ehr_id).ok_or_else(|| invalid("Name the ehr_id to clear."))?;
        let chosen = match given(&form.replica) {
            Some(EVERY_REPLICA) => replicas.iter().collect(),
            Some(name) => replicas
                .iter()
                .filter(|replica| replica.name == name)
                .collect(),
            None => Vec::new(),
        };
        if chosen.is_empty() {
            return Err(invalid("Choose a replica this console names."));
        }
        Ok(ClearPlan {
            ehr_id: ehr_id.to_owned(),
            replicas: chosen,
        })
    }

    /// The drift repair `form` asks for and the replica it goes to, the
    /// first of `replicas`: the stored-query store is the replicas' shared
    /// one, so one replica repairs it for all.
    ///
    /// # Errors
    /// Returns [`Refusal::Invalid`] for a console with no admin listener, a
    /// form not confirmed, naming no stored query, version or member, or a
    /// member list that cannot be sent as a header.
    pub fn repair<'a>(
        form: &AdminForm,
        replicas: &'a [ReplicaSettings],
    ) -> Result<(&'a ReplicaSettings, Redistribution), Refusal> {
        offered(form, replicas, "drift repair")?;
        let replica = replicas.first().ok_or_else(|| {
            invalid(
                "This console is configured with no admin listener, so it offers no admin action.",
            )
        })?;
        let name = given(&form.name).ok_or_else(|| invalid("Name the stored query to send."))?;
        let version = given(&form.version).ok_or_else(|| invalid("Name the version to send."))?;
        let endpoints = given(&form.endpoints).ok_or_else(|| {
            invalid("Name the members to send it to, or * for every active member.")
        })?;
        HeaderValue::from_str(endpoints)
            .map_err(|_illegal| invalid("The members cannot be sent as a header."))?;
        Ok((
            replica,
            Redistribution {
                name: name.to_owned(),
                version: version.to_owned(),
                endpoints: endpoints.to_owned(),
            },
        ))
    }

    /// What `replica` did with the action `what`, `answered` read by `read`,
    /// logged by replica, status and code, never by what the action named.
    pub fn acted<A, T>(
        replica: &str,
        what: &str,
        answered: Result<A, GatewayError>,
        read: fn(A) -> T,
    ) -> ReplicaAnswer<T> {
        let acted = match answered {
            Ok(report) => {
                tracing::info!(replica, action = what, "an operator's admin action ran");
                Acted::Done(read(report))
            }
            Err(error) => match stopped(&error, Asked::Action { what, replica }) {
                Stop::Refused(refusal) => Acted::Refused(refusal),
                Stop::Fault(fault) => Acted::Fault(fault),
            },
        };
        ReplicaAnswer {
            replica: replica.to_owned(),
            acted,
        }
    }

    /// What the console renders of `cleared`.
    #[must_use]
    pub fn cleared(cleared: ClearedEhrId) -> ClearLine {
        ClearLine {
            ehr_id: cleared.ehr_id,
            held: cleared.held,
            claimants: cleared.claimants,
            indexed: cleared.indexed,
            sessions: u64::try_from(cleared.sessions).unwrap_or(u64::MAX),
        }
    }

    /// What the console renders of `distribution`.
    #[must_use]
    pub fn repaired(distribution: Distribution) -> RepairLine {
        let federation = &distribution.federation;
        RepairLine {
            status: distribution.status.as_u16(),
            complete: federation.complete(),
            endpoints: federation
                .endpoints()
                .iter()
                .map(|endpoint| EndpointLine {
                    id: endpoint.id().as_str().to_owned(),
                    status: endpoint.status().as_str().to_owned(),
                    latency_ms: endpoint.outcome().latency_ms(),
                    row_count: endpoint.row_count(),
                    organisation: endpoint.organisation().map(str::to_owned),
                    error: endpoint
                        .outcome()
                        .error()
                        .map(crate::query::run::server::error_text),
                })
                .collect(),
            name: distribution.definition.name,
            version: distribution.definition.version,
            registry: distribution.registry,
        }
    }
}
