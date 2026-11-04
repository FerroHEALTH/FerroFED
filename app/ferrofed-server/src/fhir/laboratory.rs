// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory reports of the document list (Regulation (EU) 2025/327
//! Art 14(1)(e), Annex II 2.1).
//!
//! The list runs the reserved query `eu.ferrofed.eehrxf::laboratory-report`
//! on every member the patient resolves at, beside the patient summary, and
//! names one `EehrxfMhdDocumentReference` of the type `11502-2` per
//! laboratory composition, at a `Binary/{token}` URL the gateway signs for
//! the caller ([`super::binary`]). A composition two members hold under one
//! full version id is listed once, from the member whose `system_id`
//! created it, else the lowest endpoint (Federation Tier §10.3). The read
//! asks that member alone for that composition, by its `ehr_id` alone
//! (§5.4.1, N33), and writes it as an HL7 Europe Laboratory Report 2.0.0
//! `Bundle-eu-lab` through the shipped laboratory mapping compiled against
//! the deployment's laboratory template
//! ([`ferrofed_eehrxf::laboratory::Served`]), its patient named as the
//! identity binding names it behind the `ehr_id`. Writing the report on
//! read, and naming it by its composition, are our own design: no
//! specification governs them.
//!
//! A member that serves laboratory reports itself has each of its own
//! `DocumentReference`s listed as it holds them, its attachment URLs
//! readdressed to `Binary/{token}` URLs the gateway signs, since the
//! member's own URL is one the caller cannot reach and one that would pass
//! the gateway's access check and access record by. The read asks that
//! member alone for its `Binary`, by its `ehr_id` alone, and answers it as
//! the member sent it. Readdressing the URLs is our own design: no
//! specification governs it.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use axum::response::Response;
use ehds_logging::category::Category;
use ehds_logging::classify::{Basis, Evidence};
use ehds_logging::record::{Action, DataSubject, EhrAt, PatientLookup};
use ferrofed_eehrxf::laboratory::Report;
use ferrofed_eehrxf::list::{self, Listing, Match};
use ferrofed_eehrxf::summary::Held;
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::single_node::native_search::{By, SearchAsked, Searched};
use ferrofed_registry::id::EndpointId;
use fhir_types::r4::operation_outcome::OperationOutcome;
use http::StatusCode;
use openehr_base::prelude::ObjectVersionId;
use openehr_base::v1_3::base_types::identification::lexical::{
    composite_id_key, composite_ids_equal,
};
use openehr_federation::status::EndpointStatus;

use super::admission::Admitted;
use super::{attach, outcome};
use crate::access::{Accessed, Asked};
use crate::auth::caller::Caller;
use crate::config::fhir::FhirSettings;
use crate::facade::mapped::{self, Answered, Composition};
use crate::facade::native::{self, Chosen};
use crate::facade::prescriptions;
use crate::facade::provenance::Provenance;
use crate::facade::request::Arrived;
use crate::facade::summary::laboratory::binary;
use crate::facade::summary::{Gathered, organisation};
use crate::federation::Federation;
use crate::fhir_wire::fhir_json;

/// The entries of a list: its matches and its outcomes, each by its
/// `fullUrl`.
pub(super) type Listed = (Vec<(String, Match)>, Vec<(String, OperationOutcome)>);

/// The operation an access record names for the read.
const OPERATION: &str = "Binary";

