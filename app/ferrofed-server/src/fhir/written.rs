// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reports the gateway writes for the document list (Regulation (EU)
//! 2025/327 Art 14(1)(e) and (f), Annex II 2.1).
//!
//! The list runs the reserved query of each written category
//! (`eu.ferrofed.eehrxf::laboratory-report`,
//! `eu.ferrofed.eehrxf::discharge-report`) on every member the patient
//! resolves at, beside the patient summary, and names one
//! `EehrxfMhdDocumentReference` per composition, of the type `11502-2` for a
//! laboratory report and `18842-5` for a discharge report, at a
//! `Binary/{token}` URL the gateway signs for the caller ([`super::binary`]).
//! A composition two members hold under one full version id is listed once,
//! from the member whose `system_id` created it, else the lowest endpoint
//! (Federation Tier §10.3). The read asks that member alone for that
//! composition, by its `ehr_id` alone (§5.4.1, N33), and writes it through
//! the shipped mapping compiled against the deployment's template, as an HL7
//! Europe Laboratory Report 2.0.0 `Bundle-eu-lab`
//! ([`ferrofed_eehrxf::laboratory::Served`]) or an HL7 Europe Hospital
//! Discharge Report 1.0.0 `bundle-eu-hdr`
//! ([`ferrofed_eehrxf::discharge::Served`]), its patient named as the
//! identity binding names it behind the `ehr_id`, with the header the
//! demographics binding holds of them, as the patient summary is. Writing
//! the report on read, and naming it by its composition, are our own
//! design: no specification governs them.
//!
//! The vaccination history (MyHealth@EU Requirements Catalogue v10.0.0
//! §1.24) is listed the same way from `eu.ferrofed.eehrxf::vaccination-history`,
//! one per attested immunisation list per member, and read from the
//! version's `ORIGINAL_VERSION`, whose attesters are its legal authenticators
//! ([`ferrofed_eehrxf::vaccination`]); one that cannot be written is listed
//! unavailable with its reason.
//!
//! A member that serves its reports itself has them listed as it holds them
//! ([`super::own`]).

use std::collections::BTreeMap;
use std::time::Instant;

use axum::response::Response;
use eehrxf::terminology::translation::Unbound;
use ferrofed_eehrxf::list::written::Writing;
use ferrofed_eehrxf::list::{self, Listing, Match};
use ferrofed_eehrxf::report::{Report, ServedError, Writes, Written};
use ferrofed_eehrxf::summary::Held;
use ferrofed_eehrxf::vaccination;
use ferrofed_identity::role::header::PatientHeader;
use ferrofed_registry::id::EndpointId;
use fhir_types::r4::operation_outcome::OperationOutcome;
use http::StatusCode;
use openehr_base::prelude::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::lexical::{
    composite_id_key, composite_ids_equal,
};

use super::admission::Admitted;
use super::{attach, outcome};
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::facade::demographics::Unheaded;
use crate::facade::mapped::{self, Answered, Composition};
use crate::facade::native::Chosen;
use crate::facade::provenance::Provenance;
use crate::facade::request::Arrived;
use crate::facade::summary::written::recorded;
use crate::facade::summary::{Gathered, organisation};
use crate::federation::Federation;
use crate::fhir_wire::fhir_json;

/// The entries of a list: its matches and its outcomes, each by its
/// `fullUrl`.
pub(super) type Listed = (Vec<(String, Match)>, Vec<(String, OperationOutcome)>);

/// A fresh `urn:uuid:` URN, the `fullUrl` of a list entry.
pub(super) fn minted_urn() -> String {
    format!("urn:uuid:{}", uuid::Uuid::new_v4())
}

