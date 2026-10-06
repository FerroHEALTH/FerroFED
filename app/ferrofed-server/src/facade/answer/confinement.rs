// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient confinement of a federated query: a caller whose grant is
//! confined to one patient reaches that patient's `{node, ehr_id}` pairs and
//! nothing else (§5.2, §12.5).

use std::time::Instant;

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_registry::id::EhrId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aql::Analysis;

use super::Failure;
use crate::facade::{confined, plan};
use crate::federation::Federation;

/// Refuses, when the caller in `conveyance` is confined to one patient, the
/// `targets` of `analysis` that reach beyond that patient ([`within`]).
///
/// # Errors
///
/// Returns [`Failure::Confined`] for such targets, with nothing sent.
pub(in crate::facade) fn held_within(
    federation: &Federation,
    conveyance: &Conveyance,
    (analysis, targets): (&Analysis, &plan::Targets),
    request_id: &str,
) -> Result<(), Failure> {
    if confined::is_confined(conveyance)
        && !within(federation.snapshot(), conveyance, analysis, targets)
    {
        confined::stopped("patient", request_id);
        return Err(Failure::Confined);
    }
    Ok(())
}

/// Refuses, when the caller in `conveyance` is confined to one patient, a
/// query whose node queries read beyond the one `EHR` each is scoped to, and
/// a query that names another patient than the confined one.
///
/// A query beyond the one `EHR` names neither a patient nor an `ehr_id`, or
/// has a class beside that `EHR`, a second `EHR`, or an `EHR` under
/// `NOT CONTAINS`. A named patient is resolved at the bound member alone,
/// on behalf of `on_behalf`, before `deadline` ([`confined::names_own`]), so
/// no localizer, consent pre-filter or other member learns of another
/// patient.
///
/// # Errors
///
/// Returns [`Failure::Confined`] for such a query, before any lookup and
/// with nothing sent, and [`Failure::Unconfirmed`] when the named patient
/// cannot be checked at the bound member.
pub(in crate::facade) async fn confine(
    federation: &Federation,
    (conveyance, on_behalf): (&Conveyance, &OnBehalfOf),
    analysis: &Analysis,
    (deadline, request_id): (Instant, &str),
) -> Result<(), Failure> {
    let Some(confinement) = conveyance.confinement() else {
        return Ok(());
    };
    // NOTE: master08 §Resource Scopes, §7.1: a patient grant reaches "data within that patient's
    // EHR", so every class the query reads must be contained under the one scoped EHR.
    if !analysis.within_one_ehr() {
        confined::stopped("population", request_id);
        return Err(Failure::Confined);
    }
    if let Analysis::Patient(query) = analysis {
        let patient = plan::patient_ref(query.subject()).map_err(Failure::Plan)?;
        let own = confined::names_own(federation, confinement, &patient, on_behalf, deadline)
            .await
            .map_err(Failure::Unconfirmed)?;
        if !own {
            confined::stopped("patient", request_id);
            return Err(Failure::Confined);
        }
    }
    Ok(())
}

/// Whether every `{node, ehr_id}` pair `targets` would dispatch to is one of
/// the confined patient's in `conveyance`, and there is at least one (§5.2,
/// §12.5).
///
/// A patient query goes to each member under the `ehr_id` its patient
/// resolved to there, and a query scoped to one `ehr_id` goes under that
/// `ehr_id`. A plan that dispatches nothing is refused too, so a confined
/// caller never learns whether another patient is known anywhere.
fn within(
    snapshot: &RegistrySnapshot,
    conveyance: &Conveyance,
    analysis: &Analysis,
    targets: &plan::Targets,
) -> bool {
    let scope = match analysis {
        // NOTE: §12.5, an ehr_id that is no HIER_OBJECT_ID names no EHR, so it is no pair
        // of the confined patient.
        Analysis::Unscoped(query) => query.ehr_scope().and_then(|value| EhrId::new(value).ok()),
        Analysis::Patient(_) => None,
    };
    let mut dispatched = targets.plan.dispatched().peekable();
    dispatched.peek().is_some()
        && dispatched.all(|endpoint| {
            let ehr_id = match analysis {
                Analysis::Patient(_) => snapshot.endpoint(endpoint).and_then(|declared| {
                    targets
                        .resolved
                        .iter()
                        .find(|(node, _)| node == declared.node())
                        .map(|(_, ehr_id)| ehr_id)
                }),
                Analysis::Unscoped(_) => scope.as_ref(),
            };
            ehr_id.is_some_and(|ehr_id| confined::admits(conveyance, endpoint, ehr_id))
        })
}