/// The entries the list names for the laboratory reports `gathered` holds,
/// on the face `fhir`, for `caller`: each composition's
/// `DocumentReference`, each by its `fullUrl`, and an `OperationOutcome`
/// for each member that did not answer the query; `None` when a URL cannot
/// be signed.
fn listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
    listing: Listing<'_>,
) -> Option<Listed> {
    let snapshot = federation.snapshot();
    let minted = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let mut matches = Vec::new();
    let mut outcomes = Vec::new();
    for (endpoint, held) in once(federation, gathered) {
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
                "a member's laboratory composition has no UUID to name its report by"
            );
            continue;
        };
        let signer = federation.signer()?;
        let token =
            super::binary::mint_laboratory(signer, fhir, caller, (&chosen, &held.uid)).ok()?;
        let custodian = organisation(snapshot, &chosen.node);
        let id = uuid::Uuid::new_v4().to_string();
        let unique = format!("urn:uuid:{object}");
        matches.push((
            format!("urn:uuid:{id}"),
            Match::Written(Box::new(list::laboratory(
                (&id, &unique),
                listing,
                &custodian,
                &format!("Binary/{token}"),
            ))),
        ));
    }
    for (endpoint, answered) in &gathered.laboratory {
        if let Answered::Silent { status, .. } = answered {
            outcomes.push((minted(), list::silent(endpoint.as_str(), status.as_str())));
        }
    }
    Some((matches, outcomes))
}

/// The entries the list names for every laboratory report `gathered` holds,
/// the gateway's ([`listed`]) and the members' own ([`own_listed`]), on the
/// face `fhir`, for `caller`; `None` when a URL cannot be signed.
pub(super) fn every(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
    listing: Listing<'_>,
) -> Option<Listed> {
    let (mut matches, mut outcomes) = listed(federation, (fhir, caller), gathered, listing)?;
    let (own, silent) = own_listed(federation, (fhir, caller), gathered)?;
    matches.extend(own);
    outcomes.extend(silent);
    Some((matches, outcomes))
}

/// The entries the list names for the laboratory reports the members of
/// `gathered` that serve them themselves answered with, on the face `fhir`,
/// for `caller`: each member's `DocumentReference` as it holds it, its
/// attachment URLs readdressed to URLs the gateway signs, each by its
/// `fullUrl`, and an `OperationOutcome` for each member that did not answer
/// or answered what the list cannot pass through; `None` when a URL cannot
/// be signed.
fn own_listed(
    federation: &Federation,
    (fhir, caller): (&FhirSettings, &Caller),
    gathered: &Gathered,
) -> Option<Listed> {
    let snapshot = federation.snapshot();
    let minted = || format!("urn:uuid:{}", uuid::Uuid::new_v4());
    let mut matches = Vec::new();
    let mut outcomes = Vec::new();
    for own in &gathered.own_laboratory {
        let endpoint = own.chosen.native.as_str();
        match &own.held {
            prescriptions::Held::Found(pages) => {
                let base = snapshot.native_endpoint(&own.chosen.native)?.url();
                let signer = federation.signer()?;
                for document in pages.iter().flat_map(|page| &page.prescriptions) {
                    // NOTE: no specification governs this: our own design; the attachment URL alone
                    // changes, so the document is read through the gateway's access check and record.
                    let passed = list::readdressed(&document.kept.resource, |url| {
                        let binary = binary(base, url)?;
                        let token = super::binary::mint_own_laboratory(
                            signer,
                            fhir,
                            caller,
                            (&own.chosen, &binary),
                        )
                        .ok()?;
                        Some(format!("Binary/{token}"))
                    })?;
                    matches.push((minted(), Match::Passed(passed)));
                }
            }
            prescriptions::Held::Unusable { reason, .. } => {
                outcomes.push((minted(), list::unread(endpoint, &reason.to_string())));
            }
            prescriptions::Held::Silent(searched) => {
                outcomes.push((minted(), list::silent(endpoint, searched.status().as_str())));
            }
            prescriptions::Held::Unanswered { status, .. } => {
                outcomes.push((minted(), list::silent(endpoint, status.as_str())));
            }
        }
    }
    Some((matches, outcomes))
}