/// The entries the list names for the written reports `gathered` holds,
/// on the face `fhir`, for `caller`: each composition's
/// `DocumentReference`, each by its `fullUrl`, and an `OperationOutcome`
/// for each member that did not answer the query and for each vaccination
/// history that cannot be written, because `subject` says why the patient
/// cannot be its subject or no person attested the record; `None` when a
/// URL cannot be signed.
pub(super) fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
    (listing, subject): (Listing<'_>, Option<&Result<(), String>>),
) -> Option<Listed> {
    let snapshot = federation.snapshot();
    let (mut matches, mut outcomes): Listed = (Vec::new(), Vec::new());
    for (written, endpoint, held) in once(federation, gathered) {
        let Some(chosen) = member(federation, gathered, &endpoint) else {
            continue;
        };
        if written == Written::Vaccination
            && let Some(reason) = unwritable(&held, subject)
        {
            outcomes.push((
                minted_urn(),
                list::unavailable(endpoint.as_str(), written.named(), &reason),
            ));
            continue;
        }
        let Ok(version) = ObjectVersionId::new(&held.uid) else {
            continue;
        };
        let object = version.object_id().value().into_owned();
        // NOTE: no specification governs this: our own design; a composition whose versioned object
        // is no UUID gives the document no urn:uuid identifier, so it is not listed.
        let Ok(object) = uuid::Uuid::parse_str(&object) else {
            tracing::warn!(
                endpoint = endpoint.as_str(),
                "a member's composition has no UUID to name its report by"
            );
            continue;
        };
        let signer = federation.signer()?;
        let token =
            super::binary::mint_written(signer, fhir, caller, (&chosen, written, &held.uid))
                .ok()?;
        let custodian = organisation(snapshot, &chosen.node);
        let unique = format!("urn:uuid:{object}");
        let id = fhir.ids.document((endpoint.as_str(), &unique));
        let writing = Writing {
            written,
            composition: &held.composition,
            operator: &fhir.operator,
            custodian: &custodian,
        };
        let described = list::written::entry(
            writing,
            (id.as_str(), &unique),
            listing,
            &format!("Binary/{token}"),
        );
        // NOTE: draft EEHRxF Annex III Table 1: a composition with no version cannot be described,
        // so it is not listed, as one with no UUID is not.
        let described = match described {
            Ok(described) => described,
            Err(undescribed) => {
                tracing::warn!(
                    endpoint = endpoint.as_str(),
                    error = %crate::chain(&undescribed),
                    "a member's composition cannot be described as a report"
                );
                continue;
            }
        };
        matches.push((
            format!("{}/DocumentReference/{}", fhir.absolute, id.as_str()),
            Match::Written(Box::new(described)),
        ));
    }
    for (endpoint, answered) in gathered.written.values().flatten() {
        if let Answered::Silent { status, .. } = answered {
            outcomes.push((
                minted_urn(),
                list::silent(endpoint.as_str(), status.as_str()),
            ));
        }
    }
    Some((matches, outcomes))
}

/// Why the vaccination history of `held` cannot be written, when it cannot:
/// `subject` says why the patient cannot be its subject, or no person
/// attested the version.
fn unwritable(held: &Held, subject: Option<&Result<(), String>>) -> Option<String> {
    if let Some(Err(reason)) = subject {
        return Some(reason.clone());
    }
    let attested = vaccination::listed(held.attestations.as_deref());
    attested.err().map(|unattested| unattested.to_string())
}

/// Whether the identity binding names the patient `patient` as a
/// vaccination history's subject must be named, asked once for a list
/// `gathered` holds a vaccination history of: `None` when it holds none,
/// else `Ok`, or why not.
pub(super) async fn subject(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    gathered: &Gathered,
    patient: (&str, &str),
) -> Option<Result<(), String>> {
    let held = gathered
        .written
        .get(&Written::Vaccination)?
        .iter()
        .any(|(_, answered)| matches!(answered, Answered::Rows(rows) if !rows.is_empty()));
    if !held {
        return None;
    }
    Some(match header(admitted, arrived, patient).await {
        Ok(header) => vaccination::named(&header).map_err(|unnamed| unnamed.to_string()),
        Err(Unheaded::Ambiguous(_)) => Err(String::from(
            "the identity binding names no one patient for the identifier",
        )),
        Err(_) => Err(String::from(
            "the identity binding could not answer for the patient",
        )),
    })
}

/// Every composition of each written category `gathered` holds, once: a
/// composition two members answered with under one full version id is kept
/// at the member whose `system_id` created it, else at the lowest endpoint.
fn once(federation: &Federation, gathered: &Gathered) -> Vec<(Written, EndpointId, Held)> {
    let snapshot = federation.snapshot();
    let mut every = Vec::new();
    for (written, answers) in &gathered.written {
        let mut kept: BTreeMap<String, (EndpointId, Held, bool)> = BTreeMap::new();
        for (endpoint, answered) in answers {
            let Answered::Rows(rows) = answered else {
                continue;
            };
            let system = snapshot
                .endpoint(endpoint)
                .and_then(|held| snapshot.node(held.node()))
                .map(|node| node.system_id().as_str().to_owned());
            for held in rows {
                let Ok(version) = ObjectVersionId::new(&held.uid) else {
                    every.push((*written, endpoint.clone(), held.clone()));
                    continue;
                };
                let originating = system.as_deref().is_some_and(|system| {
                    composite_ids_equal(system, version.creating_system_id_str())
                });
                let key = composite_id_key(version.value());
                // NOTE: Federation Tier §10.3: the copy kept is the originating member's, else the
                // lowest endpoint's, which the endpoint order visits first.
                let replace = kept
                    .get(&key)
                    .is_none_or(|(_, _, held_originating)| originating && !held_originating);
                if replace {
                    kept.insert(key, (endpoint.clone(), held.clone(), originating));
                }
            }
        }
        every.extend(
            kept.into_values()
                .map(|(endpoint, held, _)| (*written, endpoint, held)),
        );
    }
    every
}

