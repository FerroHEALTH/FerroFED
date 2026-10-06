// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The planning of a federated query: its targets, the plan shaped for the
//! request's modes, and the resolution the targets came from, remembered
//! (§8, §10, §11.4, §12.5.1).

use std::time::Instant;

use ferrofed_engine::fanout::{Completion, Plan};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::consent::Requester;
use ferrofed_identity::session::SessionKey;
use openehr_federation::aql::Analysis;
use openehr_federation::aql::subject::Subject;
use openehr_federation::attribute::EndpointAttribute;
use openehr_federation::dedup::DedupMode;
use openehr_federation::outcome::ErrorDetail;

use super::Failure;
use crate::facade::{owner, plan};
use crate::federation::Federation;

/// Drops the `session`'s bindings that name a member the consent pre-filter
/// denied (N27a), holds the `{node, ehr_id}` set a resolution produced as the
/// `session`'s resolution bindings (§12.5.1 step 2), teaches the `ehr_id` index where
/// each `ehr_id` is held (step 3), and records the state the resolution
/// showed of the resolver.
pub(in crate::facade) fn remember(
    federation: &Federation,
    session: Option<&SessionKey>,
    targets: &plan::Targets,
) {
    let resolved = &targets.resolved;
    if let Some(session) = session {
        // NOTE: N27a; a consent denial drops every `ehr_id` the session cached for a denied
        // member before the new bindings are held (no specification governs this: our own design).
        federation
            .bindings()
            .forget_denied(session, &targets.denied);
        federation.bindings().record(
            session,
            Instant::now(),
            resolved.iter().map(|(node, ehr_id)| (node, ehr_id)),
        );
    }
    for (node, ehr_id) in resolved {
        owner::learn(federation.index(), ehr_id, node);
    }
    if let Some(observed) = targets.resolver {
        federation.dependencies().resolver(observed);
    }
}

/// `plan` shaped for `analysis` under the request's completion strategy,
/// dedup mode and ENDPOINT attributes, withholding consent unless the
/// request is served `disclosed` ([`Federation::discloses_consent_to`]).
pub(in crate::facade) fn planned(
    disclosed: bool,
    plan: Plan,
    analysis: &Analysis,
    (completion, dedup, attributes): (Completion, DedupMode, Vec<EndpointAttribute>),
) -> Plan {
    let mut plan = plan
        .completing(completion)
        .ordered(analysis.order().clone())
        .deduplicating(dedup)
        .annotating(attributes);
    if let Some(recombination) = analysis.recombination() {
        plan = plan.recombining(recombination.clone());
    }
    if !disclosed {
        plan = plan.withholding_consent(ErrorDetail::Text(String::from(plan::UNAVAILABLE)));
    }
    plan
}

/// The targets of `analysis` within `selection`, with the subject a patient
/// query names: a patient query is localized, consent-checked and resolved
/// for `requester` on behalf of `on_behalf` before `deadline`
/// ([`plan::patient`]), and any other query is planned as it stands.
///
/// # Errors
///
/// Returns [`Failure::Plan`] when the targets cannot be planned.
pub(in crate::facade) async fn targeted<'q>(
    federation: &Federation,
    (analysis, selection): (&'q Analysis, plan::Selection<'_>),
    (requester, on_behalf): (Option<&Requester>, &OnBehalfOf),
    (deadline, disclosed): (Instant, bool),
) -> Result<(plan::Targets, Option<&'q Subject>), Failure> {
    Ok(match analysis {
        Analysis::Patient(query) => (
            plan::patient(
                federation,
                selection,
                (query, requester, on_behalf),
                (deadline, disclosed),
            )
            .await
            .map_err(Failure::Plan)?,
            Some(query.subject()),
        ),
        Analysis::Unscoped(query) => (
            plan::unscoped(federation.snapshot(), selection, query).map_err(Failure::Plan)?,
            None,
        ),
    })
}
