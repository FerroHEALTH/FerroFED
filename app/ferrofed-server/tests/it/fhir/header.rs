// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary header from the identity binding (eHN PS A.1.1,
//! A.1.2): the name and birth date the harness PDQm Supplier holds are the
//! document's `Patient`, no header value and no patient identifier reaches a
//! node (§5.4.1, N33), and a patient the binding cannot name is refused
//! before any member is asked.

use axum::Router;
use ferrofed_testkit::eps;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdq::PdqSupplier;
use http::StatusCode;
use serde_json::Value;

use super::{
    BIRTH_DATE, FAMILY, GIVEN, TestResult, UID_A, UID_B, UNREACHED_SUPPLIER, gateway, gateway_over,
    node_holding, resources, summary, supplier,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, wire};
use crate::support::call;

/// The gateway over node A and node B, the patient known at both, its
/// header asked of the Supplier at `pdq`.
fn gateway_asking(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    pdq: &str,
) -> Result<Router, Box<dyn std::error::Error>> {
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    Ok(gateway_over(dir, (&a.uri(), &b.uri(), pdq), &rows, ("", ""))?.0)
}

/// The `OperationOutcome` issue code of `text`.
fn issue(text: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    Ok(outcome
        .pointer("/issue/0/code")
        .cloned()
        .unwrap_or(Value::Null))
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_header_is_the_supplier_s_and_no_header_value_reaches_a_node() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "the EPS profiles: {findings:#?}");
    let document: Value = serde_json::from_str(&text)?;
    let patient = *resources(&document, "Patient").first().ok_or("a Patient")?;
    assert_eq!(
        patient["name"],
        serde_json::json!([{ "family": FAMILY, "given": [GIVEN] }]),
        "eHN PS A.1.1.2, A.1.1.3: the name the binding holds"
    );
    assert_eq!(patient["birthDate"], BIRTH_DATE, "eHN PS A.1.1.4");
    assert_eq!(1, pdq.searches(), "the Supplier is asked once");
    for node in [&a, &b] {
        let captured = wire(node).await?;
        for withheld in [PATIENT, PATIENT_TAIL, FAMILY, GIVEN] {
            assert!(
                !captured.contains(withheld),
                "N33: nothing of the header or the identifier in the query, path or headers: {captured}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_patient_the_binding_does_not_know_is_not_found_and_nothing_is_sent() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = PdqSupplier::start().await?;
    pdq.add(&[(NAMESPACE, "SENTINEL-SOMEONE-ELSE")], true)?;
    let app = gateway_asking(dir.path(), (&a, &b), &pdq.base_url())?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!(issue(&text)?, "not-found");
    assert!(
        text.contains("no member holds a patient summary for this identifier"),
        "Federation Tier §11.3: answered as a patient no member holds: {text}"
    );
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_patient_the_binding_holds_no_name_for_writes_no_document() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = PdqSupplier::start().await?;
    pdq.add(&[(NAMESPACE, PATIENT)], true)?;
    let app = gateway_asking(dir.path(), (&a, &b), &pdq.base_url())?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!(issue(&text)?, "required");
    assert!(text.contains("ips-pat-1"), "{text}");
    assert!(
        received(&a).await?.is_empty() && received(&b).await?.is_empty(),
        "no member is asked for a summary that could name no patient"
    );
    Ok(())
}

#[tokio::test]
async fn several_patients_under_the_identifier_write_no_document() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await?;
    let second = pdq.add(&[(NAMESPACE, PATIENT)], true)?;
    pdq.describe(&second, ("SENTINEL-FAMILY-2", "SENTINEL-GIVEN-2"), None)?;
    let app = gateway_asking(dir.path(), (&a, &b), &pdq.base_url())?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert_eq!(issue(&text)?, "multiple-matches");
    for name in [FAMILY, "SENTINEL-FAMILY-2"] {
        assert!(!text.contains(name), "never one of them picked: {text}");
    }
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn a_binding_that_does_not_answer_fails_the_summary_and_nothing_is_sent() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let app = gateway_asking(dir.path(), (&a, &b), UNREACHED_SUPPLIER)?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(
        StatusCode::BAD_GATEWAY,
        status,
        "an upstream failure is never a document with a header left out: {text}"
    );
    assert_eq!(issue(&text)?, "exception");
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}