/// The member behind the Query API endpoint `endpoint`, as the list reached
/// it by the `ehr_id` its patient resolved to there.
fn member(federation: &Federation, gathered: &Gathered, endpoint: &EndpointId) -> Option<Chosen> {
    let node = federation.snapshot().endpoint(endpoint)?.node().clone();
    let ehr_id = gathered
        .resolved
        .iter()
        .find(|(resolved, _)| resolved == &node)?
        .1
        .clone();
    Some(Chosen {
        via: endpoint.clone(),
        node,
        native: endpoint.clone(),
        ehr_id,
    })
}

/// `GET {fhir-base}/Binary/{token}` of a written report: the composition of
/// the version `composition` at `member`, written in the `Bundle` profile of
/// `written`.
pub(super) async fn read(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, written, composition): (&Chosen, Written, &str),
) -> Response {
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let Some(served) = fhir.writes(written) else {
        return outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document at this URL; list the documents again",
        );
    };
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .unwrap_or_else(Instant::now);
    // NOTE: openEHR RM Common: attestations live on the ORIGINAL_VERSION, not the composition, so
    // the vaccination history's legal authenticators are read with the version.
    let fetched = match written {
        Written::Vaccination => {
            let object = ObjectVersionId::new(composition).map_or_else(
                |_| composition.to_owned(),
                |version| version.object_id().value().into_owned(),
            );
            mapped::version_at(
                federation,
                arrived,
                member,
                ((&object, composition), deadline),
            )
            .await
        }
        _ => mapped::composition_at(federation, arrived, member, (composition, deadline)).await,
    };
    let (status, left) = match &fetched {
        Ok(Composition::Body(_) | Composition::Absent) => ("active", true),
        Ok(Composition::Silent { status, left }) => (status.as_str(), *left),
        Err(_) => ("node-error", false),
    };
    let mut response = match fetched {
        Ok(Composition::Body(body)) => {
            document(
                admitted,
                arrived,
                (member, composition),
                (written, served, &body),
            )
            .await
        }
        Ok(Composition::Absent) => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "the member holds this document no longer; list the documents again",
        ),
        Ok(Composition::Silent { status, .. }) => super::own::unanswered(&member.via, status),
        Err(failure) => failure.respond(admitted.request_id, admitted.outbound),
    };
    let delivered = response.status() == StatusCode::OK;
    attach(
        &mut response,
        super::own::accessed(
            federation,
            (member, &member.via),
            (recorded(written), status, left, delivered),
        ),
    );
    response
}

