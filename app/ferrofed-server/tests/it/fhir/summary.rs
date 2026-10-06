// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary on the FHIR face: an EPS document from both members,
//! built from the gateway's own section queries with no `[stored_queries]`,
//! no patient identifier reaching a node, one access record per request,
//! every silent member named, and the ITS-REST face unchanged.

use axum::body::Body;
use ferrofed_eehrxf::patient_summary::Section;
use ferrofed_testkit::eps;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::{
    FHIR, PUBLIC, TestResult, UID_A, UID_B, gateway, gateway_over, node_holding, resources,
    section, summary,
};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, node_failing, received, wire};
use crate::support::{call, exchange, field};

/// The LOINC code of the allergies section.
const ALLERGIES_SECTION: &str = "48765-2";

/// The LOINC code of the problems section.
const PROBLEMS_SECTION: &str = "11450-4";

// conformance: CP-26 track-10
#[tokio::test]
async fn the_summary_is_an_eps_document_from_both_members_and_no_identifier_reaches_a_node()
-> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, headers, body) = exchange(app, summary("")?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        field(&headers, header::CONTENT_TYPE.as_str()),
        Some("application/fhir+json")
    );
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "the EPS profiles: {findings:#?}");
    let document: Value = serde_json::from_str(&text)?;
    let allergies = section(&document, ALLERGIES_SECTION).ok_or("the allergies section")?;
    assert_eq!(
        allergies["entry"].as_array().map(Vec::len),
        Some(2),
        "one entry from each member, never merged: {allergies}"
    );
    assert_eq!(
        allergies["author"].as_array().map(Vec::len),
        Some(2),
        "the section names both members"
    );
    let problems = section(&document, PROBLEMS_SECTION).ok_or("the problems section")?;
    assert_eq!(
        problems.pointer("/emptyReason/coding/0/code"),
        Some(&Value::from("nilknown")),
        "every member answered, and none holds a problem"
    );
    assert_eq!(
        resources(&document, "Provenance").len(),
        2,
        "one Provenance per mapped composition"
    );
    let patient = resources(&document, "Patient");
    assert_eq!(
        patient
            .first()
            .and_then(|patient| patient.pointer("/identifier/0/value")),
        Some(&Value::from(PATIENT)),
        "the caller is told whose summary it is"
    );
    let full_url = document
        .pointer("/entry/1/fullUrl")
        .and_then(Value::as_str)
        .ok_or("a fullUrl")?;
    assert!(
        full_url.starts_with(&format!("{PUBLIC}{FHIR}/")),
        "entries are named under the face's base: {full_url}"
    );
    for (node, ehr_id) in [(&a, EHR_A), (&b, EHR_B)] {
        let captured = wire(node).await?;
        for withheld in [PATIENT, PATIENT_TAIL] {
            assert!(
                !captured.contains(withheld),
                "N33: no identifier in the query, path or headers: {captured}"
            );
        }
        let dispatched = received(node).await?;
        assert_eq!(
            Section::ALL.len(),
            dispatched.len(),
            "one request per section query"
        );
        for body in &dispatched {
            assert!(
                body.contains(ehr_id),
                "§7.1: scoped to the member's ehr_id: {body}"
            );
            assert!(
                !body.contains("external_ref"),
                "§7.1: the subject predicates are consumed: {body}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn the_face_needs_no_stored_query_registry() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let (app, state) = gateway_over(dir.path(), (&a.uri(), &b.uri()), &rows, ("", ""))?;
    assert!(state.definitions().is_none(), "no [stored_queries] is set");
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[cfg(feature = "binding-ihe")]
#[tokio::test]
async fn a_summary_is_recorded_once_with_the_patient_summary_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let tables = audit_tables(&repository, "");
    let (app, _) = gateway_over(dir.path(), (&a.uri(), &b.uri()), &rows, ("", &tables))?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(
        1,
        records.len(),
        "one access record per request: {records:#?}"
    );
    let record = records.first().ok_or("a record")?;
    assert!(
        record.contains("eehrxf-document-priority-category-cs|Patient-Summaries"),
        "Art 14(1)(a): the category of a summary: {record}"
    );
    assert!(
        record.contains("eehrxf-document-priority-category-cs|Patient-Summaries:construction"),
        "the request serves the category by construction: {record}"
    );
    for endpoint in ["node-a-pub", "node-b-pub"] {
        assert!(record.contains(endpoint), "the origin {endpoint}: {record}");
    }
    Ok(())
}

#[tokio::test]
async fn a_silent_member_fails_the_summary_and_is_named() -> TestResult {
    let a = node_holding(UID_A).await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, headers, body) = exchange(app, summary("")?).await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert_eq!(
        field(&headers, header::CONTENT_TYPE.as_str()),
        Some("application/fhir+json")
    );
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(outcome["resourceType"], "OperationOutcome");
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("incomplete"))
    );
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(diagnostics.contains("node-b-pub"), "{diagnostics}");
    assert!(!diagnostics.contains("node-a-pub"), "{diagnostics}");
    assert!(!text.contains(PATIENT_TAIL), "{text}");
    Ok(())
}

