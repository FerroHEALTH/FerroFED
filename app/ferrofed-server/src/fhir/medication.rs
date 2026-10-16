// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routes of electronic prescriptions and dispensations on the FHIR
//! face (Regulation (EU) 2025/327 Art 14(1)(b) and (c)).
//!
//! `GET {fhir-base}/MedicationRequest` searches a patient's prescriptions,
//! `GET {fhir-base}/MedicationRequest/{id}` reads one, and
//! `POST {fhir-base}/MedicationDispense` receives a dispensation, each at
//! the members `[[fhir.serve]]` declares for its category. A category no
//! member is declared for is not served: every route of it answers `404`
//! with an `OperationOutcome` of the issue type `not-supported` saying so,
//! and asks no member, so the answer is never an empty `searchset` a client
//! could read as a patient with no prescription. The status and the issue
//! type are our own design: no specification governs them.
//!
//! The search names each prescription by the id the read takes
//! ([`super::locator`]), which names the member that holds it, so a read
//! goes to that member alone.

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use axum::Extension;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::Response;
use eehrxf::category::Category;
use eehrxf::dataset::DatasetError;
use eehrxf::medication::Mpd;
use ferrofed_eehrxf::dispensations::{self, Unread};
use ferrofed_eehrxf::prescriptions::SearchRequest;
use ferrofed_eehrxf::prescriptions::page::{Kept, Page};
use ferrofed_eehrxf::prescriptions::searchset::{Answered, Assembled, Silent, assemble};
use ferrofed_eehrxf::summary::Organisation;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::id::EndpointId;
use fhir_types::codec::{Primitive, Value};
use http::{HeaderValue, Method, StatusCode, Uri, header};
use jiff::fmt::rfc2822::DateTimePrinter;
use openehr_federation::status::EndpointStatus;
use openehr_sdt::smart_scopes::{Permission, ResourceFamily};

use crate::auth::caller::Caller;
use crate::auth::permission::{Requirement, Resource};
use crate::config::fhir::FhirSettings;
use crate::error::Code;
use crate::facade::native::Chosen;
use crate::facade::prescription::{self, Unfound};
use crate::facade::prescriptions::{self, Held, Prescribed, Unprescribed};
use crate::facade::provenance::Provenance;
use crate::facade::summary::organisation;
use crate::federation::Federation;
use crate::state::AppState;

use super::locator::{self, Locator};
use super::{admission, attach, fhir_json, outcome, refusal};

/// The path of the prescription search under `{fhir-base}`.
pub const PRESCRIPTIONS: &str = "/MedicationRequest";

/// The path of the read of one prescription under `{fhir-base}`.
pub const PRESCRIPTION: &str = "/MedicationRequest/{id}";

/// The path a dispensation is received at under `{fhir-base}`.
pub const DISPENSATIONS: &str = "/MedicationDispense";

/// What a path of the prescriptions and dispensations addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addressed {
    /// `{fhir-base}/MedicationRequest`, exactly.
    Prescriptions,
    /// `{fhir-base}/MedicationRequest/{id}`, its id one FHIR R4 `id`.
    Prescription,
    /// `{fhir-base}/MedicationDispense`, exactly.
    Dispensations,
}

/// What `path`, the raw request path, addresses among the prescriptions and
/// dispensations of the face `fhir` serves, or `None` for any other path.
#[must_use]
pub fn addressed(fhir: &FhirSettings, path: &str) -> Option<Addressed> {
    if path == fhir.base.join(PRESCRIPTIONS) {
        return Some(Addressed::Prescriptions);
    }
    if path == fhir.base.join(DISPENSATIONS) {
        return Some(Addressed::Dispensations);
    }
    let prefix = fhir.base.join(PRESCRIPTIONS);
    let id = path.strip_prefix(prefix.as_str())?.strip_prefix('/')?;
    // NOTE: FHIR R4 Datatypes `id`, `[A-Za-z0-9\-\.]{1,64}`; any other segment names no
    // resource, so the gate refuses it as a variant of the path.
    locator::is_fhir_id(id).then_some(Addressed::Prescription)
}

/// Returns what a request of `method` to `addressed` requires at the gate.
///
/// A search of prescriptions is a `GET` that takes the `composition-`
/// search over every template, a read of one a `GET` that takes the
/// `composition-` read over every template, and the receipt of a
/// dispensation a `POST` that takes the `composition-` create over every
/// template; every other method is refused.
#[must_use]
pub fn requirement(addressed: Addressed, method: &Method) -> Requirement {
    match addressed {
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // prescription is EHR content its member holds, so only a search of every template's covers it.
        Addressed::Prescriptions if *method == Method::GET => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Search,
            resource: Resource::Unnamed,
        },
        // NOTE: no specification governs the scope of the FHIR face: our own design; the read
        // names no template before the member answers, so only a read of every template's covers it.
        Addressed::Prescription if *method == Method::GET => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Read,
            resource: Resource::Unnamed,
        },
        // NOTE: no specification governs the scope of the FHIR face: our own design; a
        // dispensation is EHR content its member writes, as a received document is, so it takes
        // the `composition-` create of every template.
        Addressed::Dispensations if *method == Method::POST => Requirement::Scope {
            family: ResourceFamily::Composition,
            permission: Permission::Create,
            resource: Resource::Unnamed,
        },
        Addressed::Prescriptions | Addressed::Prescription | Addressed::Dispensations => {
            Requirement::Refused
        }
    }
}

