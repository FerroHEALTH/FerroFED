// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The origins of a federated query's record (Annex II 3.2(e)): an endpoint
//! the query never left the gateway for, such as one past its in-flight cap,
//! is no origin, a query that left for none is not recorded, and each origin
//! of a merged answer carries the categories of its own rows (Annex II
//! 3.2(c)).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::COMPLETENESS;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{
    DISCHARGE, DISCHARGE_CATEGORY, LAB_CATEGORY, LAB_REPORT, TestResult, accesses, composition,
    gateway, gateway_with, named, node_with_rows,
};
use crate::facade::{EHR_B, NAMESPACE, PATIENT, body, post, settings_with_room};
use crate::feed_audit::SETTLE;
use crate::support::call;

const UID_A: &str = "0c1d2e3f-4a5b-4c6d-8e7f-9a0b1c2d3e4f::cdr-a.example.org::1";
const UID_B: &str = "1d2e3f4a-5b6c-4d7e-8f9a-0b1c2d3e4f5a::cdr-b.example.org::1";

/// How long a slow member keeps the query that holds its one in-flight slot.
const HOLD: Duration = Duration::from_millis(2_500);

/// The patient query selecting whole compositions.
fn compositions() -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// A member answering the query with one composition of `template` after
/// `delay`.
async fn slow_node(template: &str, uid: &str, delay: Duration) -> Server {
    let server = Server::start().await;
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c"}}],"rows":[[{}]]}}"##,
        composition(template, uid)
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(answer.into_bytes(), "application/json")
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// Waits until `server` received a request, or fails after a bound.
async fn until_received(server: &Server) -> TestResult {
    for _ in 0..500 {
        if !server
            .received_requests()
            .await
            .unwrap_or_default()
            .is_empty()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err("the member never received the query".into())
}

/// The patient query, best effort, with a budget of one second, which ends
/// its wait for a slot held by another request (§11.4, §11.5).
fn shortened() -> Result<Request<axum::body::Body>, Box<dyn Error>> {
    let mut request = post(body(&compositions())?)?;
    let headers = request.headers_mut();
    headers.insert(COMPLETENESS, "partial".parse()?);
    headers.insert(header::HeaderName::from_static("prefer"), "wait=1".parse()?);
    Ok(request)
}

/// The endpoint ids of the origins of `record`.
fn origins(record: &Value) -> Vec<&str> {
    named(record, "origin")
        .iter()
        .filter_map(|origin| {
            origin
                .pointer("/what/identifier/value")
                .and_then(Value::as_str)
        })
        .collect()
}

/// The `kind` details of the origin `endpoint` of `record`.
fn origin_details(record: &Value, endpoint: &str, kind: &str) -> Vec<String> {
    named(record, "origin")
        .iter()
        .filter(|origin| origin.pointer("/what/identifier/value") == Some(&Value::from(endpoint)))
        .flat_map(|origin| {
            origin
                .get("detail")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|detail| detail["type"] == kind)
                .filter_map(|detail| detail["valueString"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .collect()
}

#[tokio::test]
async fn a_query_every_member_of_which_was_capped_writes_no_record() -> TestResult {
    let (node_a, node_b) = (
        slow_node(LAB_REPORT, UID_A, HOLD).await,
        slow_node(DISCHARGE, UID_B, HOLD).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", "max_in_flight_per_node = 1\n"),
        &settings_with_room(),
    )?;
    let held = tokio::spawn({
        let (app, request) = (app.clone(), post(body(&compositions())?)?);
        async move { call(app, request).await.map_err(|error| error.to_string()) }
    });
    until_received(&node_a).await?;
    until_received(&node_b).await?;

    let (status, text) = call(app, shortened()?).await?;
    assert!(
        text.contains("in-flight cap"),
        "both members are time-out for their cap: {status} {text}"
    );
    assert_eq!(
        1,
        node_a.received_requests().await.unwrap_or_default().len()
    );
    assert_eq!(
        1,
        node_b.received_requests().await.unwrap_or_default().len()
    );
    let (status, text) = held.await??;
    assert_eq!(StatusCode::OK, status, "{text}");

    repository.wait_for(1, SETTLE).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let records = accesses(&repository.records())?;
    let [record] = records.as_slice() else {
        return Err(format!(
            "Annex II 3.2(e): only the query that reached a member is recorded, got {records:?}"
        )
        .into());
    };
    assert_eq!(vec!["node-a-pub", "node-b-pub"], origins(record));
    Ok(())
}

#[tokio::test]
async fn a_capped_member_is_no_origin_of_the_answer() -> TestResult {
    let node_a = slow_node(LAB_REPORT, UID_A, HOLD).await;
    let node_b = node_with_rows(&[composition(DISCHARGE, UID_B)]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        ("", "max_in_flight_per_node = 1\n"),
        &settings_with_room(),
    )?;
    let held = tokio::spawn({
        let (app, request) = (app.clone(), post(body(&compositions())?)?);
        async move { call(app, request).await.map_err(|error| error.to_string()) }
    });
    until_received(&node_a).await?;

    let (status, text) = call(app, shortened()?).await?;
    assert_eq!(StatusCode::OK, status, "a partial answer: {text}");
    assert!(text.contains("in-flight cap"), "member A is capped: {text}");
    let (status, text) = held.await??;
    assert_eq!(StatusCode::OK, status, "{text}");

    let records = accesses(&repository.wait_for(2, SETTLE).await)?;
    assert_eq!(2, records.len(), "one record per query: {records:?}");
    let capped: Vec<&Value> = records
        .iter()
        .filter(|record| origins(record).len() == 1)
        .collect();
    let [record] = capped.as_slice() else {
        return Err(format!("one record names a single origin, got {records:?}").into());
    };
    assert_eq!(
        vec!["node-b-pub"],
        origins(record),
        "Annex II 3.2(e): member A never sent data to the capped query"
    );
    assert_eq!(
        vec![DISCHARGE_CATEGORY],
        origin_details(record, "node-b-pub", "ehds-category")
    );
    let ehrs: Vec<&str> = named(record, "ehr")
        .iter()
        .filter_map(|ehr| {
            ehr.pointer("/what/identifier/value")
                .and_then(Value::as_str)
        })
        .collect();
    assert_eq!(
        vec![EHR_B],
        ehrs,
        "the EHR at member A was never reached by the capped query"
    );
    Ok(())
}

#[tokio::test]
async fn each_origin_of_a_merged_answer_carries_the_categories_of_its_own_rows() -> TestResult {
    let (node_a, node_b) = (
        node_with_rows(&[composition(LAB_REPORT, UID_A)]).await,
        node_with_rows(&[composition(DISCHARGE, UID_B)]).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&node_a.uri(), &node_b.uri()), &repository, "")?;
    let (status, text) = call(app, post(body(&compositions())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Value = serde_json::from_str(&text)?;
    assert_eq!(
        Some(1),
        answer["columns"].as_array().map(Vec::len),
        "A57: no column is added to tell the origins apart: {text}"
    );
    let rows = answer["rows"].as_array().ok_or("rows")?;
    assert_eq!(2, rows.len(), "both members' rows are merged: {text}");
    assert!(
        rows.iter()
            .all(|row| row.as_array().is_some_and(|cells| cells.len() == 1)),
        "{text}"
    );
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record, got {records:?}").into());
    };
    assert_eq!(
        vec![LAB_CATEGORY],
        origin_details(record, "node-a-pub", "ehds-category"),
        "Annex II 3.2(c), (e): node A delivered lab reports"
    );
    assert_eq!(
        vec![format!("{LAB_CATEGORY}:returned")],
        origin_details(record, "node-a-pub", "ehds-category-basis")
    );
    assert_eq!(
        vec![DISCHARGE_CATEGORY],
        origin_details(record, "node-b-pub", "ehds-category"),
        "node B delivered discharge reports"
    );
    Ok(())
}
