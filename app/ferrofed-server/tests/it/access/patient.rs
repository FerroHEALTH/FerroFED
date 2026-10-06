// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient behind an `ehr_id` a routed request addressed (Regulation
//! (EU) 2025/327 Art 9(1); IHE `RESTful` ATNA §3.81.4.1.2.2): the record
//! names the patient the identity binding holds under it, in each namespace
//! `[access_log] patient_namespaces` names, as the request's own patient is
//! named, so a `patient.identifier` search finds the access; a patient it
//! cannot name is said so beside the `ehr_id`, and the access is recorded
//! and answered all the same; and the identifier reaches no node and no log
//! line (§5.4, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    LAB_REPORT, TestResult, accesses, composition, details, named, profile, settings_resolving,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, crossref, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::request_log::logged;
use crate::support::call;

const UID: &str = "4c1b2a39-8d7e-4f6a-9b5c-0e1d2c3b4a59::cdr-a.example.org::1";

/// The read of the lab report at node A, by its `ehr_id` alone.
fn read() -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{EHR_A}/composition/{UID}"))
        .header("openEHR-federation-endpoint", "node-a-pub")
        .body(Body::empty())
}

/// A node answering the read of the lab report under [`EHR_A`].
async fn node() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{UID}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            composition(LAB_REPORT, UID).into_bytes(),
            "application/json",
        ))
        .mount(&server)
        .await;
    server
}

/// The `[access_log]` key naming `namespaces`, or nothing for none.
fn namespaces(namespaces: &[&str]) -> String {
    if namespaces.is_empty() {
        return String::new();
    }
    let quoted: Vec<String> = namespaces
        .iter()
        .map(|namespace| format!("\"{namespace}\""))
        .collect();
    format!(
        "\n[access_log]\npatient_namespaces = [{}]\n",
        quoted.join(", ")
    )
}

/// A gateway over `node` resolving through `identity`, naming patients in
/// `asked`, recording to `repository`.
fn gateway(
    dir: &std::path::Path,
    node: &Server,
    repository: &FeedRepository,
    (identity, asked): (&str, &[&str]),
) -> Result<Router, Box<dyn Error>> {
    let idle = format!("{}/node-b", node.uri());
    let settings = settings_resolving(
        dir,
        (&node.uri(), &idle),
        repository,
        ("", ""),
        (&namespaces(asked), identity),
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, &settings_with_room()))
}

/// The development cross-reference, the patient at both members.
fn cross_reference() -> String {
    crossref(&[("node-a", EHR_A), ("node-b", EHR_B)])
}

/// The one access record of the read through `identity`, naming patients in
/// `asked`, with the node that answered it.
async fn record_of(identity: &str, asked: &[&str]) -> Result<(Value, Server), Box<dyn Error>> {
    let node = node().await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &node, &repository, (identity, asked))?;
    let (status, text) = call(app, read()?).await?;
    assert_eq!(StatusCode::OK, status, "the read is answered: {text}");
    // NOTE: no specification governs this: our own design; the identity service's own
    // ITI-83 record may arrive first, so the wait goes on until the access record does.
    let mut records = Vec::new();
    for count in 1..=3 {
        records = accesses(&repository.wait_for(count, SETTLE).await)?;
        if !records.is_empty() {
            break;
        }
    }
    let [record] = records.as_slice() else {
        return Err(format!("one record of the read, got {}", records.len()).into());
    };
    Ok((record.clone(), node))
}

