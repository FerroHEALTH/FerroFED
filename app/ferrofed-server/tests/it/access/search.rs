// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The searches the book's audit page documents, run with ITI-81 Retrieve
//! ATNA Audit Event against the records the gateway wrote to the harness
//! Audit Record Repository (IHE `RESTful` ATNA Rev. 3.6 §3.81; Regulation
//! (EU) 2025/327 Art 9(1), (2), Annex II 3.3): a person's records by
//! `patient.identifier`, the read the request addressed by `ehr_id` alone
//! included; an operator's by caller, `ehr_id`, endpoint and outcome; a
//! search with no `date` refused; and each search recorded as the `Audit Log
//! Used` event of §3.81.5.1.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use axum::body::Body;
use base64::Engine as _;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode};
use jiff::Timestamp;
use jiff::tz::Offset;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{LAB_REPORT, TestResult, accesses, composition, settings_resolving};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

const UID: &str = "71c0e5d2-3b4a-4f69-8e1d-2c3b4a5d6e7f::cdr-a.example.org::1";
const MISSING: &str = "0f9e8d7c-6b5a-4493-8271-605f4e3d2c1b::cdr-a.example.org::1";

/// The outcome system ITI-81 names an outcome search in.
const OUTCOMES: &str = "http://hl7.org/fhir/audit-event-outcome";