/// Every laboratory composition `gathered` holds, once: a composition two
/// members answered with under one full version id is kept at the member
/// whose `system_id` created it, else at the lowest endpoint.
fn once(federation: &Federation, gathered: &Gathered) -> Vec<(EndpointId, Held)> {
    let snapshot = federation.snapshot();
    let mut kept: BTreeMap<String, (EndpointId, Held, bool)> = BTreeMap::new();
    let mut unversioned = Vec::new();
    for (endpoint, answered) in &gathered.laboratory {
        let Answered::Rows(rows) = answered else {
            continue;
        };
        let system = snapshot
            .endpoint(endpoint)
            .and_then(|held| snapshot.node(held.node()))
            .map(|node| node.system_id().as_str().to_owned());
        for held in rows {
            let Ok(version) = ObjectVersionId::new(&held.uid) else {
                unversioned.push((endpoint.clone(), held.clone()));
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
    let mut every: Vec<(EndpointId, Held)> = kept
        .into_values()
        .map(|(endpoint, held, _)| (endpoint, held))
        .collect();
    every.extend(unversioned);
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

/// `GET {fhir-base}/Binary/{token}` of a laboratory report: the composition
/// of the version `composition` at `member`, written as a `Bundle-eu-lab`.
pub(super) async fn read(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, composition): (&Chosen, &str),
) -> Response {
    let (federation, fhir) = (&admitted.federation, admitted.fhir);
    let Some(served) = fhir.laboratory.as_deref() else {
        return outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document at this URL; list the documents again",
        );
    };
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
            written(admitted, arrived, (member, composition), (served, &body)).await
        }
        Ok(Composition::Absent) => outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "the member holds this document no longer; list the documents again",
        ),
        Ok(Composition::Silent { status, .. }) => unanswered(&member.via, status),
        Err(failure) => failure.respond(admitted.request_id, admitted.outbound),
    };
    let delivered = response.status() == StatusCode::OK;
    attach(
        &mut response,
        accessed(federation, (member, &member.via), (status, left, delivered)),
    );
    response
}

/// `GET {fhir-base}/Binary/{token}` of a laboratory report `member` serves
/// itself: its `Binary` of the logical id `binary`, as the member sent it.
pub(super) async fn read_own(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, binary): (&Chosen, &str),
) -> Response {
    let federation = &admitted.federation;
    let snapshot = federation.snapshot();
    let (Some(client), Some(native)) = (
        federation.clients().get(&member.via),
        snapshot.native_endpoint(&member.native),
    ) else {
        return outcome(
            StatusCode::NOT_FOUND,
            "not-found",
            "no document at this URL; list the documents again",
        );
    };
    // NOTE: §5.4.1, N33; the read names no patient, so the gate withholds nothing beyond what it
    // reads of every request, and the member is located by its ehr_id alone.
    let (options, until) = match native::options(federation, arrived, Arc::new(Withheld::default()))
    {
        Ok(options) => options,
        Err(clock) => {
            tracing::error!(
                error = %crate::chain(&clock),
                request_id = %admitted.outbound,
                "the budget of a laboratory report read could not be held"
            );
            return crate::error::fixed(crate::error::Code::Internal, admitted.request_id);
        }
    };
    let asked = SearchAsked {
        native,
        ehr_id: &member.ehr_id,
        resource: "Binary",
        by: By::Read(binary),
        next: None,
    };
    let searched = client.native_search(asked, &options, until).await;
    let left = searched.contact().left();
    let (mut response, status) = match &searched {
        Searched::Page(forwarded) => (super::binary::passed(forwarded), EndpointStatus::Active),
        // NOTE: FHIR R4 `http.html` §3.1.0.2: a read of a resource the member holds no longer is
        // `404` or `410`, an answer about the document that leaves the member active.
        Searched::Erred(code) if *code == StatusCode::NOT_FOUND || *code == StatusCode::GONE => (
            outcome(
                StatusCode::NOT_FOUND,
                "not-found",
                "the member holds this document no longer; list the documents again",
            ),
            EndpointStatus::Active,
        ),
        silent => (unanswered(&member.native, silent.status()), silent.status()),
    };
    let delivered = response.status() == StatusCode::OK;
    attach(
        &mut response,
        accessed(
            federation,
            (member, &member.native),
            (status.as_str(), left, delivered),
        ),
    );
    response
}