#[tokio::test]
async fn under_partial_a_silent_member_is_named_in_every_section() -> TestResult {
    let a = node_holding(UID_A).await;
    let b = node_failing(500).await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let (app, _) = gateway_over(
        dir.path(),
        (&a.uri(), &b.uri()),
        &rows,
        ("best_effort = true\n", ""),
    )?;
    let mut request = summary("")?;
    request
        .headers_mut()
        .insert("openehr-federation-completeness", "partial".parse()?);
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "§11.4: {text}");
    let findings = eps::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    let document: Value = serde_json::from_str(&text)?;
    let allergies = section(&document, ALLERGIES_SECTION).ok_or("the allergies section")?;
    assert_eq!(allergies["entry"].as_array().map(Vec::len), Some(1));
    let problems = section(&document, PROBLEMS_SECTION).ok_or("the problems section")?;
    assert_eq!(
        problems.pointer("/emptyReason/coding/0/code"),
        Some(&Value::from("unavailable")),
        "never nilknown while a member is silent"
    );
    let div = problems
        .pointer("/text/div")
        .and_then(Value::as_str)
        .ok_or("a narrative")?;
    assert!(div.contains("No answer from: org-b (node-error)"), "{div}");
    Ok(())
}

#[tokio::test]
async fn a_patient_no_member_holds_is_not_found_and_nothing_is_sent() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let elsewhere = format!(
        "\n[[dev.crossref]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"SYNTHETIC-OTHER-1\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n"
    );
    let (app, _) = gateway_over(dir.path(), (&a.uri(), &b.uri()), &[], ("", &elsewhere))?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("not-found"))
    );
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn with_no_cross_reference_the_summary_fails_closed_and_nothing_is_sent() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let (app, _) = gateway_over(dir.path(), (&a.uri(), &b.uri()), &[], ("", ""))?;
    let (status, text) = call(app, summary("")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert!(text.contains("could not be resolved"), "{text}");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn the_face_answers_its_capability_statement() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(
        app,
        Request::get(format!("{FHIR}/metadata")).body(Body::empty())?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let statement: Value = serde_json::from_str(&text)?;
    assert_eq!(statement["resourceType"], "CapabilityStatement");
    assert_eq!(statement["fhirVersion"], "4.0.1");
    assert_eq!(
        statement.pointer("/rest/0/resource/0/operation/0/definition"),
        Some(&Value::from(
            "http://hl7.org/fhir/uv/ips/OperationDefinition/summary"
        ))
    );
    Ok(())
}

// conformance: CP-21
#[tokio::test]
async fn an_its_rest_path_under_the_base_still_answers_as_it_did() -> TestResult {
    let (a, b) = (node_holding(UID_A).await, node_holding(UID_B).await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, headers, body) = exchange(
        app,
        Request::get("/v1/demographic/party/1").body(Body::empty())?,
    )
    .await?;
    let text = String::from_utf8(body)?;
    assert_eq!(StatusCode::NOT_IMPLEMENTED, status, "N32: {text}");
    assert_eq!(
        field(&headers, header::CONTENT_TYPE.as_str()),
        Some("application/json"),
        "the ITS-REST error, never an OperationOutcome"
    );
    assert!(text.contains("not-implemented"), "{text}");
    assert!(received(&a).await?.is_empty() && received(&b).await?.is_empty());
    Ok(())
}