/// `GET {fhir-base}/MedicationRequest?patient.identifier=system|value`: a
/// patient's prescriptions, at every member that serves them
/// ([`crate::facade::prescriptions`]).
///
/// A patient no member serving prescriptions holds, and one none may say
/// anything about, both answer the same empty `searchset` (FHIR R4 search
/// §3.1.1.3: an unknown subject is no failure; Federation Tier §11.3).
pub(super) async fn search(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    uri: Uri,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    let read = match SearchRequest::from_query(uri.query()) {
        Ok(read) => read,
        Err(refused) => {
            return refusal(StatusCode::BAD_REQUEST, "invalid", &refused.to_string());
        }
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to search prescriptions",
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
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let none = BTreeSet::new();
    let declared = fhir
        .serving
        .members_of(Category::Prescription)
        .unwrap_or(&none);
    let patient = (read.system(), read.value());
    let searched = prescriptions::search(federation, &arrived, patient, declared).await;
    let (request_id, outbound) = (admitted.request_id, admitted.outbound);
    match searched {
        Ok(prescribed) => listed(federation, fhir, (&read, &prescribed)),
        Err(Unprescribed::NotFound(prescribed)) => {
            let mut response = listed_none(fhir, &read);
            let accessed = prescribed.and_then(|prescribed| {
                prescriptions::accessed(federation, &prescribed, patient, None)
            });
            attach(&mut response, accessed);
            response
        }
        Err(Unprescribed::Incomplete {
            status,
            silent,
            prescribed,
        }) => {
            let named = silent
                .iter()
                .map(|(endpoint, status)| format!("{endpoint} ({status})"))
                .collect::<Vec<_>>()
                .join(", ");
            let code = if status == StatusCode::GATEWAY_TIMEOUT {
                "timeout"
            } else {
                "incomplete"
            };
            let diagnostics = if named.is_empty() {
                "the search is incomplete: the patient could not be resolved at every member (§11.3)"
                    .to_owned()
            } else {
                format!("the search is incomplete: no answer from {named} (§11.3)")
            };
            let mut response = outcome(status, code, &diagnostics);
            attach(
                &mut response,
                prescriptions::accessed(federation, &prescribed, patient, None),
            );
            response
        }
        Err(Unprescribed::Failure(failure)) => failure.respond(request_id, outbound),
        Err(Unprescribed::Internal(what)) => {
            tracing::error!(request_id = %outbound, what, "the prescription search could not be run");
            crate::error::fixed(Code::Internal, request_id)
        }
    }
}

/// The page of the `searchset` `read` asks for of what `prescribed` holds,
/// on the face `fhir`, with the access it records.
fn listed(
    federation: &Federation,
    fhir: &FhirSettings,
    (read, prescribed): (&SearchRequest, &Prescribed),
) -> Response {
    let snapshot = federation.snapshot();
    let found: Vec<(&prescriptions::Member, Organisation, Vec<Page>)> = prescribed
        .members
        .iter()
        .filter_map(|member| match &member.held {
            Held::Found(pages) => Some((
                member,
                organisation(snapshot, &member.chosen.node),
                named(fhir, &member.chosen, pages),
            )),
            Held::Unusable { .. } | Held::Silent(_) => None,
        })
        .collect();
    let answered: Vec<Answered<'_>> = found
        .iter()
        .map(|(member, organisation, pages)| Answered {
            endpoint: member.chosen.native.as_str(),
            organisation,
            pages,
        })
        .collect();
    let silent: Vec<Silent> = prescribed
        .members
        .iter()
        .filter(|member| !matches!(member.held, Held::Found(_)))
        .map(|member| Silent {
            endpoint: member.chosen.native.as_str().to_owned(),
            status: member.held.status().as_str().to_owned(),
        })
        .chain(prescribed.unresolved.iter().map(|endpoint| Silent {
            endpoint: endpoint.as_str().to_owned(),
            status: EndpointStatus::NotResolved.as_str().to_owned(),
        }))
        .collect();
    let assembled = written(fhir, read, (&answered, &silent));
    let mut response = fhir_json(StatusCode::OK, &assembled.searchset);
    let accessed = prescriptions::accessed(
        federation,
        prescribed,
        (read.system(), read.value()),
        Some(&assembled),
    );
    attach(&mut response, accessed);
    response
}

/// The pages `pages` of the member `chosen`, each prescription named by the
/// id the face reads it by ([`locator`]), on the face `fhir`.
///
/// A prescription named so has that id and a `fullUrl` of the face's read
/// (FHIR R4 `http.html` §3.1.0.2), and is otherwise as the member sent it;
/// one whose parts give no such id keeps the member's own id and `fullUrl`
/// and is marked unreadable, which the searchset warns of.
fn named(fhir: &FhirSettings, chosen: &Chosen, pages: &[Page]) -> Vec<Page> {
    let base = fhir.absolute.trim_end_matches('/');
    let mut pages = pages.to_vec();
    for prescription in pages
        .iter_mut()
        .flat_map(|page| page.prescriptions.iter_mut())
    {
        let kept = &mut prescription.kept;
        let id = kept.id().and_then(|resource| {
            Locator::new(chosen.native.clone(), chosen.ehr_id.clone(), resource).id()
        });
        let Some(id) = id else {
            // NOTE: no specification governs this: our own design; a prescription whose parts
            // give no id is listed as its member states it, and the searchset warns it cannot be read.
            tracing::warn!(
                endpoint = chosen.native.as_str(),
                "a member's prescription has no id the face can read it by"
            );
            prescription.unreadable = true;
            continue;
        };
        kept.full_url = Some(format!("{base}{PRESCRIPTIONS}/{id}"));
        kept.rename(&id);
    }
    pages
}

/// The empty `searchset` `read` is answered with when no member serving
/// prescriptions holds the patient, or none may say so.
fn listed_none(fhir: &FhirSettings, read: &SearchRequest) -> Response {
    fhir_json(StatusCode::OK, &written(fhir, read, (&[], &[])).searchset)
}

/// The page `read` asks for of what `answered` holds, naming every member
/// of `silent`, on the face `fhir`.
fn written(
    fhir: &FhirSettings,
    read: &SearchRequest,
    (answered, silent): (&[Answered<'_>], &[Silent]),
) -> Assembled {
    let recorded = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let mut mint = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    assemble(
        (read, fhir.absolute.trim_end_matches('/')),
        (answered, silent),
        &recorded,
        &mut mint,
    )
}

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
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    Path(id): Path<String>,
    headers: http::HeaderMap,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Prescription) {
        return not_served(Category::Prescription);
    }
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to read prescriptions",
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
    let declared = admitted
        .fhir
        .serving
        .members_of(Category::Prescription)
        .unwrap_or(&none);
    let read = prescription::read(federation, &arrived, &id, declared).await;
    answered(
        federation,
        read,
        (&id, admitted.request_id, admitted.outbound),
    )
}

/// The answer to the read of the prescription `id` that settled as `read`,
/// to the request of the exchange id `request_id` and the gateway's id
/// `outbound`, with the access it records.
fn answered(
    federation: &Federation,
    read: Result<prescription::Read, Unfound>,
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
            tracing::info!(request_id = %outbound, "a prescription id has no owner");
            let mut response = outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "no prescription is known by this id",
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
                "the prescription could not be read: no answer from {endpoint} ({}) (§11.3)",
                EndpointStatus::Offline.as_str()
            ),
        ),
        Err(Unfound::Several(read)) => {
            let mut response = outcome(
                StatusCode::CONFLICT,
                "conflict",
                &format!(
                    "{} answered more than one prescription for this id, and the gateway never chooses between them (FHIR R4 search §3.1.1.4.1)",
                    read.chosen.native
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
                    "the prescription could not be read: no answer from {} ({}) (§11.3)",
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
    let Some(kept) = read.prescription() else {
        return crate::error::fixed(Code::Internal, request_id);
    };
    let mut kept = kept.clone();
    // NOTE: FHIR R4 `http.html` §3.1.0.2: the returned resource's id is the [id] read, the
    // one change the face makes to the member's prescription.
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
        "a member's prescription has a versionId the read cannot give as its ETag"
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
        "a member's prescription has a lastUpdated the read cannot give as its Last-Modified"
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

/// `POST {fhir-base}/MedicationDispense`: a dispensation, written to the
/// one member that owns the prescription it names
/// ([`crate::facade::dispensation`]).
///
/// The body is one `MedicationDispense` in FHIR JSON (FHIR R4 `http.html`
/// §3.1.0.8), held to the HL7 Europe MPD 1.0.0 profiles before any member
/// is asked: a text that is no `MedicationDispense` is `400`, and one that
/// breaks its profiles `422` with one issue per finding. A written
/// dispensation answers `201` with a `Location` naming it by the face's id.
pub(super) async fn dispense(
    State(state): State<Arc<AppState>>,
    outbound: Option<Extension<OutboundId>>,
    caller: Option<Extension<Caller>>,
    headers: http::HeaderMap,
    body: Bytes,
) -> Response {
    let started = Instant::now();
    if !serves(&state, Category::Dispensation) {
        return not_served(Category::Dispensation);
    }
    if !super::fhir_json_body(&headers) {
        return refusal(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not-supported",
            "a dispensation is sent as application/fhir+json",
        );
    }
    let Ok(text) = std::str::from_utf8(&body) else {
        return refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "the dispensation is not UTF-8 text (RFC 8259 §8.1)",
        );
    };
    let mpd = match &*MPD {
        Ok(mpd) => mpd,
        Err(failure) => {
            tracing::error!(error = %crate::chain(failure), "the HL7 Europe MPD package this build carries could not be read");
            return refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the dispensation profiles could not be read",
            );
        }
    };
    let received = match dispensations::read(text, mpd) {
        Ok(received) => received,
        Err(unread) => return unread_answer(&unread),
    };
    let admitted = admission::admit(
        &state,
        (outbound, caller.as_deref()),
        (&headers, started),
        || {
            refusal(
                StatusCode::FORBIDDEN,
                "forbidden",
                "the caller was not admitted to write a dispensation",
            )
        },
    );
    let admitted = match admitted {
        Ok(admitted) => admitted,
        Err(refused) => return refused,
    };
    let targeted = match targeted(&admitted.federation, &headers) {
        Ok(targeted) => targeted,
        Err(refused) => return refused,
    };
    let confined = match admitted.confine().await {
        Ok(confined) => confined,
        Err(refused) => return refused,
    };
    let arrived = admitted.arrived(&confined);
    crate::facade::dispensation::dispense(
        &admitted.federation,
        admitted.fhir,
        &arrived,
        (&received, &body),
        &targeted,
    )
    .await
}

/// The HL7 Europe MPD 1.0.0 profiles a received dispensation is held to,
/// read from the package this build carries.
static MPD: LazyLock<Result<Mpd, DatasetError>> = LazyLock::new(|| {
    Mpd::read(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/specs/eu-hl7-mpd/hl7.fhir.eu.mpd-1.0.0.tgz"
    )))
});

