// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's federated query held to the patient its
//! treatment relationship confirmation binds (eHDSI XCA Profile 8.0.1
//! §4.2.1, §4.2.2).
//!
//! The query must name the TRC patient, or be scoped to one of that
//! patient's own `ehr_id`s, and every `{node, ehr_id}` pair it is dispatched
//! to must be one of that patient's ([`crate::facade::trc`]). A query for
//! another patient is refused before the demographics supplier, a localizer,
//! the consent pre-filter or any member is asked. A query for another
//! patient, one for a TRC patient no member holds, and one no member may be
//! sent, every holder restricting it, get the one answer
//! [`Failure::NoData`], so none tells the three apart. A query that names no
//! patient and no EHR is [`Failure::Unbound`]. A failure of a service the
//! plan depends on is never taken for no data.

use ferrofed_engine::conveyance::relayed::Relayed;
use ferrofed_registry::id::EhrId;
use openehr_federation::aql::Analysis;

use super::Failure;
use crate::facade::plan;
use crate::facade::trc::{self, Binding, Reached};
use crate::federation::Federation;

/// Holds `analysis`, before it is planned, to `binding`, the patient the
/// contact point relaying `relayed` is bound to, logging under `logged`;
/// a query with no binding passes.
///
/// # Errors
///
/// [`Failure::Unbound`] for a query that names no patient and is scoped to
/// no `ehr_id`, or reads beyond its one `EHR`, and [`Failure::NoData`] for
/// one that names another patient or a TRC patient no member holds; each
/// break of the binding is recorded.
pub(in crate::facade) fn analysed(
    (binding, relayed): (Option<&Binding>, Option<&Relayed>),
    analysis: &Analysis,
    logged: &str,
) -> Result<(), Failure> {
    let Some(binding) = binding else {
        return Ok(());
    };
    let scoped = match analysis {
        Analysis::Patient(_) => true,
        // NOTE: §12.5, N29: an ehr_id that is no HIER_OBJECT_ID names no EHR, so the query
        // is scoped to none of the TRC patient's.
        Analysis::Unscoped(query) => query
            .ehr_scope()
            .is_some_and(|value| EhrId::new(value).is_ok()),
    };
    if !scoped || !analysis.within_one_ehr() {
        trc::recorded(relayed, binding, Reached::Unbound("population"), logged);
        return Err(Failure::Unbound);
    }
    if let Analysis::Patient(query) = analysis {
        let subject = query.subject();
        let (system, value) = (subject.namespace(), subject.value());
        if !binding.names(system, value) {
            if !binding.patient().is(system, value) {
                trc::recorded(relayed, binding, Reached::Patient(system, value), logged);
            }
            return Err(Failure::NoData);
        }
    }
    Ok(())
}

/// Holds the plan of `analysis`, `targets`, to `binding`: every endpoint it
/// dispatches to must reach one of the TRC patient's own `{node, ehr_id}`
/// pairs, and a plan that dispatches to none, with every service it depends
/// on answering, is no data; a plan with no binding passes.
///
/// # Errors
///
/// [`Failure::NoData`] for a pair outside the binding, recorded as a break
/// of it, and for a plan no member is sent, recorded as nothing.
pub(in crate::facade) fn planned(
    (binding, relayed): (Option<&Binding>, Option<&Relayed>),
    federation: &Federation,
    (analysis, targets): (&Analysis, &plan::Targets),
    logged: &str,
) -> Result<(), Failure> {
    let Some(binding) = binding else {
        return Ok(());
    };
    let scope = match analysis {
        Analysis::Unscoped(query) => query.ehr_scope().and_then(|value| EhrId::new(value).ok()),
        Analysis::Patient(_) => None,
    };
    let mut dispatched = targets.plan.dispatched().peekable();
    if dispatched.peek().is_none() {
        // NOTE: §11.3, §14.1: a resolver or localizer that did not answer is an upstream
        // failure the answer reports, never a patient that is not there.
        if targets.resolution_failed || targets.unlocalized {
            return Ok(());
        }
        return Err(Failure::NoData);
    }
    for endpoint in dispatched {
        let Some(node) = federation
            .snapshot()
            .endpoint(endpoint)
            .map(ferrofed_registry::snapshot::Endpoint::node)
        else {
            return Err(Failure::NoData);
        };
        let ehr_id = match analysis {
            Analysis::Patient(_) => targets
                .resolved
                .iter()
                .find(|(resolved, _)| resolved == node)
                .map(|(_, ehr_id)| ehr_id),
            Analysis::Unscoped(_) => scope.as_ref(),
        };
        match ehr_id {
            Some(ehr_id) if binding.holds(node, ehr_id) => {}
            Some(ehr_id) => {
                trc::recorded(relayed, binding, Reached::Resource(node, ehr_id), logged);
                return Err(Failure::NoData);
            }
            None => return Err(Failure::NoData),
        }
    }
    Ok(())
}
