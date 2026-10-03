// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Declared values on the single-node route, against two mock nodes: a header
//! or query value the ITS-REST operation declares travels only when it
//! matches the kind the operation declares for it, and a mismatch is a `400`
//! that asks no node (§5.4.1, N33). Every assertion on what a node received
//! reads the node's own capture (§16, track 10).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};
use wiremock::MockServer;

use crate::facade::{EHR_A, PATIENT};
use crate::path_ehr_id::{ENDPOINT_A, answer, asked, holder, over, probe_at, stranger};
use crate::support::error_body;

type TestResult = Result<(), Box<dyn Error>>;

/// The endpoint header (§8.4).
const ENDPOINT: &str = "openEHR-federation-endpoint";

/// A version uid node A minted.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The query string of every request `server` received, in order.
async fn queries(server: &MockServer) -> Result<Vec<Option<String>>, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .into_iter()
        .map(|request| request.url.query().map(str::to_owned))
        .collect())
}

/// Asserts that `text` is a `400` body with the code
/// `parameter-value-invalid` that quotes no identifier.
fn refused(status: StatusCode, text: &str) -> TestResult {
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!("parameter-value-invalid", error_body(text)?.code, "{text}");
    assert!(!text.contains(PATIENT), "the 400 quotes nothing: {text}");
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_malformed_version_at_time_is_refused_and_no_node_is_asked() -> TestResult {
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    for targeted in [true, false] {
        let a = holder().await;
        let b = stranger().await;
        let dir = tempfile::tempdir()?;
        let mut request = Request::get(format!("{resource}?version_at_time={PATIENT}"));
        if targeted {
            request = request.header(ENDPOINT, ENDPOINT_A);
        }
        let (status, _, text) =
            answer(over(dir.path(), &a, &b)?, request.body(Body::empty())?).await?;
        refused(status, &text)?;
        assert!(text.contains("query parameter 1"), "{text}");
        for server in [&a, &b] {
            assert!(
                asked(server).await?.is_empty(),
                "targeted {targeted}: no node is asked, not even by the probe"
            );
        }
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_well_formed_version_at_time_reaches_the_node_byte_identical() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let query = "version_at_time=2015-01-20T19:30:22.765%2B01:00";
    let request = Request::get(format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}?{query}"))
        .header(ENDPOINT, ENDPOINT_A)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(vec![Some(query.to_owned())], queries(&a).await?);
    assert!(asked(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn an_enumerated_header_outside_its_values_is_refused() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let request = Request::post(format!("/v1/ehr/{EHR_A}/composition"))
        .header(ENDPOINT, ENDPOINT_A)
        .header("content-type", "application/json")
        .header("prefer", format!("return=minimal; patient={PATIENT}"))
        .body(Body::from("{}"))?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    refused(status, &text)?;
    assert!(text.contains("the Prefer header"), "{text}");
    for server in [&a, &b] {
        assert!(asked(server).await?.is_empty(), "nothing is sent");
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_free_text_parameter_passes_unclassified() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let query = format!("path={PATIENT}");
    let request = Request::get(format!("/v1/ehr/{EHR_A}/directory?{query}"))
        .header(ENDPOINT, ENDPOINT_A)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "node A's own answer: {text}");
    assert_eq!(
        vec![Some(query)],
        queries(&a).await?,
        "§5.4.1, N33: free text cannot be classified, so it travels as sent"
    );
    Ok(())
}

#[tokio::test]
async fn the_probe_leaves_out_a_value_its_own_operation_does_not_admit() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let flat = "application/openehr.wt.flat+json";
    let resource = format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}");
    let request = Request::get(&resource)
        .header("accept", flat)
        .body(Body::empty())?;
    let (status, _, text) = answer(over(dir.path(), &a, &b)?, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![probe_at(), ("GET".to_owned(), resource)],
        asked(&a).await?
    );
    let accepts: Vec<Option<String>> = a
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .map(|request| {
            request
                .headers
                .get("accept")
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .collect();
    let [probe, read] = accepts.as_slice() else {
        return Err(format!("two requests at node A, not {}", accepts.len()).into());
    };
    assert_ne!(
        Some(flat),
        probe.as_deref(),
        "the probe's operation lists no {flat}"
    );
    assert_eq!(
        Some(flat),
        read.as_deref(),
        "the read carries the client's Accept"
    );
    Ok(())
}
