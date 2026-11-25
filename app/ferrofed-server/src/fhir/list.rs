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
//! under `partial` is named in an `outcome` entry (§11.3, §11.4). Laboratory
//! discharge and imaging reports are listed when the gateway maps them
//! ([`super::written`]),
//! and laboratory, discharge and imaging reports when a member serves them
//! itself ([`super::own`]).

use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::Instant;

use aws_lc_rs::digest::{SHA1_FOR_LEGACY_USE_ONLY, digest};
use axum::extract::Path;
use axum::response::Response;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ehds_logging::category::Category;
use ferrofed_eehrxf::list::{
    self, DISCHARGE_SUMMARY, Fetched, IMAGING_REPORT, ListRefusal, ListRequest, Listing, Match,
    Subject,
};
use ferrofed_eehrxf::report::Written;
use ferrofed_engine::single_node::forward::Forwarded;
use ferrofed_engine::single_node::native as engine;
use http::{StatusCode, Uri, header};
use openehr_federation::status::EndpointStatus;

use super::admission::Asked;
use super::face_id::Unresolved;
use super::patient_summary::refused;
use super::trc::Service;
use super::{UNADMITTED, attach, outcome};
use crate::access::NoAccess;
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::facade::native::{Answer, Native};
use crate::facade::summary::own::Wanted;
use crate::facade::summary::{self as gathered, Gathered, Scope, Unsummarised};
use crate::federation::Federation;
use crate::fhir_wire::fhir_json;
use crate::ids::FaceId;

/// The holder the summary written on request is named under, beside the
/// patient's id, when its id is derived.
const SUMMARY_HOLDER: &str = "summary written on request";

/// The media type of a member's document that names none (RFC 9110 §8.3).
const OCTET_STREAM: &str = "application/octet-stream";

/// `GET {fhir-base}/DocumentReference?patient.identifier=system|value`.
pub(super) async fn documents(asked: Asked, uri: Uri) -> Response {
    answered(asked, ListRequest::from_query(uri.query()), Mode::Search).await
}

/// `GET {fhir-base}/DocumentReference/{id}`: the entry the list names by
/// `id`, found by the list of the patient the id stands for.
pub(super) async fn document(asked: Asked, Path(id): Path<String>) -> Response {
    answered(asked, ListRequest::read(&id), Mode::Read).await
}

/// The answer to the document list `read` in the mode `mode`.
async fn answered(asked: Asked, read: Result<ListRequest, ListRefusal>, mode: Mode) -> Response {
    let lists = Lists::declared(&asked);
    super::searched(
        (&asked, Instant::now()),
        read,
        UNADMITTED,
        |read| {
            // NOTE: FHIR R4 search; a `type` that admits no document type the face lists matches
            // nothing, so the empty set is answered and nothing is resolved or sent.
            if read.admits_summaries() || lists.wanted(read).any() {
                ControlFlow::Continue(())
            } else {
                let mut response = nothing(mode);
                response.extensions_mut().insert(NoAccess);
                ControlFlow::Break(response)
            }
        },
        async |read, admitted, arrived| {
            let federation = &admitted.federation;
            let store = asked.state.store().map(AsRef::as_ref);
            let (system, value) = match named(store, admitted.fhir, (read.subject(), mode)).await {
                Ok(named) => named,
                Err(answer) => return answer,
            };
            let patient = (system.as_str(), value.as_str());
            let service = if read.admits_summaries() {
                Service::PatientSummary
            } else {
                Service::OriginalDocument
            };
            if let Err(refused) = super::trc::named(admitted, &arrived, patient, service) {
                return refused;
            }
            let wanted = lists.wanted(read);
            match gathered::gather(
                Arc::clone(federation),
                arrived,
                patient,
                (Scope::List, wanted),
            )
            .await
            {
                Ok(gathered) => found(store, admitted, (&gathered, read, mode), patient).await,
                // NOTE: FHIR R4 search, Federation Tier §11.3: a patient no member holds matches no
                // document, and answers as one no member may disclose does.
                Err(unsummarised) if unsummarised.reaches_no_member() => {
                    let mut response = nothing(mode);
                    response.extensions_mut().insert(NoAccess);
                    response
                }
                Err(Unsummarised::NotFound(gathered)) => {
                    let mut response = nothing(mode);
                    let accessed = gathered.and_then(|gathered| {
                        gathered::access::accessed(
                            federation,
                            &gathered,
                            patient,
                            (Scope::List, None),
                        )
                    });
                    attach(&mut response, accessed);
                    response
                }
                Err(unsummarised) => refused(
                    federation,
                    (unsummarised, Scope::List),
                    patient,
                    (admitted.request_id, admitted.outbound),
                ),
            }
        },
    )
    .await
}

