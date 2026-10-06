// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient behind each `ehr_id` an access reached when the request named
//! none, so a search of the log by the patient's identifier finds the access
//! (Regulation (EU) 2025/327 Art 9(1); IHE `RESTful` ATNA §3.81.4.1.2.2).
//!
//! A routed request addressed by `ehr_id`, and a query scoped to one, names
//! no patient. The identity binding is asked for the patient the member
//! holds under the `ehr_id`, in each namespace `[access_log]
//! patient_namespaces` names, on behalf of the verified caller and within
//! the per-node budget. Whatever it answers, the access is recorded: the
//! record names the patient it found, or says why it names none, beside the
//! `ehr_id`. The ask goes to the identity service alone, never to a node
//! (§5.4, N33), and no identifier reaches a log line. No specification
//! governs where the record looks the patient up: our own design.

use std::time::Instant;

use ehds_logging::record::{DataSubject, EhrAt, PatientIdentifier, PatientLookup};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_identity::role::resolver::Identification;
use ferrofed_registry::id::{EhrId, EndpointId};

use crate::federation::Federation;

/// Names the patient behind each `ehr_id` of `subject`, asked in
/// `namespaces` on behalf of `on_behalf`, when the request named none.
pub(crate) async fn name(
    federation: &Federation,
    namespaces: &[IdentifierNamespace],
    on_behalf: &OnBehalfOf,
    subject: &mut DataSubject,
) {
    let named = subject.patient.is_some();
    for ehr in &mut subject.ehrs {
        ehr.patient = if named {
            PatientLookup::RequestNamed
        } else {
            lookup(federation, namespaces, on_behalf, ehr).await
        };
    }
}

/// What the identity binding of `federation` says of the patient behind
/// `ehr`.
async fn lookup(
    federation: &Federation,
    namespaces: &[IdentifierNamespace],
    on_behalf: &OnBehalfOf,
    ehr: &EhrAt,
) -> PatientLookup {
    if namespaces.is_empty() {
        return PatientLookup::NotConfigured;
    }
    let Some(resolver) = federation.resolver() else {
        return PatientLookup::Unsupported;
    };
    let member = EndpointId::new(ehr.endpoint.as_str())
        .ok()
        .and_then(|endpoint| federation.snapshot().endpoint(&endpoint))
        .map(|endpoint| endpoint.node().clone());
    let (Some(member), Ok(ehr_id)) = (member, EhrId::new(ehr.ehr_id.as_str())) else {
        tracing::warn!(
            endpoint = ehr.endpoint,
            "the endpoint or the ehr_id of an access names no member's EHR, so the record names no patient"
        );
        return PatientLookup::Unavailable;
    };
    // NOTE: §11.5: one ask of one service is bounded as one node's request is, so
    // naming the patient never holds an answer longer than reaching a node may.
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .unwrap_or_else(Instant::now);
    match resolver
        .identify(&member, &ehr_id, namespaces, on_behalf, deadline)
        .await
    {
        Identification::Named(named) => PatientLookup::Found(
            named
                .iter()
                .map(|patient| PatientIdentifier {
                    namespace: patient.namespace().as_str().to_owned(),
                    value: patient.withheld(),
                })
                .collect(),
        ),
        Identification::Unknown => PatientLookup::NotFound,
        Identification::Unsupported => PatientLookup::Unsupported,
        Identification::Unavailable(error) => {
            tracing::warn!(
                error = %crate::chain(&error),
                endpoint = ehr.endpoint,
                "the identity service could not name the patient behind an ehr_id, so the record says so"
            );
            PatientLookup::Unavailable
        }
        _ => PatientLookup::Unavailable,
    }
}