/// The endpoints the `openEHR-federation-endpoint` header names, each one
/// the registry of `federation` holds (§8.4).
///
/// # Errors
///
/// The `400` answer of a header that is no list of endpoint ids, or that
/// names one the registry does not hold, with nothing sent.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
fn targeted(
    federation: &Federation,
    headers: &http::HeaderMap,
) -> Result<BTreeSet<EndpointId>, Response> {
    let malformed = || {
        refusal(
            StatusCode::BAD_REQUEST,
            "invalid",
            "openEHR-federation-endpoint names no endpoint of the registry (§8.4)",
        )
    };
    let snapshot = federation.snapshot();
    let mut named = BTreeSet::new();
    for value in headers.get_all(openehr_federation::headers::ENDPOINT) {
        let value = value.to_str().map_err(|_unread| malformed())?;
        for id in value.split(',').map(str::trim) {
            let id = EndpointId::new(id).map_err(|_unparsed| malformed())?;
            if snapshot.endpoint(&id).is_none() && snapshot.native_endpoint(&id).is_none() {
                return Err(malformed());
            }
            named.insert(id);
        }
    }
    Ok(named)
}

/// The answer to a dispensation refused before any member is asked.
fn unread_answer(unread: &Unread) -> Response {
    match unread {
        Unread::NonConformant { findings } => {
            let mut response = fhir_json(
                StatusCode::UNPROCESSABLE_ENTITY,
                &ferrofed_eehrxf::receive::nonconformant(findings),
            );
            response.extensions_mut().insert(crate::access::NoAccess);
            response
        }
        Unread::Check(_) => {
            tracing::error!(error = %crate::chain(unread), "a dispensation's profiles could not be checked");
            refusal(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the dispensation profiles could not be checked",
            )
        }
        _ if unread.is_form() => refusal(StatusCode::BAD_REQUEST, "invalid", &crate::chain(unread)),
        _ => refusal(
            StatusCode::UNPROCESSABLE_ENTITY,
            "processing",
            &crate::chain(unread),
        ),
    }
}

/// Whether a member is declared for `category`.
fn serves(state: &AppState, category: Category) -> bool {
    state
        .fhir()
        .is_some_and(|fhir| fhir.serving.serves(category))
}

/// The answer to a route of `category` when no member serves it, asking no
/// member.
fn not_served(category: Category) -> Response {
    let what = if category == Category::Dispensation {
        "electronic dispensations"
    } else {
        "electronic prescriptions"
    };
    refusal(
        StatusCode::NOT_FOUND,
        "not-supported",
        &format!(
            "{what} ({}) are not served by this deployment",
            category.code()
        ),
    )
}