/// A node answering the patient query with one lab report, and at node A
/// the read of [`UID`] under [`EHR_A`], and `404` for [`MISSING`].
async fn node() -> Server {
    let server = Server::start().await;
    let json = |status: u16, text: String| {
        ResponseTemplate::new(status).set_body_raw(text.into_bytes(), "application/json")
    };
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(json(
            200,
            format!(
                r##"{{"q":"node","columns":[{{"name":"#0","path":"c"}}],"rows":[[{}]]}}"##,
                composition(LAB_REPORT, UID)
            ),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{UID}")))
        .respond_with(json(200, composition(LAB_REPORT, UID)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition/{MISSING}")))
        .respond_with(json(
            404,
            r#"{"message":"synthetic: no such composition"}"#.to_owned(),
        ))
        .mount(&server)
        .await;
    server
}

/// A read at node A of `uid`, by the `ehr_id` alone.
fn read(uid: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/ehr/{EHR_A}/composition/{uid}"))
        .header("openEHR-federation-endpoint", "node-a-pub")
        .body(Body::empty())
}

/// The patient query naming [`PATIENT`].
fn patient_query() -> Result<Request<Body>, Box<dyn Error>> {
    let aql = format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    Ok(post(body(&aql)?)?)
}

/// The `date` bounds of a search of the days around today, in UTC.
fn around_today() -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let today = Offset::UTC.to_datetime(Timestamp::now()).date();
    Ok(vec![
        ("date".to_owned(), format!("ge{}", today.yesterday()?)),
        ("date".to_owned(), format!("le{}", today.tomorrow()?)),
    ])
}

/// The status and the body of `GET [base]/AuditEvent` with `pairs`, each
/// percent-encoded as §3.81.4.1.2.2 asks.
async fn search(
    repository: &FeedRepository,
    pairs: &[(String, String)],
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let url = reqwest::Url::parse_with_params(&format!("{}AuditEvent", repository.base()), pairs)?;
    let answer = reqwest::Client::new().get(url).send().await?;
    let status = StatusCode::from_u16(answer.status().as_u16())?;
    let bytes = answer.bytes().await?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok((status, value))
}

/// The records of a `searchset` `Bundle`, each its `AuditEvent`.
fn found(bundle: &Value) -> Result<Vec<Value>, Box<dyn Error>> {
    if bundle.get("type").and_then(Value::as_str) != Some("searchset") {
        return Err(format!("a searchset Bundle, got {bundle}").into());
    }
    Ok(bundle
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource").cloned())
        .collect())
}

/// The `action` of each record of `records`, sorted.
fn actions(records: &[Value]) -> Vec<String> {
    let mut actions: Vec<String> = records
        .iter()
        .filter_map(|record| record.get("action").and_then(Value::as_str))
        .map(str::to_owned)
        .collect();
    actions.sort();
    actions
}

/// The searches around today with the parameters `extra` added.
fn with(extra: &[(&str, &str)]) -> Result<Vec<(String, String)>, Box<dyn Error>> {
    let mut pairs = around_today()?;
    pairs.extend(
        extra
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned())),
    );
    Ok(pairs)
}

/// The repository after one gateway wrote three records: a patient query, a
/// read by `ehr_id` whose patient the cross-reference names, and a read node
/// A answered `404`; with the nodes, which live as long as it is read.
async fn written() -> Result<(FeedRepository, [Server; 2]), Box<dyn Error>> {
    let (a, b) = (node().await, node().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = settings_resolving(
        dir.path(),
        (&a.uri(), &b.uri()),
        &repository,
        ("", ""),
        (
            &format!("\n[access_log]\npatient_namespaces = [\"{NAMESPACE}\"]\n"),
            &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
        ),
    )?;
    let app = ferrofed_server::router(Arc::new(AppState::build(&settings)?), &settings_with_room());
    let (status, text) = call(app.clone(), patient_query()?).await?;
    if status != StatusCode::OK {
        return Err(format!("the query answered {status}: {text}").into());
    }
    let (status, text) = call(app.clone(), read(UID)?).await?;
    if status != StatusCode::OK {
        return Err(format!("the read answered {status}: {text}").into());
    }
    let (status, _) = call(app, read(MISSING)?).await?;
    if status != StatusCode::NOT_FOUND {
        return Err(format!("the read of a missing composition answered {status}").into());
    }
    let records = accesses(&repository.wait_for(3, SETTLE).await)?;
    if records.len() != 3 {
        return Err(format!("three access records, got {}", records.len()).into());
    }
    Ok((repository, [a, b]))
}

/// The person's identifier, as a `patient.identifier` token.
fn the_patient() -> String {
    format!("{NAMESPACE}|{PATIENT}")
}

/// Art 9(1), (2): the person's search through the access service finds the
/// query and both reads by `ehr_id`, and no other person's search finds
/// any of them.
#[tokio::test]
async fn the_persons_search_finds_every_access_to_their_data() -> TestResult {
    let (repository, _nodes) = written().await?;
    let (status, bundle) = search(
        &repository,
        &with(&[("patient.identifier", &the_patient())])?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "the search is answered");
    assert_eq!(
        vec!["E", "R", "R"],
        actions(&found(&bundle)?),
        "the query and both reads"
    );
    assert_eq!(
        Some(3),
        bundle.get("total").and_then(Value::as_u64),
        "the total"
    );
    let other = format!("{NAMESPACE}|SENTINEL-OTHER-82qz");
    let (status, bundle) = search(&repository, &with(&[("patient.identifier", &other)])?).await?;
    assert_eq!(StatusCode::OK, status, "§3.81.4.2.2: no match is a 200");
    assert!(found(&bundle)?.is_empty(), "another person finds nothing");
    Ok(())
}

/// Annex II 3.3: an operator reviews the log by `ehr_id`, by endpoint, by
/// caller and by outcome.
#[tokio::test]
async fn an_operator_reviews_by_ehr_id_endpoint_caller_and_outcome() -> TestResult {
    let (repository, _nodes) = written().await?;
    let ehr_b = format!("|{EHR_B}");
    let (_, bundle) = search(&repository, &with(&[("entity.identifier", &ehr_b)])?).await?;
    assert_eq!(
        vec!["E"],
        actions(&found(&bundle)?),
        "only the query reached node B's EHR"
    );
    let ehr_a = format!("|{EHR_A}");
    let (_, bundle) = search(&repository, &with(&[("entity.identifier", &ehr_a)])?).await?;
    assert_eq!(
        vec!["E", "R", "R"],
        actions(&found(&bundle)?),
        "every access reached node A's EHR"
    );
    let (_, bundle) = search(&repository, &with(&[("entity.identifier", "|node-b-pub")])?).await?;
    assert_eq!(
        vec!["E"],
        actions(&found(&bundle)?),
        "only the query had node B as origin"
    );
    let claims = crate::support::claims();
    let caller = format!("{}|{}", claims.iss, claims.sub);
    let (_, bundle) = search(&repository, &with(&[("agent.identifier", &caller)])?).await?;
    assert_eq!(3, found(&bundle)?.len(), "the caller's accesses");
    let failed = format!("{OUTCOMES}|4,8,12");
    let (_, bundle) = search(&repository, &with(&[("outcome", &failed)])?).await?;
    let failures = found(&bundle)?;
    assert_eq!(vec!["R"], actions(&failures), "the read answered 404");
    assert_eq!(
        Some("4"),
        failures
            .first()
            .and_then(|record| record.get("outcome"))
            .and_then(Value::as_str),
        "a minor failure"
    );
    Ok(())
}

/// §3.81.4.1.2.1: a search names its period, which bounds what it finds,
/// and one with no `date` is refused.
#[tokio::test]
async fn the_period_bounds_a_search_and_is_required() -> TestResult {
    let (repository, _nodes) = written().await?;
    let past = vec![
        ("date".to_owned(), "ge1999-01-01".to_owned()),
        ("date".to_owned(), "le2000-01-01".to_owned()),
        ("patient.identifier".to_owned(), the_patient()),
    ];
    let (_, bundle) = search(&repository, &past).await?;
    assert!(
        found(&bundle)?.is_empty(),
        "nothing was recorded in that period"
    );
    let undated = vec![("patient.identifier".to_owned(), the_patient())];
    let (status, _) = search(&repository, &undated).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "a date is required");
    Ok(())
}

/// §3.81.5.1: every search of the log is recorded at the repository as an
/// `Audit Log Used` event naming the search.
#[tokio::test]
async fn every_search_of_the_log_is_recorded() -> TestResult {
    let (repository, _nodes) = written().await?;
    search(
        &repository,
        &with(&[("patient.identifier", &the_patient())])?,
    )
    .await?;
    search(&repository, &with(&[("entity.identifier", "|node-a-pub")])?).await?;
    let used = repository.log_used();
    assert_eq!(2, used.len(), "one record per search");
    let first: Value = serde_json::from_str(used.first().ok_or("one Audit Log Used")?)?;
    let at = |pointer: &str| first.pointer(pointer).and_then(Value::as_str);
    assert_eq!(Some("110101"), at("/type/code"), "Audit Log Used");
    assert_eq!(Some("ITI-81"), at("/subtype/0/code"), "ITI-81");
    assert_eq!(Some("R"), at("/action"), "a read");
    assert_eq!(
        Some("13"),
        at("/entity/0/role/code"),
        "the log, a security resource"
    );
    let query = at("/entity/0/query").ok_or("the search's query")?;
    let query = String::from_utf8(base64::engine::general_purpose::STANDARD.decode(query)?)?;
    assert!(
        query.contains("patient.identifier="),
        "the record names its search: {query}"
    );
    Ok(())
}
