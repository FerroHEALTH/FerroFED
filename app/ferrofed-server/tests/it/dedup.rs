// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Version-identity dedup through the façade (§10, N15, CP-9): the
//! imported-composition scenario returns both copies without the
//! `openEHR-federation-dedup` header and one under `version-identity`, with
//! the suppressed endpoint in `meta.federation.dedup`; the mode is recorded
//! on every answer; a header value naming no offered mode is a `400` before
//! any node is asked; and an aggregate recombined across nodes is refused
//! under the mode (§11.6.3). Every body is validated against the result-set
//! schema, and no node receives the patient identifier (§5.4.1, N33).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode, header};
use openehr_federation::headers::DEDUP;
use serde::Deserialize;

use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, dev_gateway, node_answering, node_failing,
    patient_query, received, schema, wire,
};
use crate::support::{call, error_body};

type TestResult = Result<(), Box<dyn Error>>;

/// The version node B created and node A holds as an import, keeping its uid
/// (§10.3, the scenario); `cdr-b.example.org` is node B's `system_id`.
const IMPORTED: &str = "8849a2f0-1d3c-4e5f-9a7b-000000000001::cdr-b.example.org::1";

/// `POST /v1/query/aql` with `aql` and the dedup header set to each of
/// `values`.
pub(crate) fn request(aql: &str, values: &[&str]) -> Result<Request<Body>, Box<dyn Error>> {
    let mut builder =
        Request::post("/v1/query/aql").header(header::CONTENT_TYPE, "application/json");
    for value in values {
        builder = builder.header(DEDUP, *value);
    }
    Ok(builder.body(Body::from(body(aql)?))?)
}

#[derive(Debug, Deserialize)]
pub(crate) struct Answer {
    pub(crate) rows: Vec<Vec<String>>,
    pub(crate) meta: Meta,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Meta {
    pub(crate) federation: Federation,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Federation {
    pub(crate) dedup: Dedup,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Dedup {
    pub(crate) mode: String,
    pub(crate) suppressed_rows: Option<u64>,
    pub(crate) suppressed_endpoints: Option<Vec<String>>,
}

/// The answer to `request` from a gateway over two nodes that both hold the
/// imported composition, validated against the schema.
async fn imported(values: &[&str]) -> Result<(StatusCode, Answer), Box<dyn Error>> {
    let a = node_answering(IMPORTED).await;
    let b = node_answering(IMPORTED).await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let (status, text) = call(app, request(&patient_query(), values)?).await?;
    schema::validate(&text)?;
    for server in [&a, &b] {
        assert!(
            !wire(server).await?.contains(PATIENT),
            "§5.4.1, N33: no node receives the patient identifier"
        );
    }
    Ok((status, serde_json::from_str(&text)?))
}

// conformance: CP-9
#[tokio::test]
async fn without_the_header_both_copies_come_back_and_none_is_recorded() -> TestResult {
    let (status, answer) = imported(&[]).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(answer.rows.len(), 2, "§10.1, N15: pass-through by default");
    assert_eq!(
        answer.meta.federation.dedup,
        Dedup {
            mode: "none".to_owned(),
            suppressed_rows: None,
            suppressed_endpoints: None,
        },
        "§10.2: the mode applied is recorded"
    );
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn none_is_accepted_explicitly() -> TestResult {
    let (status, answer) = imported(&["none"]).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(answer.rows.len(), 2);
    assert_eq!(answer.meta.federation.dedup.mode, "none");
    Ok(())
}

/// Covers the visibility half of CP-29 (§10.2, §10.3): the suppressed
/// copies stay visible in `meta.federation.dedup`; its write-routing half is
/// in [`crate::dedup_write`].
// conformance: CP-9 CP-29
#[tokio::test]
async fn under_version_identity_one_row_comes_back_and_the_copy_is_named() -> TestResult {
    let (status, answer) = imported(&["version-identity"]).await?;
    assert_eq!(StatusCode::OK, status);
    assert_eq!(
        answer.rows,
        [vec![PATIENT.to_owned(), IMPORTED.to_owned()]],
        "§10.2: one row for the version"
    );
    assert_eq!(
        answer.meta.federation.dedup,
        Dedup {
            mode: "version-identity".to_owned(),
            suppressed_rows: Some(1),
            suppressed_endpoints: Some(vec!["node-a-pub".to_owned()]),
        },
        "§10.3, N36: node B created the version, so node A's copy is the one dropped"
    );
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn a_failing_answer_still_records_the_mode() -> TestResult {
    let a = node_answering(IMPORTED).await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let (status, text) = call(app, request(&patient_query(), &["version-identity"])?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.4: {text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert!(answer.rows.is_empty());
    assert_eq!(answer.meta.federation.dedup.mode, "version-identity");
    Ok(())
}

// conformance: CP-9
#[tokio::test]
async fn a_value_naming_no_offered_mode_is_refused_before_any_node_is_asked() -> TestResult {
    for values in [
        vec!["content-hash"],
        vec!["Version-Identity"],
        vec!["none", "version-identity"],
    ] {
        let a = node_answering(IMPORTED).await;
        let b = node_answering(IMPORTED).await;
        let dir = tempfile::tempdir()?;
        let app = dev_gateway(
            dir.path(),
            &a.uri(),
            &b.uri(),
            &[("node-a", EHR_A), ("node-b", EHR_B)],
        )?;
        let (status, text) = call(app, request(&patient_query(), &values)?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{values:?}: {text}");
        assert_eq!("dedup-invalid", error_body(&text)?.code, "{values:?}");
        assert!(
            !text.contains("content-hash") && !text.contains("Version-Identity"),
            "§5.4.3: a refusal never quotes a header value: {text}"
        );
        for server in [&a, &b] {
            assert!(received(server).await?.is_empty(), "no node is asked");
        }
    }
    Ok(())
}

// conformance: CP-9 CP-10
#[tokio::test]
async fn a_recombined_aggregate_under_version_identity_is_refused() -> TestResult {
    let a = node_answering(IMPORTED).await;
    let b = node_answering(IMPORTED).await;
    let dir = tempfile::tempdir()?;
    let app = dev_gateway(
        dir.path(),
        &a.uri(),
        &b.uri(),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
    )?;
    let aql = format!(
        "SELECT COUNT(*) FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, text) = call(app, request(&aql, &["version-identity"])?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert_eq!(
        "indecomposable-aggregate",
        error_body(&text)?.code,
        "§11.6.3: a decomposable aggregate must not be combined with de-duplication"
    );
    assert!(!text.contains(PATIENT), "§5.4.3: {text}");
    for server in [&a, &b] {
        assert!(received(server).await?.is_empty(), "no node is asked");
    }
    Ok(())
}