/// The documents the list names beside the summaries, as the deployment
/// declares them.
#[derive(Debug, Clone, Copy)]
struct Lists {
    /// Whether it lists laboratory reports.
    laboratory: bool,
    /// Whether the gateway writes laboratory, discharge and imaging reports
    /// from the members' compositions.
    written: (bool, bool, bool),
    /// Whether it lists discharge reports.
    discharge: bool,
    /// Whether it lists imaging documents.
    imaging: bool,
}

impl Lists {
    /// The documents the deployment `asked` runs under lists.
    fn declared(asked: &Asked) -> Self {
        let fhir = asked.state.fhir();
        // NOTE: no specification governs this: our own design; a member declared native for the
        // category serves its reports itself, with or without the gateway's own mapping.
        let serves = |category: Category| {
            asked.state.federation().is_some_and(|federation| {
                federation
                    .snapshot()
                    .native_endpoints()
                    .any(|native| native.serves(category.code()))
            })
        };
        let laboratory = fhir.is_some_and(|fhir| fhir.laboratory.is_some());
        let discharge = fhir.is_some_and(|fhir| fhir.discharge.is_some());
        let imaging = fhir.is_some_and(|fhir| fhir.imaging.is_some());
        Self {
            laboratory: laboratory || serves(Category::MedicalTestResult),
            written: (laboratory, discharge, imaging),
            discharge: discharge || serves(Category::DischargeReport),
            imaging: imaging || serves(Category::MedicalImaging),
        }
    }

    /// The documents the search `read` asks the members for beside the
    /// summaries: of each kind the list names, the types it admits.
    fn wanted(self, read: &ListRequest) -> Wanted {
        let types = |listed: bool, codes: &[&'static str]| {
            if listed { codes.to_vec() } else { Vec::new() }
        };
        let laboratory = self.laboratory && read.admits_laboratory();
        let mut written = BTreeSet::new();
        if self.written.0 && laboratory {
            written.insert(Written::Laboratory);
        }
        // NOTE: no specification governs this: our own design; the gateway lists each report it
        // writes under one code of the category's value set alone.
        if self.written.1 && read.discharge().contains(&DISCHARGE_SUMMARY) {
            written.insert(Written::Discharge);
        }
        if self.written.2 && read.imaging().contains(&IMAGING_REPORT) {
            written.insert(Written::Imaging);
        }
        Wanted {
            laboratory,
            discharge: types(self.discharge, read.discharge()),
            imaging: types(self.imaging, read.imaging()),
            written,
        }
    }
}

/// The list `gathered` gives for `patient` on the request `admitted`, the
/// patient's id recorded in `store` first, since the list names it.
async fn found(
    store: Option<&crate::store::Store>,
    admitted: &super::admission::Admitted<'_>,
    (gathered, read, mode): (&Gathered, &ListRequest, Mode),
    (system, value): (&str, &str),
) -> Response {
    let id = admitted.fhir.ids.patient((system, value));
    let date = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let listing = Listing {
        system,
        value,
        patient: id.as_str(),
        date: &date,
    };
    let (response, mut ids) = listed(
        &admitted.federation,
        (admitted.fhir, admitted.caller),
        (gathered, read, mode),
        listing,
        &admitted.logged,
    );
    ids.push(id);
    super::face_id::record(store, admitted.fhir, (system, value), ids, &admitted.logged).await;
    response
}

/// What a list that names nothing answers: the empty `searchset`, or a
/// `404` for a read by id (FHIR R4 `http.html#read`).
fn nothing(mode: Mode) -> Response {
    match mode {
        Mode::Search => fhir_json(StatusCode::OK, &list::searchset(Vec::new(), Vec::new())),
        Mode::Read => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document the face lists has this id",
        ),
    }
}

/// What a list answers: the `searchset` of a search, or the one entry a
/// read by id names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// `GET {fhir-base}/DocumentReference?...`.
    Search,
    /// `GET {fhir-base}/DocumentReference/{id}`.
    Read,
}

