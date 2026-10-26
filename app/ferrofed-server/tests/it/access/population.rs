// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of a query over many patients' data (Regulation (EU) 2025/327
//! Art 9(1)): when the delivered rows project the `ehr_id` each came from,
//! the record names each `ehr_id` at the origin that answered it and the
//! patient behind it, up to `[access_log] max_named_ehrs`; otherwise, and
//! above that bound, it states the query's scope in an `ehds-population`
//! entity beside the origins it asked. The query each node receives is the
//! one the client wrote (§7.1), and no identifier reaches a node or a log
//! line (§5.4, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    LAB_REPORT, REPORT, TestResult, accesses, composition, details, named, profile,
    settings_resolving,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::request_log::logged;
use crate::support::call;

/// The patient the cross-reference holds under [`EHR_B`] at node B, another
/// person than [`PATIENT`].
const OTHER: &str = "SENTINEL-OTHER-71mz";

const UID_A: &str = "3a1f6c2e-4b5d-4e7f-8a9b-0c1d2e3f4a5b::cdr-a.example.org::1";
const UID_B: &str = "9e8d7c6b-5a49-4382-9170-6f5e4d3c2b1a::cdr-b.example.org::1";

/// A query over every patient's lab reports that projects the `ehr_id` of
/// each row.
fn projecting() -> String {
    format!("SELECT e/ehr_id/value, c FROM EHR e CONTAINS COMPOSITION c[{REPORT}]")
}

/// The same query without the `ehr_id`.
fn not_projecting() -> String {
    format!("SELECT c FROM EHR e CONTAINS COMPOSITION c[{REPORT}]")
}

/// A node holding the EHR `holds`, answering every query with the rows
/// `rows`, each a JSON array.
async fn node(holds: &str, rows: &[String]) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{holds}")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(br#"{"message":"synthetic"}"#.to_vec(), "application/json"),
        )
        .mount(&server)
        .await;
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0"}},{{"name":"#1"}}],"rows":[{}]}}"##,
        rows.join(",")
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// Node A's row of the lab report under [`EHR_A`], and node B's under
/// [`EHR_B`], projected as `[ehr_id, composition]` when `projected`.
fn rows(projected: bool) -> (Vec<String>, Vec<String>) {
    let row = |ehr_id: &str, uid: &str| {
        let composition = composition(LAB_REPORT, uid);
        if projected {
            format!(r#"["{ehr_id}",{composition}]"#)
        } else {
            format!("[{composition}]")
        }
    };
    (vec![row(EHR_A, UID_A)], vec![row(EHR_B, UID_B)])
}

/// The `[access_log]` keys: the patients named in [`NAMESPACE`], at most
/// `max` `ehr_id`s named.
fn keys(max: usize) -> String {
    format!("\n[access_log]\npatient_namespaces = [\"{NAMESPACE}\"]\nmax_named_ehrs = {max}\n")
}

/// The development cross-reference: [`PATIENT`] under [`EHR_A`] at node A,
/// [`OTHER`] under [`EHR_B`] at node B.
fn cross_reference() -> String {
    let row = |value: &str, member: &str, ehr_id: &str| {
        format!(
            "\n[[testkit.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{value}\"\n\
             member = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        )
    };
    format!(
        "{}{}",
        row(PATIENT, "node-a", EHR_A),
        row(OTHER, "node-b", EHR_B)
    )
}

/// A gateway over `nodes`, naming at most `max` `ehr_id`s, recording to
/// `repository`.
fn gateway(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    repository: &FeedRepository,
    max: usize,
) -> Result<Router, Box<dyn Error>> {
    let settings = settings_resolving(
        dir,
        (&a.uri(), &b.uri()),
        repository,
        ("", ""),
        (&keys(max), &cross_reference()),
    )?;
    let state = Arc::new(AppState::build(&settings)?);
    Ok(ferrofed_server::router(state, &settings_with_room()))
}

/// The one access record of `aql` over nodes answering the rows `projected`
/// selects, naming at most `max` `ehr_id`s, with the two nodes.
async fn record_of(
    aql: &str,
    projected: bool,
    max: usize,
) -> Result<(Value, (Server, Server)), Box<dyn Error>> {
    let (a, b) = rows(projected);
    let nodes = (node(EHR_A, &a).await, node(EHR_B, &b).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&nodes.0, &nodes.1), &repository, max)?;
    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "the query is answered: {text}");
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the query, got {}", records.len()).into());
    };
    Ok((record.clone(), nodes))
}

