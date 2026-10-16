// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read of one prescription or one dispensation on the FHIR face, by
//! the id the face names it by.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use eehrxf::category::Category;
use ferrofed_eehrxf::prescriptions::page::Kept;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use fhir_types::codec::{Primitive, Value};
use http::{HeaderValue, StatusCode, header};
use jiff::fmt::rfc2822::DateTimePrinter;
use openehr_federation::status::EndpointStatus;

use crate::auth::caller::Caller;
use crate::error::Code;
use crate::facade::prescription::{self, Readable, Unfound};
use crate::facade::provenance::Provenance;
use crate::federation::Federation;
use crate::state::AppState;

use crate::fhir::locator;
use crate::fhir::{admission, attach, fhir_json, outcome, refusal};

use super::{not_served, serves};

/// `GET {fhir-base}/MedicationRequest/{id}`: one prescription, read at the
/// member its id names ([`crate::facade::prescription`]).
///
/// The prescription is answered as the member sent it, its `id` the one the
/// request named (FHIR R4 `http.html` §3.1.0.2: "The returned resource SHALL
/// have an id element with a value that is the \[id\]"), and the answer names
/// the member in `openEHR-federation-endpoint` and
/// `openEHR-federation-system-id` (§7a.3, N31). An id with no owner answers
/// `404` with one text whatever the reason, two members claiming its
/// `ehr_id` `409` with the integrity incident of N42, and a member that
/// does not answer `504` or `424` naming it (§11.1, §11.3).
pub(in crate::fhir) async fn read(
    state: State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    id: Path<String>,
    headers: http::HeaderMap,
) -> Response {
    read_of(
        Readable::Prescription,
        state,
        (outbound, caller),
        id,
        headers,
    )
    .await
}

/// `GET {fhir-base}/MedicationDispense/{id}`: one dispensation, read at the
/// member its id names, as a prescription is read ([`read`]).
///
/// The id is the one the `Location` of the dispensation's receipt named
/// (FHIR R4 `http.html` §3.1.0.8), so the face can read what it wrote.
pub(in crate::fhir) async fn read_dispensation(
    state: State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    id: Path<String>,
    headers: http::HeaderMap,
) -> Response {
    read_of(
        Readable::Dispensation,
        state,
        (outbound, caller),
        id,
        headers,
    )
    .await
}

/// The read of `readable` by the id `id`, for the request of `headers`.
async fn read_of(
    readable: Readable,
    State(state): State<Arc<AppState>>,
    (outbound, caller): (Option<Extension<OutboundId>>, Option<Extension<Caller>>),
    Path(id): Path<String>,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    let category = served(readable);
    if !serves(&state, category) {
        return not_served(category);
    }
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                &format!("the caller was not admitted to read {}s", noun(readable)),
            )
        },
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
    let none = BTreeSet::new();
    let declared = admitted.fhir.serving.members_of(category).unwrap_or(&none);
    let read = prescription::read(federation, &arrived, (readable, &id), declared).await;
    answered(
        federation,
        (readable, read),
        (&id, admitted.request_id, admitted.outbound),
    )
}

/// The answer to the read of the prescription `id` that settled as `read`,
/// to the request of the exchange id `request_id` and the gateway's id
/// `outbound`, with the access it records.
fn answered(
    federation: &Federation,
    (readable, read): (Readable, Result<prescription::Read, Unfound>),
    (id, request_id, outbound): (&str, &str, OutboundId),
) -> Response {
    let named = |endpoints: &[EndpointId]| {
        endpoints
            .iter()
            .map(EndpointId::as_str)
            .collect::<Vec<_>>()
            .join(", ")
    };
    match read {
        Ok(read) => found(federation, &read, id, request_id),
        Err(Unfound::NoOwner(read)) => {
            tracing::info!(request_id = %outbound, "a prescription or dispensation id has no owner");
            let mut response = outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                &format!("no {} is known by this id", noun(readable)),
            );
            attach(
                &mut response,
                read.and_then(|read| prescription::accessed(federation, &read)),
            );
            response
        }
        Err(Unfound::Ambiguous(endpoints)) => refusal(
            StatusCode::CONFLICT,
            "conflict",
            &format!(
                "the id names the member endpoints {}, and the gateway never chooses between them",
                named(&endpoints)
            ),
        ),
        Err(Unfound::Claimed(claimants)) => refusal(
            StatusCode::CONFLICT,
            "conflict",
            &format!(
                "the ehr_id the id names is claimed by the member endpoints {}, and the gateway never chooses between them (§12.5.2, N42)",
                named(&claimants)
            ),
        ),
        Err(Unfound::Confined) => crate::error::fixed(Code::PatientConfinement, request_id),
        Err(Unfound::Offline(endpoint)) => refusal(
            StatusCode::GATEWAY_TIMEOUT,
            "timeout",
            &format!(
                "the {} could not be read: no answer from {endpoint} ({}) (§11.3)",
                noun(readable),
                EndpointStatus::Offline.as_str()
            ),
        ),
        Err(Unfound::Several(read)) => {
            let mut response = outcome(
                StatusCode::CONFLICT,
                "conflict",
                &format!(
                    "{} answered more than one {} for this id, and the gateway never chooses between them (FHIR R4 search §3.1.1.4.1)",
                    read.chosen.native,
                    noun(readable)
                ),
            );
            attach(&mut response, prescription::accessed(federation, &read));
            response
        }
        Err(Unfound::Unanswered(read)) => {
            let status = read.held.status();
            let (code, answer) = match status {
                EndpointStatus::Offline | EndpointStatus::TimeOut => {
                    ("timeout", StatusCode::GATEWAY_TIMEOUT)
                }
                _ => ("incomplete", StatusCode::FAILED_DEPENDENCY),
            };
            let mut response = outcome(
                answer,
                code,
                &format!(
                    "the {} could not be read: no answer from {} ({}) (§11.3)",
                    noun(readable),
                    read.chosen.native,
                    status.as_str()
                ),
            );
            attach(&mut response, prescription::accessed(federation, &read));
            response
        }
        Err(Unfound::Failure(failure)) => failure.respond(request_id, outbound),
    }
}