/// The answer `gathered` gives for the patient of `listing` on the face
/// `fhir` in the mode `mode`, with the access it records, and the id of
/// every document it names.
fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    (gathered, read, mode): (&Gathered, &ListRequest, Mode),
    listing: Listing<'_>,
    logged: &str,
) -> (Response, Vec<FaceId>) {
    let (system, value) = (listing.system, listing.value);
    let (matches, outcomes) = match entries(
        federation,
        (fhir, caller),
        (gathered, read),
        listing,
        logged,
    ) {
        Ok(entries) => entries,
        Err(response) => return (response, Vec::new()),
    };
    let Ok(matches) = selected(matches, read).inspect_err(|error| {
        tracing::error!(request_id = logged, %error, "a listed document did not serialise");
    }) else {
        return (
            unlisted(federation, gathered, (system, value), "document"),
            Vec::new(),
        );
    };
    let ids = matches
        .iter()
        .filter_map(|(_, listed)| list::id(listed))
        .filter_map(FaceId::parse)
        .collect();
    let mut response = match (mode, matches.as_slice()) {
        (Mode::Search, _) => fhir_json(StatusCode::OK, &list::searchset(matches, outcomes)),
        (Mode::Read, [(_, listed)]) => match list::json(listed) {
            Ok(document) => fhir_json(StatusCode::OK, &document),
            Err(_) => {
                return (
                    unlisted(federation, gathered, (system, value), "document"),
                    ids,
                );
            }
        },
        // NOTE: FHIR R4 http.html#read: an id that names no document is a 404; one a member that
        // did not answer may hold says so, so the answer never reads as complete (§11.3).
        (Mode::Read, _) => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            if outcomes.is_empty() {
                "no document the face lists has this id"
            } else {
                "no document the face lists has this id among the members that answered"
            },
        ),
    };
    attach(
        &mut response,
        gathered::access::accessed(
            federation,
            gathered,
            (system, value),
            (Scope::List, Some(&BTreeSet::new())),
        ),
    );
    (response, ids)
}

/// The matches and the outcomes `gathered` gives for the patient of
/// `listing`: the summary its operator has the gateway write when the
/// patient resolved at a member, each member's own document at a URL signed
/// for `caller`, the written and the members' own reports, and an `outcome`
/// entry for each member that did not answer.
///
/// # Errors
///
/// The `500` of a document that could not be named by a signed URL.
#[expect(
    clippy::result_large_err,
    reason = "the answer is the handler's, returned at most once per request"
)]
fn entries(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    (gathered, read): (&Gathered, &ListRequest),
    listing: Listing<'_>,
    logged: &str,
) -> Result<super::written::Listed, Response> {
    let (system, value) = (listing.system, listing.value);
    let minted = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let located = |id: &FaceId| format!("{}/DocumentReference/{}", fhir.absolute, id.as_str());
    // NOTE: §7.1, §11.3: the gateway writes a summary only from a member the patient resolved
    // at, so a list naming every member unresolved lists none.
    let mut matches = Vec::new();
    if read.admits_summaries() && !gathered.resolved.is_empty() {
        let id = fhir.ids.document((SUMMARY_HOLDER, listing.patient));
        // NOTE: IHE ITI TF-1 §18.3.3.1: an On-Demand Document Entry keeps its uniqueId, and each
        // document written from it on retrieve takes a new one, as `Patient/$summary` does.
        matches.push((
            located(&id),
            Match::Written(Box::new(list::on_demand(
                (id.as_str(), &id.urn()),
                listing,
                &fhir.operator,
            ))),
        ));
    }
    let mut outcomes = Vec::new();
    let natives = gathered.natives.iter().filter(|_| read.admits_summaries());
    for native in natives {
        let endpoint = native.chosen.native.as_str();
        match &native.answer {
            Answer::Document(document) => {
                let forwarded = document.forwarded();
                let Some(fetched) = fetched(federation, (fhir, caller), native, forwarded) else {
                    tracing::error!(
                        request_id = logged,
                        endpoint,
                        "a member's document could not be named by a signed URL"
                    );
                    return Err(unlisted(federation, gathered, (system, value), "document"));
                };
                let custodian = gathered::organisation(federation.snapshot(), &native.chosen.node);
                let (held_system, held_value) = document.identifier();
                let id = fhir
                    .ids
                    .document((endpoint, &format!("{held_system}|{held_value}")));
                matches.push((
                    located(&id),
                    Match::Written(Box::new(list::native(
                        (id.as_str(), document.identifier()),
                        listing,
                        &custodian,
                        fetched,
                    ))),
                ));
            }
            Answer::Unread(engine::Answer::Absent) => {}
            Answer::Unusable { .. } => outcomes.push((minted(), list::unusable(endpoint))),
            silent @ Answer::Unread(_) => {
                outcomes.push((minted(), list::silent(endpoint, silent.status().as_str())));
            }
        }
    }
    let Some((reports, silent)) =
        super::written::listed(federation, (fhir, caller), gathered, listing)
    else {
        tracing::error!(
            request_id = logged,
            "a written report could not be named by a signed URL"
        );
        return Err(unlisted(federation, gathered, (system, value), "report"));
    };
    matches.extend(reports);
    outcomes.extend(silent);
    let Some((own, silent)) = super::own::listed(federation, (fhir, caller), gathered) else {
        tracing::error!(
            request_id = logged,
            "a member's own report could not be named by a signed URL"
        );
        return Err(unlisted(federation, gathered, (system, value), "report"));
    };
    matches.extend(own);
    outcomes.extend(silent);
    // NOTE: Federation Tier §11.3, §11.4: under best-effort, a member the resolver could not
    // answer for is named as a silent one, so the list never reads as complete.
    for endpoint in &gathered.unresolved {
        outcomes.push((
            minted(),
            list::silent(endpoint.as_str(), EndpointStatus::NotResolved.as_str()),
        ));
    }
    Ok((matches, outcomes))
}

