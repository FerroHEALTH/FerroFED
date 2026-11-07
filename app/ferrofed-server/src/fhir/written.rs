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
//! identity binding names it behind the `ehr_id`. Writing the report on
//! read, and naming it by its composition, are our own design: no
//! specification governs them.
//!
//! A member that serves its reports itself has them listed as it holds them
//! ([`super::own`]).

use std::collections::BTreeMap;
use std::time::Instant;

use axum::response::Response;
use ehds_logging::category::Category;
use ferrofed_eehrxf::list::{self, Listing, Match};
use ferrofed_eehrxf::report::{Report, Written};
use ferrofed_eehrxf::summary::Held;
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
use crate::facade::mapped::{self, Answered, Composition};
use crate::facade::native::Chosen;
use crate::facade::provenance::Provenance;
use crate::facade::request::Arrived;
use crate::facade::summary::{Gathered, organisation};
use crate::federation::Federation;
use crate::fhir_wire::fhir_json;

/// The entries of a list: its matches and its outcomes, each by its
/// `fullUrl`.
pub(super) type Listed = (Vec<(String, Match)>, Vec<(String, OperationOutcome)>);

/// The entries the list names for the written reports `gathered` holds,
/// on the face `fhir`, for `caller`: each composition's
/// `DocumentReference`, each by its `fullUrl`, and an `OperationOutcome`
/// for each member that did not answer the query; `None` when a URL cannot
/// be signed.
pub(super) fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
    listing: Listing<'_>,
) -> Option<Listed> {
    let snapshot = federation.snapshot();
    let minted = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let mut matches = Vec::new();
    let mut outcomes = Vec::new();
    for (written, endpoint, held) in once(federation, gathered) {
        let Some(chosen) = member(federation, gathered, &endpoint) else {
            continue;
        };
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
        let id = uuid::Uuid::new_v4().to_string();
        let unique = format!("urn:uuid:{object}");
        matches.push((
            format!("urn:uuid:{id}"),
            Match::Written(Box::new(list::written(
                written,
                (&id, &unique),
                listing,
                &custodian,
                &format!("Binary/{token}"),
            ))),
        ));
    }
    for (endpoint, answered) in gathered.written.values().flatten() {
        if let Answered::Silent { status, .. } = answered {
            outcomes.push((minted(), list::silent(endpoint.as_str(), status.as_str())));
        }
    }
    Some((matches, outcomes))
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
    let served = match written {
        Written::Laboratory => fhir.laboratory.is_some(),
        Written::Discharge => fhir.discharge.is_some(),
    };
    if !served {
        return outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document at this URL; list the documents again",
        );
    }
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .unwrap_or_else(Instant::now);
    let fetched =
        mapped::composition_at(federation, arrived, member, (composition, deadline)).await;
    let (status, left) = match &fetched {
        Ok(Composition::Body(_) | Composition::Absent) => ("active", true),
        Ok(Composition::Silent { status, left }) => (status.as_str(), *left),
        Err(_) => ("node-error", false),
    };
    let mut response = match fetched {
        Ok(Composition::Body(body)) => {
            document(admitted, arrived, (member, composition), (written, &body)).await
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
            (category(written), status, left, delivered),
        ),
    );
    response
}

/// The priority category `written` is recorded under.
const fn category(written: Written) -> Category {
    match written {
        Written::Laboratory => Category::MedicalTestResult,
        Written::Discharge => Category::DischargeReport,
    }
}

/// The document the composition `body` of the version `composition` at
/// `member` is written as, in the `Bundle` profile of `written`, naming its
/// member.
async fn document(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, composition): (&Chosen, &str),
    (written, body): (Written, &[u8]),
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
        operator: &fhir.operator,
        member: &custodian,
    };
    let body = String::from_utf8_lossy(body);
    let bundle: Result<_, Box<dyn std::error::Error + Send + Sync>> = match written {
        Written::Discharge => fhir
            .discharge
            .as_deref()
            .ok_or_else(|| "no discharge mapping".into())
            .and_then(|served| Ok(served.document(&body, report)?)),
        Written::Laboratory => fhir
            .laboratory
            .as_deref()
            .ok_or_else(|| "no laboratory mapping".into())
            .and_then(|served| Ok(served.document(&body, report)?)),
    };
    match bundle {
        Ok(bundle) => Provenance::member(snapshot, &member.via, &member.node)
            .stamp(fhir_json(StatusCode::OK, &bundle)),
        Err(refusal) => {
            tracing::warn!(
                request_id = %admitted.outbound,
                error = %crate::chain(refusal.as_ref()),
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
