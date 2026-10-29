// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary served to a caller confined to one patient, as an
//! electronic health data access service asks for a person's own summary
//! (Regulation (EU) 2025/327 Art 3(1), Art 4): the token's own patient gets
//! their summary from every member that holds them, another patient is
//! refused `403` before the demographics supplier or any member is asked
//! (Federation Tier §5.2), no patient identifier reaches a node or a log
//! line (§5.4.1, N33), and the access record names the person as the caller
//! and as the data subject (Art 9, Annex II 3.2). A professional's request in
//! the same deployment is served as before.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::{ACT_REASON, Claims, Coding};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub::{self, Entry};
use http::StatusCode;
use serde_json::Value;

use super::receive::{EHR_SYSTEM, OTHER, OTHER_A, OTHER_NAMESPACE, bound, rows};
use super::{
    FAMILY, GIVEN, TestResult, UID_A, UID_B, known, node_holding, resources, summary,
    text_registered,
};
use crate::auth::{bearing, claims, minted};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::{Logs, call};

/// The `ehr_id` of [`OTHER`] at node B.
const OTHER_B: &str = "5555eeee-5555-4555-8555-555555555555";

/// The synthetic person the access service acts for: the token's `sub`.
const PERSON: &str = "synthetic-person-6r3d";

/// The scope an access service holds for the patient summary: a search of
/// the gateway's own section queries, confined to the token's patient.
const SUMMARY_SCOPE: &str = "patient/aql-eu.ferrofed.eehrxf::*.s";

/// The claims of a person's own grant: confined to [`PATIENT`], whose EHR
/// at node A the token names, for the purpose `PATRQT`.
fn own_grant() -> Claims {
    let mut granted = claims();
    PERSON.clone_into(&mut granted.sub);
    granted.scope = Some(SUMMARY_SCOPE.to_owned());
    granted.ehr_id = Some(EHR_A.to_owned());
    if let Some(extensions) = granted.extensions.as_mut() {
        // NOTE: HL7 v3 ActReason PATRQT, "patient requested", as the vendored BALP examples
        // code a disclosure the patient asked for.
        extensions.ihe_iua.purpose_of_use = vec![Coding {
            system: ACT_REASON.to_owned(),
            code: String::from("PATRQT"),
        }];
    }
    granted
}

/// A stub PDQm Supplier answering a lookup of [`PATIENT`] and one of
/// [`OTHER`] each with that patient.
async fn suppliers() -> Server {
    pdqm_stub::by_identifier(&[
        (PATIENT, &[known()]),
        (
            OTHER,
            &[Entry::new(&[(OTHER_NAMESPACE, OTHER)]).named((FAMILY, GIVEN), None)],
        ),
    ])
    .await
}

/// The summary gateway over node A and node B, its issuer's patient tokens
/// bound to node A, the cross-reference knowing both patients at both
/// members and the token's `ehrId` in node A's `ehr_id` system, with the
/// tables `tables` added.
fn gateway(
    dir: &Path,
    (a, b, pdq): (&Server, &Server, &Server),
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let crossref = rows(&[
        (EHR_SYSTEM, EHR_A, "node-a", EHR_A),
        (EHR_SYSTEM, EHR_A, "node-b", EHR_B),
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
        (OTHER_NAMESPACE, OTHER, "node-b", OTHER_B),
    ]);
    let members = [("node-a", EHR_A), ("node-b", EHR_B)];
    let text = text_registered(
        dir,
        (&a.uri(), &b.uri(), &pdqm_stub::base_url(pdq)),
        &members,
        "",
        ("", &format!("{crossref}{tables}")),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, &bound()?))
}

/// `GET {fhir-base}/Patient/$summary` for [`OTHER`].
fn other_summary() -> Result<http::Request<axum::body::Body>, http::Error> {
    http::Request::get(format!(
        "{}/Patient/$summary?identifier={}%7C{OTHER}",
        super::FHIR,
        OTHER_NAMESPACE.replace(':', "%3A")
    ))
    .body(axum::body::Body::empty())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_persons_own_grant_gets_their_summary_and_no_identifier_reaches_a_node_or_a_log()
-> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = suppliers().await;
    let app = gateway(dir.path(), (&a, &b, &pdq), "")?;
    let token = minted(&own_grant())?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, bearing(summary("")?, &token)?).await?;
    drop(guard);
    assert_eq!(StatusCode::OK, status, "Art 3(1): {text}");
    let document: Value = serde_json::from_str(&text)?;
    let patient = resources(&document, "Patient");
    assert_eq!(
        patient
            .first()
            .and_then(|patient| patient.pointer("/identifier/0/value")),
        Some(&Value::from(PATIENT)),
        "the person's own summary"
    );
    for (node, ehr_id) in [(&a, EHR_A), (&b, EHR_B)] {
        let dispatched = received(node).await?;
        assert_eq!(
            Section::ALL.len(),
            dispatched.len(),
            "every member that holds the person is asked each section query"
        );
        for body in &dispatched {
            assert!(body.contains(ehr_id), "§7.1: scoped to {ehr_id}: {body}");
        }
        let captured = wire(node).await?;
        for withheld in [PATIENT, PATIENT_TAIL] {
            assert!(
                !captured.contains(withheld),
                "N33: no identifier in the query, path or headers: {captured}"
            );
        }
    }
    let logged = logs.text();
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !logged.contains(withheld),
            "N33: no identifier in a log line: {logged}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_persons_own_grant_asking_for_another_patient_is_refused_before_anyone_is_asked()
-> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = suppliers().await;
    let app = gateway(dir.path(), (&a, &b, &pdq), "")?;
    let token = minted(&own_grant())?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text) = call(app, bearing(other_summary()?, &token)?).await?;
    drop(guard);
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("forbidden"))
    );
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.starts_with("patient-confinement"),
        "the code every confined route answers: {diagnostics}"
    );
    assert!(
        !text.contains(OTHER),
        "the refusal names no patient: {text}"
    );
    assert_eq!(
        0,
        pdqm_stub::searches(&pdq).await,
        "the demographics supplier is never asked about another patient"
    );
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no member is asked"
    );
    assert!(
        !logs.text().contains(OTHER),
        "N33: no identifier in a log line"
    );
    Ok(())
}

#[tokio::test]
async fn a_professional_in_the_same_deployment_is_served_as_before() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = suppliers().await;
    let app = gateway(dir.path(), (&a, &b, &pdq), "")?;
    let token = minted(&claims())?;
    let (status, text) = call(app, bearing(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        pdqm_stub::searches(&pdq).await,
        "one header lookup, as before"
    );
    for node in [&a, &b] {
        assert_eq!(Section::ALL.len(), received(node).await?.len());
    }
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn the_access_record_names_the_person_as_caller_and_as_data_subject() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let pdq = suppliers().await;
    let app = gateway(dir.path(), (&a, &b, &pdq), &audit_tables(&repository, ""))?;
    let token = minted(&own_grant())?;
    let (status, text) = call(app, bearing(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(2, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(1, records.len(), "one access record: {records:#?}");
    let record: Value = serde_json::from_str(records.first().ok_or("a record")?)?;
    let agents = record["agent"].as_array().ok_or("agents")?;
    let user = agents
        .iter()
        .find(|agent| agent.to_string().contains(PERSON))
        .ok_or("Annex II 3.2(b): the person is the caller")?;
    let user = user.to_string();
    assert!(
        user.contains("\"person\""),
        "the person acts in person: {user}"
    );
    assert!(user.contains("PATRQT"), "the purpose: {user}");
    let subject = record["entity"]
        .as_array()
        .ok_or("entities")?
        .iter()
        .find(|entity| entity.pointer("/what/identifier/value") == Some(&Value::from(PATIENT)))
        .ok_or("Art 9(1): the person is the data subject")?;
    assert_eq!(
        subject.pointer("/what/identifier/system"),
        Some(&Value::from(NAMESPACE))
    );
    let text = record.to_string();
    for ehr_id in [EHR_A, EHR_B] {
        assert!(text.contains(ehr_id), "each EHR reached: {ehr_id}");
    }
    Ok(())
}

/// The claims of a person's own grant of [`own_grant`], scoped to the
/// lookup: the read of the person's EHRs.
fn own_lookup_grant() -> Claims {
    let mut granted = own_grant();
    granted.scope = Some(String::from("patient/composition-*.r"));
    granted
}

/// `GET {fhir-base}/Patient` for the patient `namespace`|`value`.
fn looked_up(namespace: &str, value: &str) -> Result<http::Request<axum::body::Body>, http::Error> {
    http::Request::get(format!(
        "{}/Patient?identifier={}%7C{value}",
        super::FHIR,
        namespace.replace(':', "%3A")
    ))
    .body(axum::body::Body::empty())
}

#[tokio::test]
async fn a_persons_own_grant_looks_up_their_own_patient_and_is_refused_another() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = suppliers().await;
    let app = gateway(dir.path(), (&a, &b, &pdq), "")?;
    let token = minted(&own_lookup_grant())?;

    let (status, text) = call(
        app.clone(),
        bearing(looked_up(OTHER_NAMESPACE, OTHER)?, &token)?,
    )
    .await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "§5.2: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(
        diagnostics.starts_with("patient-confinement"),
        "the code every confined route answers: {diagnostics}"
    );
    assert!(
        !text.contains(OTHER),
        "the refusal names no patient: {text}"
    );
    assert_eq!(
        0,
        pdqm_stub::searches(&pdq).await,
        "the demographics supplier is never asked about another patient"
    );

    let (status, text) = call(app, bearing(looked_up(NAMESPACE, PATIENT)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "Art 3(1): {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    assert_eq!(
        bundle.pointer("/entry/0/resource/identifier/0/value"),
        Some(&Value::from(PATIENT)),
        "the person's own Patient: {bundle}"
    );
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no member is asked"
    );
    Ok(())
}