/// The answer of the one prescription `read` found, under the id `id` the
/// request named, naming its member, with the access it records.
fn found(
    federation: &Federation,
    read: &prescription::Read,
    id: &str,
    request_id: &str,
) -> Response {
    let Some(kept) = read.resource() else {
        return crate::error::fixed(Code::Internal, request_id);
    };
    let mut kept = kept.clone();
    // NOTE: FHIR R4 `http.html` §3.1.0.2: the returned resource's id is the [id] read, the
    // one change the face makes to the member's resource.
    kept.rename(id);
    let resource = kept.unknown();
    let snapshot = federation.snapshot();
    let mut response = Provenance::member(snapshot, &read.chosen.native, &read.chosen.node)
        .stamp(fhir_json(StatusCode::OK, &resource));
    if let Some(tag) = version_tag(&kept, request_id) {
        response.headers_mut().insert(header::ETAG, tag);
    }
    if let Some(date) = modified(&kept, request_id) {
        response.headers_mut().insert(header::LAST_MODIFIED, date);
    }
    attach(&mut response, prescription::accessed(federation, read));
    response
}

/// The weak `ETag` of the member's version of `kept`, `W/"<versionId>"`, when
/// its `meta.versionId` is a FHIR `id`.
///
/// A `versionId` that is no FHIR `id` gives none, and is logged by its shape
/// alone, never by its value.
fn version_tag(kept: &Kept, request_id: &str) -> Option<HeaderValue> {
    // NOTE: FHIR R4 `http.html` §3.1.0.2, §3.1.0.1.3 and §3.1.0.5: a read SHOULD carry its
    // versionId as a weak `ETag`, and a versionId is valid as the `id` data type is.
    let version = kept.resource.get("meta")?.get("versionId")?;
    let shape = match version.as_str() {
        Some(text) if locator::is_fhir_id(text) => {
            match HeaderValue::try_from(format!("W/\"{text}\"")) {
                Ok(tag) => return Some(tag),
                Err(_unfit) => "no header value",
            }
        }
        Some(_) => "no FHIR id",
        None => "no string",
    };
    tracing::warn!(
        request_id,
        shape,
        "a member's resource has a versionId the read cannot give as its ETag"
    );
    None
}

/// The `Last-Modified` of the member's version of `kept`, its
/// `meta.lastUpdated` as an HTTP-date, when that is a FHIR `instant`.
///
/// A `lastUpdated` that is no FHIR `instant` gives none, and is logged by
/// its shape alone, never by its value.
fn modified(kept: &Kept, request_id: &str) -> Option<HeaderValue> {
    // NOTE: FHIR R4 `http.html` §3.1.0.2 and §3.1.0.1.3: a read SHOULD carry meta.lastUpdated
    // as `Last-Modified`, an RFC 9110 §5.6.7 HTTP-date.
    let updated = kept.resource.get("meta")?.get("lastUpdated")?;
    let shape = match http_date(updated) {
        Ok(date) => return Some(date),
        Err(shape) => shape,
    };
    tracing::warn!(
        request_id,
        shape,
        "a member's resource has a lastUpdated the read cannot give as its Last-Modified"
    );
    None
}

/// The HTTP-date in GMT of the FHIR `instant` `updated`, or the shape that
/// keeps it from being one.
fn http_date(updated: &Value) -> Result<HeaderValue, &'static str> {
    // NOTE: FHIR R4 datatypes `instant`: a value outside its lexical form is no instant, and
    // the decode error is that answer, so it is dropped for its shape.
    let instant = fhir_types::r4::primitives::Instant::from_json_parts(
        Some(updated),
        None,
        &mut fhir_types::codec::Path::root("lastUpdated"),
    )
    .map_err(|_outside| "no FHIR instant")?;
    let text = instant.value.ok_or("no FHIR instant")?;
    let at: jiff::Timestamp = text.parse().map_err(|_unparsed| "no timestamp")?;
    let date = DateTimePrinter::new()
        .timestamp_to_rfc9110_string(&at)
        .map_err(|_unprinted| "no HTTP-date")?;
    HeaderValue::try_from(date).map_err(|_unfit| "no header value")
}

/// What `readable` is called in an answer.
fn noun(readable: Readable) -> &'static str {
    match readable {
        Readable::Prescription => "prescription",
        Readable::Dispensation => "dispensation",
    }
}

/// The category whose members serve `readable`.
fn served(readable: Readable) -> Category {
    match readable {
        Readable::Prescription => Category::Prescription,
        Readable::Dispensation => Category::Dispensation,
    }
}