/// The identifier values of the `entity:patient`s of `record`.
fn patients(record: &Value) -> Vec<(String, String)> {
    record["entity"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entity| {
            entity.pointer("/role/code").and_then(Value::as_str) == Some("1")
                && entity.pointer("/type/code").and_then(Value::as_str) == Some("1")
        })
        .map(|entity| {
            let text = |pointer: &str| {
                entity
                    .pointer(pointer)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            (
                text("/what/identifier/system"),
                text("/what/identifier/value"),
            )
        })
        .collect()
}

/// The `ehr` entity of `record` keeps the `ehr_id`, so an
/// `entity.identifier` search finds the access, and says how its patient
/// was looked up.
fn ehr_says(record: &Value, lookup: &str) -> TestResult {
    let ehrs = named(record, "ehr");
    let [ehr] = ehrs.as_slice() else {
        return Err(format!("one ehr entity, got {ehrs:?}").into());
    };
    if ehr
        .pointer("/what/identifier/value")
        .and_then(Value::as_str)
        != Some(EHR_A)
    {
        return Err(format!("the ehr entity names another ehr_id: {ehr}").into());
    }
    let said = details(record, "ehr", "patient-lookup");
    if said != [lookup] {
        return Err(format!("the patient lookup is {said:?}, not {lookup}").into());
    }
    Ok(())
}

/// Every request `server` received, as text.
async fn received(server: &Server) -> Result<String, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .map(|request| {
            format!(
                "{} {:?} {}",
                request.url,
                request.headers,
                String::from_utf8_lossy(&request.body)
            )
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Art 9(1): the patient the cross-reference holds under the `ehr_id` is
/// the record's patient, and the read claims `PatientRead`; the node is
/// asked by the `ehr_id` alone (N33).
#[tokio::test]
async fn a_read_by_ehr_id_names_the_patient_the_identity_binding_holds() -> TestResult {
    let (record, node) = record_of(&cross_reference(), &[NAMESPACE]).await?;
    assert_eq!(
        vec![(NAMESPACE.to_owned(), PATIENT.to_owned())],
        patients(&record)
    );
    assert_eq!(
        profile(&record),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.PatientRead")
    );
    ehr_says(&record, "found")?;
    let wire = received(&node).await?;
    assert!(!wire.is_empty(), "the node was asked");
    assert!(
        !wire.contains(PATIENT),
        "no patient identifier reaches the node: {wire}"
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_with_no_identifier_in_the_namespace_is_said_not_found() -> TestResult {
    let (record, _node) = record_of(&cross_reference(), &["urn:oid:2.999.7"]).await?;
    assert!(patients(&record).is_empty());
    assert_eq!(
        profile(&record),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Read")
    );
    ehr_says(&record, "not-found")
}

#[tokio::test]
async fn with_no_namespace_configured_the_record_says_so() -> TestResult {
    let (record, _node) = record_of(&cross_reference(), &[]).await?;
    assert!(patients(&record).is_empty());
    ehr_says(&record, "not-configured")
}

/// A PIX Manager that fails every ITI-83 with `500`.
async fn failing_manager() -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$ihe-pix"))
        .respond_with(
            ResponseTemplate::new(500).set_body_raw(b"{}".to_vec(), "application/fhir+json"),
        )
        .mount(&server)
        .await;
    server
}

/// The `[pixm]` binding over the Manager at `pix`.
fn pixm(pix: &Server) -> String {
    format!(
        "\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\
         \"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n",
        pix.uri()
    )
}

/// The identity service failing does not drop the access: the read is
/// answered and recorded, the record saying the patient is unavailable.
#[tokio::test]
async fn an_identity_service_that_fails_leaves_the_patient_unavailable() -> TestResult {
    let pix = failing_manager().await;
    let (record, _node) = record_of(&pixm(&pix), &[NAMESPACE]).await?;
    assert!(patients(&record).is_empty());
    ehr_says(&record, "unavailable")?;
    let asked = pix.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, asked.len(), "one ITI-83 for the ehr_id");
    Ok(())
}

/// §5.4, N33: neither the patient found nor a failure names the patient in
/// a log line.
#[test]
fn naming_the_patient_writes_no_identifier_to_the_log() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let (node, pix, repository) = runtime.block_on(async {
        (
            node().await,
            failing_manager().await,
            FeedRepository::start().await,
        )
    });
    let (dir, other) = (tempfile::tempdir()?, tempfile::tempdir()?);
    let found = gateway(
        dir.path(),
        &node,
        &repository,
        (&cross_reference(), &[NAMESPACE]),
    )?;
    let failing = gateway(
        other.path(),
        &node,
        &repository,
        (&pixm(&pix), &[NAMESPACE]),
    )?;
    let mut text = logged(&found, "trace", vec![read()?])?;
    text.push_str(&logged(&failing, "trace", vec![read()?])?);
    assert!(
        text.contains("the identity service could not name the patient"),
        "the failure was logged, so the check is not vacuous: {text}"
    );
    assert!(!text.contains(PATIENT), "{text}");
    Ok(())
}
