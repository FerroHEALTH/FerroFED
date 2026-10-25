// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {fhir-base}/Patient?identifier=system|value`: the patient lookup of
//! the FHIR face (Regulation (EU) 2025/327 Annex II 2.1; Xt-EHR D5.1 §3.4.6
//! `api-producer-patient`).
//!
//! The caller is admitted and confined as for a summary, and the identity
//! binding's demographics service is asked for the patient first. The
//! patient is then located over the members as any subject is: localized,
//! checked against the consent pre-filter and resolved at the gateway (§5.2),
//! and no member is asked anything, so no patient identifier travels to a
//! node (§5.4.1, N33). When the patient resolved at a member, the answer is a
//! `searchset` with the one `Patient` the demographics service holds; an
//! identifier no member holds and a patient no member may disclose answer
//! alike, with the empty `searchset` (§11.3). Under `partial`, a member the
//! resolver could not answer for is named in an `outcome` entry (§11.4). A
//! lookup that finds the patient is recorded, naming the members the patient
//! resolved at (Annex II 3.2).

use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::State;
use axum::response::Response;
use ferrofed_eehrxf::lookup::{self, LookupRequest};
use ferrofed_eehrxf::summary::header_patient;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_identity::role::header::PatientHeader;
use http::{StatusCode, Uri};

use super::{
    Headless, admission, attach, fhir_json, header_of, outcome, refused, unheaded_outcome,
};
use crate::access::NoAccess;
use crate::auth::caller::Caller;
use crate::facade::demographics::Unheaded;
use crate::facade::summary::{self as gathered, Gathered, Scope, Unsummarised};
use crate::federation::Federation;
use crate::state::AppState;

/// `GET {fhir-base}/Patient?identifier=system|value`.
pub(super) async fn patients(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    let read = match LookupRequest::from_query(uri.query()) {
        Ok(read) => read,
        Err(refusal) => {
            let mut response = outcome(StatusCode::BAD_REQUEST, "invalid", &refusal.to_string());
            response.extensions_mut().insert(NoAccess);
            return response;
        }
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        unadmitted,
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    let federation = &admitted.federation;
    let (request_id, outbound, logged) = (admitted.request_id, admitted.outbound, &admitted.logged);
    let patient = (read.system(), read.value());
    let header = match header_of(federation, &arrived, patient, (request_id, logged)).await {
        Ok(header) => header,
        // NOTE: Xt-EHR D5.1 §3.4.6 asks for a lookup that identifies the patient, which a
        // Patient with no name still does; only a summary needs the name its profile requires.
        Err(Headless::Unheaded(Unheaded::Unnamed(header))) => *header,
        Err(headless) => return unfound(headless),
    };
    match gathered::gather(Arc::clone(federation), arrived, patient, Scope::Lookup).await {
        Ok(gathered) => found(federation, &gathered, (patient, &header)),
        // NOTE: FHIR R4 search, Federation Tier §11.3, Regulation (EU) 2025/327 Art 8: a patient
        // no member holds matches nothing, and one no member may disclose answers alike.
        Err(unsummarised) if unsummarised.reaches_no_member() => empty(),
        Err(Unsummarised::NotFound(gathered)) => {
            let mut response =
                fhir_json(StatusCode::OK, &lookup::searchset(Vec::new(), Vec::new()));
            let accessed = gathered.and_then(|gathered| {
                gathered::accessed(federation, &gathered, patient, (Scope::Lookup, None))
            });
            attach(&mut response, accessed);
            response
        }
        Err(unsummarised) => refused(
            federation,
            (unsummarised, Scope::Lookup),
            patient,
            (request_id, outbound),
        ),
    }
}

/// The `OperationOutcome` of a caller the gate did not admit to the lookup.
fn unadmitted() -> Response {
    outcome(
        StatusCode::FORBIDDEN,
        "forbidden",
        "the caller was not admitted to read the patient's EHRs",
    )
}

/// The empty `searchset`, with the statement that no member was sent
/// anything.
fn empty() -> Response {
    let mut response = fhir_json(StatusCode::OK, &lookup::searchset(Vec::new(), Vec::new()));
    response.extensions_mut().insert(NoAccess);
    response
}

/// The answer to a lookup whose patient the identity binding gave no
/// header for, `headless`.
fn unfound(headless: Headless) -> Response {
    let mut response = match headless {
        Headless::Refused(refused) => *refused,
        // NOTE: Federation Tier §11.3; a patient the binding does not know answers as one no
        // member holds, the empty searchset.
        Headless::Unheaded(Unheaded::NoMatch) => return empty(),
        // The lookup answers a patient with no name before it gets here, so the summary's
        // refusal of one is never the lookup's answer.
        Headless::Unheaded(unheaded) => unheaded_outcome(unheaded, "Patient"),
    };
    response.extensions_mut().insert(NoAccess);
    response
}

/// The `searchset` `gathered` gives for the patient `system`|`value` the
/// identity binding describes in `header`: the `Patient` when the patient
/// resolved at a member, and an `outcome` entry for each member the resolver
/// could not answer for, with the access it records.
fn found(
    federation: &Federation,
    gathered: &Gathered,
    ((system, value), header): ((&str, &str), &PatientHeader),
) -> Response {
    let minted = || uuid::Uuid::new_v4().to_string();
    let mut matches = Vec::new();
    // NOTE: §11.3: the patient is a match only where it resolved at a member, so a lookup whose
    // every member was unresolved names none.
    if !gathered.resolved.is_empty() {
        let id = minted();
        matches.push((
            format!("urn:uuid:{id}"),
            header_patient(&id, (system, value), header),
        ));
    }
    // NOTE: Federation Tier §11.3, §11.4: under best-effort, a member the resolver could not
    // answer for is named, so the lookup never reads as complete.
    let outcomes = gathered
        .unresolved
        .iter()
        .map(|endpoint| {
            (
                format!("urn:uuid:{}", minted()),
                lookup::unresolved(endpoint.as_str()),
            )
        })
        .collect();
    let mut response = fhir_json(StatusCode::OK, &lookup::searchset(matches, outcomes));
    attach(
        &mut response,
        gathered::accessed(federation, gathered, (system, value), (Scope::Lookup, None)),
    );
    response
}
