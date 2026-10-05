// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A `patient/` grant confined to its patient across the federation: the
//! token's `ehrId` resolved to each member's own `ehr_id`, and every request
//! held to those pairs (§5.2, §12.5).
//!
//! The caller's issuer is bound to one member, whose platform issued the
//! token; its `ehrId` is that member's `ehr_id` for the patient of the
//! launch context (SMART on openEHR, master07 §Context Selection). The
//! gateway reads it as an identifier in the member's `ehr_id` system and
//! resolves it through the cross-reference at every member (§5.2), to the
//! set of `{node, ehr_id}` pairs of that patient. A request under the grant
//! is admitted only when every pair it would dispatch or route to is in the
//! set; any other patient, a query that names no patient, and an area that
//! holds no EHR are refused `403` with nothing sent ([`refused`]). A bare
//! `ehr_id` is never compared across members, because one `ehr_id` can name
//! another patient's EHR at another node (§12.5, §12.5.2).
//!
//! Each node is told, in the conveyance, the patient's own `ehr_id` there,
//! so it can enforce the grant too (N26, N33), and a node outside the set is
//! signed no conveyance, so no request reaches it. No specification defines
//! a patient-confined grant across nodes, so this is FerroFED's own design.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use axum::response::Response;
use ferrofed_engine::conveyance::{Confinement, Conveyance};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::patient::{PatientRef, PatientRefError};
use ferrofed_identity::role::resolver::{Resolution, ResolverError};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use secrecy::SecretString;

use crate::auth::caller::Caller;
use crate::error::{self, Code};
use crate::facade::owner;
use crate::facade::security::TARGET;
use crate::federation::Federation;

/// Why the patient of a confined grant could not be resolved, so the grant
/// cannot be confined and nothing is sent.
///
/// None of them names the `ehrId` or any other value of the token.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum Unconfined {
    /// The request's overall deadline cannot be represented.
    #[error("the request's deadline cannot be represented")]
    Clock,
    /// The member the issuer is bound to has left the registry.
    #[error(
        "the member endpoint {0} the token's issuer is bound to is no endpoint of the registry"
    )]
    Departed(EndpointId),
    /// No cross-reference resolver is configured.
    #[error("no cross-reference resolver is configured to resolve the token's patient (§5.2)")]
    NoResolver,
    /// The token's `ehrId` does not form a patient reference.
    #[error("the token's ehrId does not form a patient reference (§5.2)")]
    Patient(#[source] PatientRefError),
    /// The cross-reference could not answer for a member.
    #[error("the cross-reference could not resolve the token's patient at member {member}")]
    Unavailable {
        /// The member it could not answer for.
        member: NodeId,
        /// What the resolver reported.
        #[source]
        source: ResolverError,
    },
    /// The cross-reference gave no answer for a member it was asked about.
    #[error("the cross-reference gave no answer for the token's patient at member {0}")]
    Unanswered(NodeId),
    /// The cross-reference places the token's patient under another `ehr_id`
    /// at the member that issued the token.
    #[error(
        "the cross-reference places the token's patient under another ehr_id at member {0}, which issued the token"
    )]
    Contradicted(NodeId),
}

impl Unconfined {
    /// The answer to the request: `424`, logged under the gateway's `logged`
    /// id with the cause.
    pub(crate) fn respond(&self, request_id: &str, logged: &str) -> Response {
        crate::metrics::security::Event::PatientContextUnavailable.record();
        tracing::error!(
            target: TARGET,
            event = "patient-context-unavailable",
            error = crate::chain(self),
            request_id = logged,
            "the patient of a patient/ grant could not be resolved, so nothing was sent"
        );
        error::response(
            Code::PatientContextUnavailable,
            self.to_string(),
            request_id,
        )
    }
}

/// The confinement of `caller`'s grant, resolved on the caller's behalf
/// before `deadline`, or `None` for a caller whose grant is not confined to
/// a patient.
///
/// The `ehrId` is resolved at every member, the bound member included, and
/// that member keeps the `ehrId` itself: the token names the patient's EHR
/// there (master07 §Context Selection).
///
/// # Errors
///
/// Returns an [`Unconfined`] when the bound member left the registry, no
/// resolver is configured, or the cross-reference could not answer for a
/// member or contradicts the token at the bound member.
pub(crate) async fn confinement(
    federation: &Federation,
    caller: Option<&Caller>,
    deadline: Instant,
) -> Result<Option<Confinement>, Unconfined> {
    let Some((caller, context)) =
        caller.and_then(|caller| caller.patient().map(|context| (caller, context)))
    else {
        return Ok(None);
    };
    let snapshot = federation.snapshot();
    let bound = snapshot
        .endpoint(context.endpoint())
        .ok_or_else(|| Unconfined::Departed(context.endpoint().clone()))?
        .node()
        .clone();
    let resolver = federation.resolver().ok_or(Unconfined::NoResolver)?;
    // NOTE: §5.2, §12.5: the ehrId is meaningful only at the member that issued it, so it
    // is resolved as an identifier in that member's ehr_id system, never matched bare.
    let patient = PatientRef::new(
        context.ehr_id_system().clone(),
        SecretString::from(context.ehr_id().as_str().to_owned()),
    )
    .map_err(Unconfined::Patient)?;
    let members: Vec<NodeId> = snapshot.nodes().map(|node| node.id().clone()).collect();
    let mut resolutions = resolver
        .resolve(&patient, &members, &caller.on_behalf(), deadline)
        .await;
    let mut held: BTreeMap<NodeId, EhrId> = BTreeMap::new();
    for member in members {
        match resolutions.remove(&member) {
            Some(Resolution::Resolved(ehr_id)) => {
                if member == bound && ehr_id != *context.ehr_id() {
                    return Err(Unconfined::Contradicted(member));
                }
                held.insert(member, ehr_id);
            }
            Some(Resolution::Unknown) => {}
            Some(Resolution::Unavailable(source)) => {
                return Err(Unconfined::Unavailable { member, source });
            }
            None => return Err(Unconfined::Unanswered(member)),
        }
    }
    held.insert(bound, context.ehr_id().clone());
    let at = snapshot
        .endpoints()
        .filter_map(|endpoint| {
            let ehr_id = held.get(endpoint.node())?;
            Some((endpoint.id().clone(), ehr_id.clone()))
        })
        .collect();
    Ok(Some(Confinement::new(context.endpoint().clone(), at)))
}