/// The identifier values of the `entity:patient`s of `record`.
fn patients(record: &Value) -> Vec<String> {
    let mut found: Vec<String> = record["entity"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|entity| {
            entity.pointer("/role/code").and_then(Value::as_str) == Some("1")
                && entity.pointer("/type/code").and_then(Value::as_str) == Some("1")
        })
        .filter_map(|entity| entity.pointer("/what/identifier/value")?.as_str())
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// The `(ehr_id, endpoint)` of each `ehr` entity of `record`.
fn ehrs(record: &Value) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = named(record, "ehr")
        .iter()
        .map(|ehr| {
            let endpoint = ehr["detail"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|detail| detail["type"] == "endpoint")
                .and_then(|detail| detail["valueString"].as_str())
                .unwrap_or_default();
            (
                ehr.pointer("/what/identifier/value")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                endpoint.to_owned(),
            )
        })
        .collect();
    found.sort();
    found
}

/// Every query the node `server` received, as text.
async fn queries(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    Ok(requests
        .iter()
        .map(|request| String::from_utf8_lossy(&request.body).into_owned())
        .collect())
}

/// Art 9(1): each `ehr_id` a delivered row names is recorded at the origin
/// that answered it, with the patient the identity binding holds under it,
/// so a `patient.identifier` search by either person finds the access.
#[tokio::test]
async fn rows_that_project_the_ehr_id_name_each_ehr_and_its_patient() -> TestResult {
    let (record, _nodes) = record_of(&projecting(), true, 50).await?;
    assert_eq!(
        vec![
            (EHR_B.to_owned(), "node-b-pub".to_owned()),
            (EHR_A.to_owned(), "node-a-pub".to_owned()),
        ],
        ehrs(&record)
    );
    assert_eq!(
        vec![OTHER.to_owned(), PATIENT.to_owned()],
        patients(&record)
    );
    assert_eq!(
        vec!["found", "found"],
        details(&record, "ehr", "patient-lookup")
    );
    assert!(named(&record, "ehds-population").is_empty(), "{record}");
    assert_eq!(
        profile(&record),
        Some("https://profiles.ihe.net/ITI/BALP/StructureDefinition/IHE.BasicAudit.Query"),
        "a record naming two patients claims the plain pattern"
    );
    Ok(())
}

/// Above `max_named_ehrs`, the record names no `ehr_id` and no patient and
/// states the query's scope, saying how many `ehr_id`s the rows named.
#[tokio::test]
async fn more_ehrs_than_the_bound_record_the_scope_instead() -> TestResult {
    let (record, _nodes) = record_of(&projecting(), true, 1).await?;
    assert!(ehrs(&record).is_empty(), "{record}");
    assert!(patients(&record).is_empty(), "{record}");
    assert_eq!(
        vec!["over-limit"],
        details(&record, "ehds-population", "ehds-population-reason")
    );
    assert_eq!(
        vec!["2"],
        details(&record, "ehds-population", "ehds-population-ehrs")
    );
    assert_eq!(
        vec!["1"],
        details(&record, "ehds-population", "ehds-population-limit")
    );
    Ok(())
}

