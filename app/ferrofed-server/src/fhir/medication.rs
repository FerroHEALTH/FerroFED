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
use std::sync::Arc;
use std::time::Instant;

use axum::Extension;
use axum::extract::{Path, State};
use axum::response::Response;
use eehrxf::category::Category;
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
/// dispensation a `POST` by a verified caller; every other method is
/// refused.
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
        // TODO(#1042): require the scope a dispensation is written under.
        Addressed::Dispensations if *method == Method::POST => Requirement::Caller,
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

/// `POST {fhir-base}/MedicationDispense`: a dispensation.
pub(super) async fn dispense(State(state): State<Arc<AppState>>) -> Response {
    if !serves(&state, Category::Dispensation) {
        return not_served(Category::Dispensation);
    }
    // TODO(#1042): write the dispensation to the member that owns its prescription.
    unavailable("the receipt of a dispensation")
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

/// The answer to a route of a served category whose interaction this
/// release does not carry, asking no member.
fn unavailable(interaction: &str) -> Response {
    refusal(
        StatusCode::NOT_IMPLEMENTED,
        "not-supported",
        &format!("{interaction} is not available in this release"),
    )
}