/// Whether `patient`, the subject a confined request names, is the patient
/// `confinement` names: resolved at the bound member alone, on behalf of
/// `on_behalf`, before `deadline`, it must be the token's own `ehr_id` there
/// (§5.2).
///
/// Nothing else is asked first, no localizer, no consent pre-filter and no
/// other member, so a confined caller who names another patient makes the
/// gateway learn nothing of that patient anywhere else.
///
/// # Errors
///
/// Returns an [`Unconfined`] when the bound member left the registry, no
/// resolver is configured, or the cross-reference could not answer for it.
pub(crate) async fn names_own(
    federation: &Federation,
    confinement: &Confinement,
    patient: &PatientRef,
    on_behalf: &OnBehalfOf,
    deadline: Instant,
) -> Result<bool, Unconfined> {
    let bound = federation
        .snapshot()
        .endpoint(confinement.bound())
        .ok_or_else(|| Unconfined::Departed(confinement.bound().clone()))?
        .node()
        .clone();
    let cross_reference = federation.resolver().ok_or(Unconfined::NoResolver)?;
    let mut answers = cross_reference
        .resolve(patient, std::slice::from_ref(&bound), on_behalf, deadline)
        .await;
    match answers.remove(&bound) {
        Some(Resolution::Resolved(ehr_id)) => {
            Ok(confinement.ehr_id_at(confinement.bound()) == Some(&ehr_id))
        }
        Some(Resolution::Unknown) => Ok(false),
        Some(Resolution::Unavailable(source)) => Err(Unconfined::Unavailable {
            member: bound,
            source,
        }),
        None => Err(Unconfined::Unanswered(bound)),
    }
}

/// Whether `conveyance` admits a request to `ehr_id` at the node `endpoint`
/// reaches: always for a caller whose grant is not confined, and for a
/// confined one only when the pair is its patient's.
pub(crate) fn admits(conveyance: &Conveyance, endpoint: &EndpointId, ehr_id: &EhrId) -> bool {
    conveyance
        .confinement()
        .is_none_or(|confinement| confinement.admits(endpoint, ehr_id))
}

/// The members at which the patient `confinement` names holds `ehr_id`, in
/// `node_id` order.
pub(crate) fn holders(
    snapshot: &RegistrySnapshot,
    confinement: &Confinement,
    ehr_id: &EhrId,
) -> Vec<NodeId> {
    confinement
        .holding(ehr_id)
        .filter_map(|endpoint| snapshot.endpoint(endpoint))
        .map(|endpoint| endpoint.node().clone())
        .collect::<BTreeSet<NodeId>>()
        .into_iter()
        .collect()
}

/// Whether `conveyance` admits sending a request for `ehr_id` where
/// `located` places it: the one endpoint it names must hold the confined
/// patient's own `ehr_id`, and a member it cannot name is beyond the grant.
///
/// A collision is admitted here, because it is refused `409` with nothing
/// sent to any claimant (§12.5.2, N42).
pub(crate) fn admits_located(
    conveyance: &Conveyance,
    located: &owner::Located<'_>,
    ehr_id: &EhrId,
) -> bool {
    match located {
        owner::Located::At { endpoint, .. } => admits(conveyance, endpoint.id(), ehr_id),
        owner::Located::Collision(_) => true,
        owner::Located::Unreachable { .. } | owner::Located::Unknown => !is_confined(conveyance),
    }
}

/// Whether `conveyance` carries a caller whose grant is confined to one
/// patient.
pub(crate) fn is_confined(conveyance: &Conveyance) -> bool {
    conveyance.confinement().is_some()
}

/// The `403` refusing a request that reaches beyond the confined patient,
/// for `why`, as a security event under the gateway's `logged` id; it names
/// no `ehr_id` and no endpoint.
pub(crate) fn refused(why: &'static str, request_id: &str, logged: &str) -> Response {
    stopped(why, logged);
    error::fixed(Code::PatientConfinement, request_id)
}

/// Logs that a request under a `patient/` grant reached beyond its patient,
/// for `why`, under the gateway's `logged` id; it names no `ehr_id` and no
/// endpoint.
pub(crate) fn stopped(why: &'static str, logged: &str) {
    crate::metrics::security::Event::PatientConfinement.record();
    tracing::warn!(
        target: TARGET,
        event = "patient-confinement",
        why,
        request_id = logged,
        "a request under a patient/ grant reached beyond its patient and was not sent"
    );
}
