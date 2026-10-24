// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The national connector reads the summary built from both members, the
//! document list and the prescriptions, and its dispensation is written to
//! the member that owns the prescription; each node is asked by its own
//! `ehr_id` with no patient identifier (§5.4.1, N33, CP-26) and told the
//! foreign professional and provider (§13.4, N24), and each request leaves
//! one access record naming them (Regulation (EU) 2025/327 Annex II 3.2;
//! Implementing Regulation (EU) 2026/2099 Art 7, Annex Tables 1 and 2).

use std::collections::BTreeMap;
use std::error::Error;
use std::time::{Duration, Instant};

use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::containers;
use ferrofed_testkit::national_connector::{
    Answer, COUNTRY, FAMILY as HP_FAMILY, HCP_ADDRESS, HCP_ID, HCP_NAME, HP_ID, NationalConnector,
    PERSON_ID,
};
use ferrofed_testkit::pix::PixManager;
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, CrossReferenceSeed};
use http::StatusCode;
use serde_json::Value;

use super::{
    CREATED, FAMILY, Harness, RX_A_IDENTIFIER, TestResult, dispensation, national, patient_id,
};
use crate::access::{accesses, details};
use crate::e2e::pixm::{DOMAIN_A, DOMAIN_B, pixm_resolver};
use crate::e2e::{EHR_A, EHR_B, PATIENT, assert_no_patient_identifier_on_the_wire};
use crate::feed_audit::SETTLE;
use crate::support::conveyed::conveyed_claims;

/// How many of the connector's requests reach a member.
const REQUESTS: usize = 4;

/// The resources of the entries of the `Bundle` `bundle` of the type
/// `kind`.
fn resources<'a>(bundle: &'a Value, kind: &str) -> Vec<&'a Value> {
    let kind = Value::from(kind);
    bundle
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .filter(|resource| resource.get("resourceType") == Some(&kind))
        .collect()
}

