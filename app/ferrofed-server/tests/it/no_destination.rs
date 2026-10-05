// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A request that node selection resolves to no destination: every registry
//! member `excluded`, so no node is asked and the answer is a `404`, kept
//! apart from the `200` of a patient who is `not-resolved` at every member in
//! scope (§11.1 "What in scope means", §11.2 first row, §11.3).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::mock::Server;
use http::StatusCode;

use crate::facade::{
    Answer, EHR_A, body, crossref, gateway, node_answering, patient_query, post, received,
    registry, schema, statuses,
};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// The registry document of node A and node B with both endpoints suspended
/// by the operator.
fn suspended(a: &Server, b: &Server) -> String {
    registry(&a.uri(), &b.uri(), "").replace(
        "connection_type = \"openehr-rest-query\"\n",
        "connection_type = \"openehr-rest-query\"\nstatus = \"suspended\"\n",
    )
}

/// Asserts that `text` is the ITS-REST error body of a request with no
/// destination, which carries no `RESULT_SET`.
fn assert_no_destination(status: StatusCode, text: &str) -> TestResult {
    assert_eq!(StatusCode::NOT_FOUND, status, "§11.2, first row: {text}");
    let error = error_body(text)?;
    assert_eq!(
        "no-destination", error.code,
        "the stable code of §11.2's first row"
    );
    assert!(
        error
            .message
            .contains("cannot be resolved to any destination"),
        "the message says why: {}",
        error.message
    );
    Ok(())
}

#[tokio::test]
async fn a_patient_query_with_every_member_excluded_is_a_404_and_asks_no_node() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &suspended(&a, &b),
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_no_destination(status, &text)?;
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "a request with no destination is never dispatched"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_query_naming_no_patient_with_every_member_excluded_is_a_404() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &suspended(&a, &b),
        "profile = \"development\"",
        "",
    )?;

    let unscoped = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";
    let (status, text) = call(app, post(body(unscoped)?)?).await?;
    assert_no_destination(status, &text)?;
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}

#[tokio::test]
async fn one_member_in_scope_where_the_patient_is_not_resolved_answers_200() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let document = registry(&a.uri(), &b.uri(), "").replacen(
        "connection_type = \"openehr-rest-query\"\n",
        "connection_type = \"openehr-rest-query\"\nstatus = \"suspended\"\n",
        1,
    );
    let app = gateway(
        dir.path(),
        &document,
        "profile = \"development\"",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "§11.3: found nowhere is not a 404: {text}"
    );
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "excluded"), ("node-b-pub", "not-resolved")],
        statuses(&answer)
    );
    assert!(answer.rows.is_empty());
    assert!(
        !answer.meta.federation.complete,
        "not-resolved is in scope and clears complete (§11.3)"
    );
    for server in [&a, &b] {
        assert!(
            received(server).await?.is_empty(),
            "no node knows the patient"
        );
    }
    Ok(())
}