/// The patient `subject` names: its identifier as sent, or the identifier
/// the store holds for the patient id a `patient` reference names.
///
/// # Errors
///
/// The answer when no patient is resolved: the empty `searchset` with an
/// `outcome` entry saying the id is unknown or has expired (FHIR R4 search
/// §3.1.1.7), a `400` when the gateway has no store, and a `500` when the
/// store does not answer or its record does not open.
#[expect(
    clippy::result_large_err,
    reason = "the answer is the handler's, returned at most once per request"
)]
async fn named(
    store: Option<&crate::store::Store>,
    fhir: &FhirSettings,
    (subject, mode): (&Subject, Mode),
) -> Result<(String, String), Response> {
    let absolute = format!("{}/Patient/", fhir.absolute);
    let id = match subject {
        Subject::Identifier { system, value } => return Ok((system.clone(), value.clone())),
        // NOTE: FHIR R4 search §3.1.1.4.11: a reference is `[id]`, `Patient/[id]` or the absolute
        // URL; an id of another server's or of another form names no patient the face answered.
        Subject::Patient(reference) => reference
            .strip_prefix(&absolute)
            .or_else(|| reference.strip_prefix("Patient/"))
            .unwrap_or(reference),
        Subject::Document(id) => id,
    };
    let resolved = match FaceId::parse(id) {
        Some(id) => super::face_id::resolved(store, fhir, &id).await,
        None => Ok(None),
    };
    match resolved {
        Ok(Some(patient)) => Ok(patient),
        Ok(None) => {
            let unknown = list::unknown_id();
            let mut response = match mode {
                Mode::Search => {
                    let entry = (format!("urn:uuid:{}", uuid::Uuid::new_v4()), unknown);
                    fhir_json(StatusCode::OK, &list::searchset(Vec::new(), vec![entry]))
                }
                // NOTE: FHIR R4 http.html#read: an id the gateway holds no record of is a 404.
                Mode::Read => fhir_json(StatusCode::NOT_FOUND, &unknown),
            };
            response.extensions_mut().insert(NoAccess);
            Err(response)
        }
        Err(Unresolved::NoStore) => {
            let mut response = outcome(
                StatusCode::BAD_REQUEST,
                "not-supported",
                &Unresolved::NoStore.to_string(),
            );
            response.extensions_mut().insert(NoAccess);
            Err(response)
        }
        Err(failure) => {
            tracing::error!(error = %crate::chain(&failure), "a patient id could not be resolved");
            let mut response = outcome(
                StatusCode::INTERNAL_SERVER_ERROR,
                "exception",
                "the patient id could not be resolved",
            );
            response.extensions_mut().insert(NoAccess);
            Err(response)
        }
    }
}

/// The `matches` every search parameter of `read` selects.
// NOTE: MyHealth@EU NCPeH API 9.1.1-ci; the gateway evaluates every DocumentReference search
// parameter on each entry it lists, whichever node holds the document (#1950).
fn selected(
    matches: Vec<(String, Match)>,
    read: &ListRequest,
) -> Result<Vec<(String, Match)>, serde_json::Error> {
    let now = jiff::Timestamp::now();
    let mut selected = Vec::with_capacity(matches.len());
    for (url, listed) in matches {
        if read.selects(&list::json(&listed)?, now) {
            selected.push((url, listed));
        }
    }
    Ok(selected)
}

/// The `500` of a list that could not name a member's `what` by a signed URL,
/// with the access it records for the patient `system`|`value`.
fn unlisted(
    federation: &Federation,
    gathered: &Gathered,
    (system, value): (&str, &str),
    what: &str,
) -> Response {
    let mut response = outcome(
        StatusCode::INTERNAL_SERVER_ERROR,
        "exception",
        &format!("a member's {what} could not be listed"),
    );
    let accessed =
        gathered::access::accessed(federation, gathered, (system, value), (Scope::List, None));
    attach(&mut response, accessed);
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