/// Waits up to [`SETTLE`] for `count` access records at `repository`, and
/// returns them.
async fn access_records(
    repository: &FeedRepository,
    count: usize,
) -> Result<Vec<Value>, Box<dyn Error>> {
    let until = Instant::now() + SETTLE;
    loop {
        let records = accesses(&repository.records())?;
        if records.len() >= count || Instant::now() >= until {
            return Ok(records);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The correlation identifiers `record` carries.
fn correlations(record: &Value) -> Vec<String> {
    record
        .get("entity")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entity| entity.pointer("/type/code") == Some(&Value::from("XrequestId")))
        .flat_map(|entity| {
            entity
                .get("detail")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter(|detail| detail.get("type") == Some(&Value::from("correlation-id")))
        .filter_map(|detail| {
            detail
                .get("valueString")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

/// The conveyance `capture` carries, read as the node reads it.
fn conveyed(capture: &Capture) -> Result<crate::support::conveyed::Conveyed, Box<dyn Error>> {
    let header = ferrofed_engine::conveyance::HEADER.to_ascii_lowercase();
    let (_, value) = capture
        .headers
        .iter()
        .find(|(name, _)| name.to_ascii_lowercase() == header)
        .ok_or_else(|| format!("{} {} carries the conveyance", capture.method, capture.path))?;
    conveyed_claims(std::str::from_utf8(value)?)
}

/// Asserts that every request in `journal` was told the foreign
/// professional and provider, marked as the contact point's assertion, and
/// carried no IUA `person_id`.
fn told_the_relayed_caller(journal: &[Capture], at: &str) -> TestResult {
    assert!(!journal.is_empty(), "{at} was asked");
    for capture in journal {
        let told = conveyed(capture)?
            .national_contact_point
            .ok_or_else(|| format!("{at}: {} names the contact point's caller", capture.path))?;
        assert_eq!(super::NCP_ISSUER, told.asserted_by, "{at}");
        assert_eq!(HP_FAMILY, told.health_professional.family_name, "{at}");
        assert_eq!(HP_ID, told.health_professional.hp_identifier, "{at}");
        assert_eq!(COUNTRY, told.health_professional.country_code, "{at}");
        let provider = &told.healthcare_provider;
        assert_eq!(HCP_ID, provider.healthcare_provider_identifier, "{at}");
        assert_eq!(HCP_NAME, provider.healthcare_provider_name, "{at}");
        assert_eq!(HCP_ADDRESS, provider.healthcare_provider_address, "{at}");
        assert!(
            !capture.contains(PERSON_ID.as_bytes()),
            "N33: {at} saw the IUA person_id"
        );
    }
    Ok(())
}

/// The connector's answers the access records are joined to.
struct Answers {
    summary: Answer,
    prescriptions: Answer,
    read: Answer,
    dispensed: Answer,
}

/// Drives the face from the connector over `harness`: the confirmation, the
/// summary, the document list, the prescriptions, the read of node A's and
/// the dispensation, each answer checked.
async fn drive(
    harness: &Harness,
    connector: &NationalConnector,
) -> Result<Answers, Box<dyn Error>> {
    let held = national(&PATIENT.namespace(), &PATIENT.value());
    let patient = patient_id(&held);
    assert!(
        connector.confirm(&harness.pdq.base_url(), patient).await?,
        "the contact point confirms the identifier at its identity service (A68)"
    );

    let summary = connector.summary(patient).await?;
    assert_eq!(StatusCode::OK, summary.status, "{}", summary.body);
    let document: Value = serde_json::from_str(&summary.body)?;
    assert_eq!(
        document.get("type"),
        Some(&Value::from("document")),
        "{document}"
    );
    assert!(
        resources(&document, "Patient")
            .iter()
            .any(|patient| patient.pointer("/name/0/family") == Some(&Value::from(FAMILY))),
        "the header comes from the PDQm Supplier: {document}"
    );
    for member in ["org-a", "org-b"] {
        assert!(
            summary.body.contains(member),
            "the summary is built from both members: {member} in {document}"
        );
    }

    let documents = connector.documents(patient).await?;
    assert_eq!(StatusCode::OK, documents.status, "{}", documents.body);
    let listed: Value = serde_json::from_str(&documents.body)?;
    assert!(
        !resources(&listed, "DocumentReference").is_empty(),
        "the gateway's summary is listed: {listed}"
    );

    let prescriptions = connector.prescriptions(patient).await?;
    assert_eq!(
        StatusCode::OK,
        prescriptions.status,
        "{}",
        prescriptions.body
    );
    let searched: Value = serde_json::from_str(&prescriptions.body)?;
    let found = resources(&searched, "MedicationRequest");
    assert_eq!(
        2,
        found.len(),
        "one prescription from each member: {searched}"
    );
    let identified = Value::from(RX_A_IDENTIFIER);
    let at_a = found
        .iter()
        .find(|rx| rx.pointer("/identifier/0/value") == Some(&identified))
        .and_then(|rx| rx.get("id"))
        .and_then(Value::as_str)
        .ok_or("node A's prescription, named by the face")?;

    let read = connector.prescription(at_a).await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.body);
    let one: Value = serde_json::from_str(&read.body)?;
    assert_eq!(
        one.pointer("/identifier/0/value"),
        Some(&identified),
        "{one}"
    );

    let dispensed = connector.dispense(&dispensation()).await?;
    assert_eq!(StatusCode::CREATED, dispensed.status, "{}", dispensed.body);
    assert!(
        dispensed
            .location()
            .is_some_and(|at| at.contains("MedicationDispense/")),
        "{:?}",
        dispensed.location()
    );
    Ok(Answers {
        summary,
        prescriptions,
        read,
        dispensed,
    })
}

/// The bodies of the dispensations created through `journal`.
fn created(journal: &[Capture]) -> Vec<Vec<u8>> {
    journal
        .iter()
        .filter(|capture| capture.method == "POST" && capture.path.ends_with("/MedicationDispense"))
        .map(|capture| capture.body.clone())
        .collect()
}

/// Asserts what each member received: the dispensation written once, as
/// sent, at node A's own endpoint alone, every request by the node's own
/// `ehr_id` with no patient identifier, and the relayed caller conveyed.
async fn members_received(harness: &Harness) -> TestResult {
    let (journal_a, journal_b) = (harness.proxy_a.journal(), harness.proxy_b.journal());
    assert_eq!(
        vec![dispensation().into_bytes()],
        created(&journal_a),
        "the dispensation is written once, as sent, to the member that owns the prescription"
    );
    assert!(
        created(&journal_b).is_empty(),
        "nothing is written to node B"
    );
    let posted = |requests: Option<Vec<wiremock::Request>>| {
        requests.is_some_and(|requests| {
            requests
                .iter()
                .any(|request| request.method == http::Method::POST)
        })
    };
    assert!(
        posted(harness.own_a.received_requests().await),
        "node A's own endpoint created {CREATED}"
    );
    assert!(
        !posted(harness.own_b.received_requests().await),
        "node B's own endpoint created nothing"
    );

    assert_no_patient_identifier_on_the_wire(&harness.nodes);
    for (node, ehr_id) in [(&harness.nodes.a, EHR_A), (&harness.nodes.b, EHR_B)] {
        let journal = node.proxy.journal();
        let at = node.node.system_id().to_string();
        assert!(
            journal
                .iter()
                .any(|capture| capture.contains(ehr_id.to_string().as_bytes())),
            "{at} is asked by its own ehr_id (N7)"
        );
        told_the_relayed_caller(&journal, &at)?;
    }
    for (journal, ehr_id, at) in [
        (&journal_a, EHR_A, "node-a-mpd"),
        (&journal_b, EHR_B, "node-b-mpd"),
    ] {
        told_the_relayed_caller(journal, at)?;
        for capture in journal {
            let written = capture.method == "POST";
            for carried in [PATIENT.value(), PATIENT.namespace()] {
                // NOTE: §5.4 scope note, N22: a write's payload reaches the owner as the client sent it.
                assert!(
                    written || !capture.contains(carried.as_bytes()),
                    "N33: {at} saw the patient identifier in {} {}",
                    capture.method,
                    capture.path
                );
            }
        }
        assert!(
            journal
                .iter()
                .any(|capture| capture.contains(ehr_id.to_string().as_bytes())),
            "{at} is asked by its node's own ehr_id"
        );
    }
    Ok(())
}

/// Asserts one access record per request of `read`, each naming the relayed
/// professional and provider as asserted by the contact point.
async fn recorded(repository: &FeedRepository, read: &Answers) -> TestResult {
    let records = access_records(repository, REQUESTS).await?;
    let mut by_correlation: BTreeMap<String, usize> = BTreeMap::new();
    for record in &records {
        let relayed = |kind: &str| details(record, "ehds-relayed", kind);
        assert_eq!(relayed("asserted"), ["true"], "{record}");
        assert_eq!(relayed("hp-family-name"), [HP_FAMILY], "{record}");
        assert_eq!(relayed("hp-identifier"), [HP_ID], "{record}");
        assert_eq!(relayed("provider-name"), [HCP_NAME], "{record}");
        assert_eq!(relayed("provider-identifier"), [HCP_ID], "{record}");
        for correlation in correlations(record) {
            *by_correlation.entry(correlation).or_default() += 1;
        }
    }
    // NOTE: Annex II 3.2: a list that reached no member read no health data at a member and
    // writes no record; no member here serves its own summary, so the list reaches none.
    let expected: BTreeMap<String, usize> = [
        &read.summary,
        &read.prescriptions,
        &read.read,
        &read.dispensed,
    ]
    .iter()
    .map(|answer| (answer.correlation.clone(), 1))
    .collect();
    assert_eq!(
        expected, by_correlation,
        "one access record per request that reached a member"
    );
    Ok(())
}

// conformance: CP-16 CP-26
#[tokio::test]
async fn the_national_connector_reads_the_summary_and_prescriptions_and_dispenses() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Box::pin(Harness::start()).await?;
    let pix = PixManager::start().await?;
    let status = seed::feed(
        &pix.base_url(),
        &CrossReferenceSeed {
            patient: PATIENT,
            ehrs: vec![(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)],
        },
    )
    .await?;
    assert_eq!(StatusCode::CREATED, status, "ITI-104 creates the Patient");
    harness.clear_journals();
    let dir = tempfile::tempdir()?;
    let gateway = harness.gateway(dir.path(), &pixm_resolver(&pix)).await?;

    let read = drive(&harness, &gateway.connector).await?;
    members_received(&harness).await?;
    recorded(&harness.repository, &read).await
}