/// The answer to a read whose member, at `endpoint`, gave no answer, with
/// its §11.1 status `status`: `504` for one in `time-out` or `offline` and
/// `424` otherwise (§11.3, §11.4).
fn unanswered(endpoint: &EndpointId, status: EndpointStatus) -> Response {
    let (answer, code) = match status {
        EndpointStatus::TimeOut | EndpointStatus::Offline => {
            (StatusCode::GATEWAY_TIMEOUT, "timeout")
        }
        _ => (StatusCode::FAILED_DEPENDENCY, "incomplete"),
    };
    outcome(
        answer,
        code,
        &format!(
            "the document could not be read: no answer from {endpoint} ({}) (§11.3)",
            status.as_str()
        ),
    )
}

/// The `Bundle-eu-lab` the composition `body` of the version `composition`
/// at `member` is written as by `served`, naming its member.
async fn written(
    admitted: &Admitted<'_>,
    arrived: &Arrived<'_>,
    (member, composition): (&Chosen, &str),
    (served, body): (&ferrofed_eehrxf::laboratory::Served, &[u8]),
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
            "the identity binding names no patient in fhir.receive_namespaces for the member's EHR, and a laboratory report names its patient",
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
    match served.document(&String::from_utf8_lossy(body), report) {
        Ok(bundle) => Provenance::member(snapshot, &member.via, &member.node)
            .stamp(fhir_json(StatusCode::OK, &bundle)),
        Err(refusal) => {
            tracing::warn!(
                request_id = %admitted.outbound,
                error = %crate::chain(&refusal),
                "a member's laboratory composition was not written as a report"
            );
            outcome(
                StatusCode::FAILED_DEPENDENCY,
                "processing",
                &format!(
                    "the laboratory report could not be written: the composition at {} does not map into Bundle-eu-lab",
                    member.via
                ),
            )
        }
    }
}

/// The access record's facts of the read of a laboratory report at
/// `member`, asked at its `endpoint`, whose answer showed the §11.1 status
/// `status`, a request having left when `left`, the report delivered when
/// `delivered` (Annex II 3.2); `None` when nothing left or no access log is
/// kept.
fn accessed(
    federation: &Federation,
    (member, endpoint): (&Chosen, &EndpointId),
    (status, left, delivered): (&str, bool, bool),
) -> Option<Accessed> {
    let log = federation.access_log()?;
    if !left {
        return None;
    }
    let evidence =
        || Evidence::reached(Basis::Returned, Vec::new()).constructed(Category::MedicalTestResult);
    Some(Accessed {
        log: Arc::clone(log),
        action: Action::Read,
        operation: OPERATION,
        resource: None,
        query: None,
        stored_query: None,
        subject: DataSubject {
            patient: None,
            ehrs: vec![EhrAt {
                endpoint: endpoint.as_str().to_owned(),
                ehr_id: member.ehr_id.as_str().to_owned(),
                patient: PatientLookup::RequestNamed,
            }],
            population: None,
        },
        evidence: evidence(),
        delivered: Some(usize::from(delivered)),
        origins: vec![Asked {
            endpoint: endpoint.as_str().to_owned(),
            node: Some(member.node.as_str().to_owned()),
            system_id: federation
                .snapshot()
                .node(&member.node)
                .map(|node| node.system_id().as_str().to_owned()),
            status: status.to_owned(),
            rows: Some(usize::from(delivered)),
            version: None,
            created_ehr: None,
            contributed: delivered,
            evidence: delivered.then(evidence),
        }],
        consent_set_aside: std::collections::BTreeSet::new(),
        inserted: None,
        transformations: Vec::new(),
    })
}
