// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `GET {fhir-base}/DocumentReference`: the document list of the FHIR face
//! (Regulation (EU) 2025/327 Annex II 2.1).
//!
//! The patient is resolved as for a summary, and every member that serves
//! the patient summary itself is asked for it at its own endpoint, by its
//! own `ehr_id` alone (§5.4.1, N33), with the summary's budget, onward
//! credential and caller token; no section query is run, since the summary
//! the gateway writes is listed and never written here. The answer is a
//! `searchset` with the gateway's summary, retrievable through
//! `Patient/$summary`, and each member's document at `Binary/{token}`, a URL
//! the gateway signs for the caller ([`super::binary`]). A
//! member that does not answer, or answers with no FHIR document `Bundle`
//! whose identifier has a system and a value (FHIR R4 `bdl-9`), is a member
//! in `node-error` (§11.1): it fails the list under all-or-nothing, and
//! under `partial` is named in an `outcome` entry (§11.3, §11.4).

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use axum::Extension;
use axum::extract::State;
use axum::response::Response;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ferrofed_eehrxf::list::{self, Fetched, ListRequest, Listing};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::forward::Forwarded;
use ferrofed_engine::single_node::native::Answer;
use http::{StatusCode, Uri, header};
use openehr_federation::status::EndpointStatus;

use super::{admission, attach, fhir_json, outcome, refused, unadmitted};
use crate::access::NoAccess;
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::facade::native::Native;
use crate::facade::summary::{self as gathered, Gathered, Scope, Unsummarised};
use crate::federation::Federation;
use crate::state::AppState;

/// The media type of a member's document that names none (RFC 9110 §8.3).
const OCTET_STREAM: &str = "application/octet-stream";

/// `GET {fhir-base}/DocumentReference?patient.identifier=system|value`.
pub(super) async fn documents(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    let read = match ListRequest::from_query(uri.query()) {
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
    // NOTE: FHIR R4 search; a `type` that admits no patient summary matches nothing the face
    // lists, so the empty set is answered and nothing is resolved or sent.
    if !read.admits_summaries() {
        let mut response = fhir_json(StatusCode::OK, &list::searchset(Vec::new(), Vec::new()));
        response.extensions_mut().insert(NoAccess);
        return response;
    }
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let (request_id, outbound, logged) = (admitted.request_id, admitted.outbound, &admitted.logged);
    let patient = (read.system(), read.value());
    match gathered::gather(Arc::clone(federation), arrived, patient, Scope::List).await {
        Ok(gathered) => listed(
            federation,
            (fhir, admitted.caller),
            &gathered,
            patient,
            logged,
        ),
        // NOTE: FHIR R4 search, Federation Tier §11.3: a patient no member holds matches no
        // document, and answers as one no member may disclose does.
        Err(unsummarised) if unsummarised.reaches_no_member() => {
            let mut response = fhir_json(StatusCode::OK, &list::searchset(Vec::new(), Vec::new()));
            response.extensions_mut().insert(NoAccess);
            response
        }
        Err(Unsummarised::NotFound(gathered)) => {
            let mut response = fhir_json(StatusCode::OK, &list::searchset(Vec::new(), Vec::new()));
            let accessed = gathered.and_then(|gathered| {
                gathered::accessed(federation, &gathered, patient, (Scope::List, None))
            });
            attach(&mut response, accessed);
            response
        }
        Err(unsummarised) => refused(
            federation,
            (unsummarised, Scope::List),
            patient,
            (request_id, outbound),
        ),
    }
}

/// The `searchset` `gathered` gives for the patient `system`|`value` on the
/// face `fhir`: the summary its operator has the gateway write, each
/// member's own document at a URL signed for `caller`, and an `outcome` entry for each member that did not
/// answer, with the access it records.
fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
    (system, value): (&str, &str),
    logged: &str,
) -> Response {
    let date = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let listing = Listing {
        system,
        value,
        date: &date,
    };
    let minted = || uuid::Uuid::new_v4().to_string();
    let (id, unique) = (minted(), format!("urn:uuid:{}", minted()));
    let mut matches = vec![(
        format!("urn:uuid:{id}"),
        list::on_demand((&id, &unique), listing, &fhir.operator),
    )];
    let mut outcomes = Vec::new();
    for native in &gathered.natives {
        match &native.answer {
            Answer::Document(document) => {
                let forwarded = document.forwarded();
                let Some(fetched) = fetched(federation, (fhir, caller), native, forwarded) else {
                    tracing::error!(
                        request_id = logged,
                        endpoint = native.chosen.native.as_str(),
                        "a member's document could not be named by a signed URL"
                    );
                    let mut response = outcome(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "exception",
                        "a member's document could not be listed",
                    );
                    let accessed = gathered::accessed(
                        federation,
                        gathered,
                        (system, value),
                        (Scope::List, None),
                    );
                    attach(&mut response, accessed);
                    return response;
                };
                let custodian = gathered::organisation(federation.snapshot(), &native.chosen.node);
                let id = minted();
                matches.push((
                    format!("urn:uuid:{id}"),
                    list::native((&id, document.identifier()), listing, &custodian, fetched),
                ));
            }
            Answer::Absent => {}
            Answer::Unusable { .. } => outcomes.push((
                format!("urn:uuid:{}", minted()),
                list::unusable(native.chosen.native.as_str()),
            )),
            silent => outcomes.push((
                format!("urn:uuid:{}", minted()),
                list::silent(native.chosen.native.as_str(), silent.status().as_str()),
            )),
        }
    }
    // NOTE: Federation Tier §11.3, §11.4: under best-effort, a member the resolver could not
    // answer for is named as a silent one, so the list never reads as complete.
    for endpoint in &gathered.unresolved {
        outcomes.push((
            format!("urn:uuid:{}", minted()),
            list::silent(endpoint.as_str(), EndpointStatus::NotResolved.as_str()),
        ));
    }
    let mut response = fhir_json(StatusCode::OK, &list::searchset(matches, outcomes));
    attach(
        &mut response,
        gathered::accessed(
            federation,
            gathered,
            (system, value),
            (Scope::List, Some(&BTreeSet::new())),
        ),
    );
    response
}

/// The document `forwarded` carries, as `native`'s entry names it: its
/// media type, the URL signed for `caller` it is read at, the SHA-1 digest
/// of its bytes in base64, and their number, or `None` when they are too
/// many for an `unsignedInt` or no key can sign the URL.
fn fetched(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    native: &Native,
    forwarded: &Forwarded,
) -> Option<Fetched> {
    let bytes = forwarded.body();
    let size = u32::try_from(bytes.len()).ok()?;
    let signer = federation.signer()?;
    // NOTE: no specification governs this: our own design; a key that cannot sign leaves
    // the document unnamed, which fails the list with a 500.
    let token = super::binary::mint(signer, fhir, caller, &native.chosen).ok()?;
    // NOTE: RFC 9110 §8.3; a member that names no media type is read as sending octets.
    let content_type = forwarded
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or(OCTET_STREAM)
        .to_owned();
    // NOTE: FHIR R4 Attachment `hash`: the base64 of the SHA-1 of the data.
    let hash = digest(&SHA1_FOR_LEGACY_USE_ONLY, bytes);
    Some(Fetched {
        content_type,
        url: format!("Binary/{token}"),
        hash: STANDARD.encode(hash.as_ref()),
        size,
    })
}