/// A query whose rows do not say whose record each came from records its
/// scope, tagged a population access: the members it asked, by node and
/// `system_id`, and the archetypes it reads.
#[tokio::test]
async fn a_query_that_projects_no_ehr_id_records_its_scope() -> TestResult {
    let (record, _nodes) = record_of(&not_projecting(), false, 50).await?;
    assert!(ehrs(&record).is_empty(), "{record}");
    assert!(patients(&record).is_empty(), "{record}");
    let marks = named(&record, "ehds-population");
    let [mark] = marks.as_slice() else {
        return Err(format!("one population entity: {record}").into());
    };
    assert_eq!(
        Some(ehds_logging::balp::POPULATION_DESCRIPTION),
        mark["description"].as_str()
    );
    assert_eq!(
        vec!["true"],
        details(&record, "ehds-population", "ehds-population")
    );
    assert_eq!(
        vec!["not-projected"],
        details(&record, "ehds-population", "ehds-population-reason")
    );
    assert_eq!(
        vec![REPORT],
        details(&record, "ehds-population", "archetype-id")
    );
    let origins: Vec<&str> = named(&record, "origin")
        .iter()
        .filter_map(|origin| origin.pointer("/what/identifier/value")?.as_str())
        .collect();
    assert_eq!(vec!["node-a-pub", "node-b-pub"], origins);
    assert_eq!(vec!["node-a", "node-b"], details(&record, "origin", "node"));
    Ok(())
}

/// A query scoped to one `ehr_id` is recorded as before: that `ehr_id` and
/// its patient, and no population entity.
#[tokio::test]
async fn an_ehr_id_scoped_query_records_its_ehr_as_before() -> TestResult {
    let aql = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c[{REPORT}] WHERE e/ehr_id/value = '{EHR_A}'"
    );
    let (record, _nodes) = record_of(&aql, false, 50).await?;
    assert!(named(&record, "ehds-population").is_empty(), "{record}");
    assert_eq!(
        vec![(EHR_A.to_owned(), "node-a-pub".to_owned())],
        ehrs(&record)
    );
    assert_eq!(vec![PATIENT.to_owned()], patients(&record));
    Ok(())
}

/// §7.1, §5.4, N33: each node receives the query the client wrote, with no
/// column added for the record, and no patient identifier the record names.
#[tokio::test]
async fn each_node_is_asked_the_query_as_written() -> TestResult {
    let (record, (a, b)) = record_of(&projecting(), true, 50).await?;
    assert_eq!(2, patients(&record).len(), "the record named both patients");
    for node in [&a, &b] {
        let asked = queries(node).await?;
        assert_eq!(1, asked.len(), "one query per node: {asked:?}");
        for text in &asked {
            assert!(text.contains("e/ehr_id/value, c"), "{text}");
            assert!(!text.contains(PATIENT), "{text}");
            assert!(!text.contains(OTHER), "{text}");
        }
    }
    let (_record, (a, _b)) = record_of(&not_projecting(), false, 50).await?;
    for text in queries(&a).await? {
        assert!(
            !text.contains("ehr_id"),
            "no ehr_id is added to a query that does not project it: {text}"
        );
    }
    Ok(())
}

/// §5.4, N33: naming the patients of a population query, and recording its
/// scope, writes no patient identifier and no `ehr_id` to a log line.
#[test]
fn a_population_record_writes_no_identifier_to_the_log() -> TestResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let ((a, b), repository) = runtime.block_on(async {
        let (a, b) = rows(true);
        (
            (node(EHR_A, &a).await, node(EHR_B, &b).await),
            FeedRepository::start().await,
        )
    });
    let (named_dir, bound_dir) = (tempfile::tempdir()?, tempfile::tempdir()?);
    let naming = gateway(named_dir.path(), (&a, &b), &repository, 50)?;
    let bounded = gateway(bound_dir.path(), (&a, &b), &repository, 0)?;
    let mut text = logged(&naming, "trace", vec![post(body(&projecting())?)?])?;
    text.push_str(&logged(
        &bounded,
        "trace",
        vec![post(body(&projecting())?)?],
    )?);
    assert!(
        text.contains("/v1/query/aql"),
        "the queries were logged: {text}"
    );
    for value in [PATIENT, OTHER, EHR_A, EHR_B] {
        assert!(!text.contains(value), "{value} in the log: {text}");
    }
    Ok(())
}