/// The document the composition `body` of the version `composition` at
/// `member` is written as, in the `Bundle` profile of `written`, naming its
/// member.
async fn document(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, composition): (&Chosen, &str),
    (written, served, body): (Written, &dyn Writes, &[u8]),
) -> Response {
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .unwrap_or_else(Instant::now);
    let named = mapped::named(
        federation,
        arrived,
        member,
        (&fhir.receive_namespaces, deadline),
    )
    .await;
    let Some((system, value)) = named else {
        return outcome(
            StatusCode::FAILED_DEPENDENCY,
            "processing",
            &format!(
                "the identity binding names no patient in fhir.receive_namespaces for the member's EHR, and a {} names its patient",
                written.named()
            ),
        );
    };
    let header = match header(admitted, arrived, (&system, &value)).await {
        Ok(header) => header,
        Err(unheaded) => {
            return super::patient_summary::unheaded_outcome(unheaded, written.named());
        }
    };
    let object = ObjectVersionId::new(composition).map_or_else(
        |_| composition.to_owned(),
        |version| version.object_id().value().into_owned(),
    );
    let identifier = format!("urn:uuid:{object}");
    let timestamp = jiff::Timestamp::now()
        .strftime("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let snapshot = federation.snapshot();
    let custodian = organisation(snapshot, &member.node);
    let report = Report {
        base: &fhir.absolute,
        identifier: &identifier,
        timestamp: &timestamp,
        patient: (&system, &value),
        header: &header,
        operator: &fhir.operator,
        member: &custodian,
        attesters: &[],
    };
    let body = String::from_utf8_lossy(body).into_owned();
    let (body, attesters) = match versioned(admitted, written, body) {
        Ok(versioned) => versioned,
        Err(refused) => return refused,
    };
    let report = Report {
        attesters: &attesters,
        ..report
    };
    let delivered = served
        .document(&body, report, (&fhir.terminology, fhir.bindings))
        .await;
    match delivered {
        Ok(delivered) => {
            unbound(admitted, written, &delivered.unbound);
            Provenance::member(snapshot, &member.via, &member.node)
                .stamp(fhir_json(StatusCode::OK, &delivered.bundle))
        }
        Err(refusal @ (ServedError::Unnamed(_) | ServedError::Unattested(_))) => outcome(
            StatusCode::FAILED_DEPENDENCY,
            "processing",
            &format!(
                "the {} could not be written: {}",
                written.named(),
                crate::chain(&refusal)
            ),
        ),
        Err(refusal) => {
            tracing::warn!(
                request_id = %admitted.outbound,
                error = %crate::chain(&refusal),
                "a member's composition was not written as a report"
            );
            outcome(
                StatusCode::FAILED_DEPENDENCY,
                "processing",
                &format!(
                    "the {} could not be written: the composition at {} does not map into {}",
                    written.named(),
                    member.via,
                    written.bundle()
                ),
            )
        }
    }
}

/// Logs, by element path alone, each concept of the `written` report no
/// translation brought into a binding `[eehrxf.terminology] transcode`
/// names, which the report carries as recorded.
fn unbound(admitted: &Admitted<'_>, written: Written, unbound: &[Unbound]) {
    if unbound.is_empty() {
        return;
    }
    tracing::warn!(
        request_id = %admitted.outbound,
        unbound = unbound.len(),
        locations = ?unbound.iter().map(|unbound| unbound.location.as_str()).collect::<Vec<_>>(),
        "codes of the {} meet no binding [eehrxf.terminology] transcode names, and are transferred as recorded",
        written.named()
    );
}

/// The composition `body` holds and the persons who attested it: for a
/// vaccination history, `body` is the version's `ORIGINAL_VERSION`, and a
/// version that gives none is the `424` that says why; for every other
/// report, `body` is the composition, attested by no one the report names.
#[expect(
    clippy::result_large_err,
    reason = "the refusal is the handler's answer, returned at most once per request"
)]
fn versioned(
    admitted: &Admitted<'_>,
    written: Written,
    body: String,
) -> Result<(String, Vec<vaccination::Attester>), Response> {
    if written != Written::Vaccination {
        return Ok((body, Vec::new()));
    }
    vaccination::original_version(&body).map_err(|unversioned| {
        tracing::warn!(
            request_id = %admitted.outbound,
            error = %crate::chain(&unversioned),
            "a member's version was not written as a vaccination history"
        );
        outcome(
            StatusCode::FAILED_DEPENDENCY,
            "processing",
            &format!("the vaccination history could not be written: {unversioned}"),
        )
    })
}

/// The header the demographics binding holds of the patient
/// `system`|`value` for a request that `arrived`: what it holds, named or
/// not, and an empty header when it holds no one for the identifier or is
/// not asked about its namespace. The report states every element the header
/// lacks absent.
///
/// A binding whose answer names no one patient, or that could not answer,
/// is the [`Unheaded`] that refuses the report.
async fn header(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    patient: (&str, &str),
) -> Result<PatientHeader, Unheaded> {
    let named = super::patient_summary::named(patient)?;
    let headed = super::patient_summary::headed(
        &admitted.federation,
        (&named, arrived.on_behalf),
        arrived.started,
    )
    .await;
    // NOTE: HL7 Europe Base 2.0.1 patient-eu-core eu-pat-1 and Patient.birthDate: an element the
    // binding does not hold is written with a data-absent-reason, so no value is filled in.
    match headed {
        Ok(held) => Ok(held),
        Err(Unheaded::Unnamed(unnamed)) => Ok(*unnamed),
        Err(Unheaded::NoBinding | Unheaded::NoMatch) => Ok(PatientHeader::default()),
        Err(refused @ (Unheaded::Ambiguous(_) | Unheaded::Unavailable { .. })) => Err(refused),
    }
}
