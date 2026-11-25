// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The document list by the patient's stable id (#1976; MyHealth@EU
//! national contact point API 9.1.1-ci `patient`): every entry the gateway
//! writes names the patient by the id the lookup answers, the same in every
//! answer; with FerroFED's store, `patient=Patient/<id>` lists the patient's
//! documents, and an id the store does not hold, never recorded or past its
//! window, answers the empty `searchset` with an `outcome` entry asking for
//! the lookup again (FHIR R4 search §3.1.1.7); without a store the search is
//! refused and `metadata` declares no `patient`.

use axum::body::Body;
use ferrofed_testkit::containers::{self, database};
use http::{Request, StatusCode};
use serde_json::Value;

use super::super::{FHIR, TestResult};
use super::{DOCUMENT_A, DOCUMENT_B, Members, entries, fhir_document, list};
use crate::e2e::store::{executed, store_toml};
use crate::facade::PATIENT_TAIL;
use crate::support::call;

/// The patient summary document type, percent-encoded.
const SUMMARY_TYPE: &str = "&type=http%3A%2F%2Floinc.org%7C60591-5";

/// `GET {fhir-base}/DocumentReference` with the query `query`.
fn search(query: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/DocumentReference?{query}")).body(Body::empty())
}

/// The id every entry the gateway writes in `bundle` names its patient by.
fn patient_id(bundle: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let written = entries(bundle, "match")
        .into_iter()
        .find(|reference| {
            reference.pointer("/custodian/display") == Some(&Value::from("Synthetic Operator"))
        })
        .ok_or("the summary written on request")?;
    let reference = written
        .pointer("/subject/reference")
        .and_then(Value::as_str)
        .ok_or("a literal subject reference")?;
    Ok(reference
        .strip_prefix("Patient/")
        .ok_or("a Patient reference")?
        .to_owned())
}

/// The `searchset` of the list `request` asks for, answered `200`.
async fn listed(
    app: &axum::Router,
    request: Request<Body>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let (status, text) = call(app.clone(), request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// The search parameter names `metadata` declares on `DocumentReference`.
async fn declared(app: &axum::Router) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let (_, text) = call(
        app.clone(),
        Request::get(format!("{FHIR}/metadata")).body(Body::empty())?,
    )
    .await?;
    let statement: Value = serde_json::from_str(&text)?;
    Ok(statement
        .pointer("/rest/0/resource")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|resource| resource["type"] == "DocumentReference")
        .flat_map(|resource| {
            resource["searchParam"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|parameter| parameter.get("name")?.as_str().map(str::to_owned))
        .collect())
}

/// Checks that `bundle` is the empty `searchset` with the one `outcome`
/// entry asking for the lookup again.
fn unknown(bundle: &Value) -> TestResult {
    if !entries(bundle, "match").is_empty() {
        return Err(format!("no match: {bundle}").into());
    }
    let outcomes = entries(bundle, "outcome");
    let [outcome] = outcomes.as_slice() else {
        return Err(format!("one outcome entry: {bundle}").into());
    };
    let code = outcome.pointer("/issue/0/code").and_then(Value::as_str);
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str);
    if code != Some("not-found")
        || !diagnostics.is_some_and(|text| text.contains("repeat the Patient lookup"))
    {
        return Err(format!("the outcome asks for the lookup again: {outcome}").into());
    }
    Ok(())
}

#[tokio::test]
async fn every_written_entry_names_the_patient_by_one_stable_id() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let first = patient_id(&listed(&app, list(SUMMARY_TYPE)?).await?)?;
    let second = patient_id(&listed(&app, list(SUMMARY_TYPE)?).await?)?;
    assert_eq!(first, second, "FHIR R4 Resource.id: one id in every answer");
    assert_eq!(first.len(), 52, "{first}");
    assert!(
        !first.contains(PATIENT_TAIL),
        "no identifier in the id: {first}"
    );
    Ok(())
}

#[tokio::test]
async fn without_a_store_a_search_by_patient_is_refused_and_not_declared() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let id = patient_id(&listed(&app, list(SUMMARY_TYPE)?).await?)?;
    members.proxy_a.clear_journal();
    members.proxy_b.clear_journal();
    let (status, text) = call(app.clone(), search(&format!("patient=Patient%2F{id}"))?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "{text}");
    assert!(text.contains("keeps no record of patient ids"), "{text}");
    assert!(members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty());
    assert!(!declared(&app).await?.iter().any(|name| name == "patient"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn with_the_store_a_patient_id_lists_its_documents_until_it_expires() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let server = database::postgres_tls("patient_id", &[]).await?;
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let store = store_toml(&server, "patient_id", dir.path())?;
    let app = tokio::task::block_in_place(|| members.gateway(dir.path(), ("", &store)))?;
    assert!(declared(&app).await?.iter().any(|name| name == "patient"));
    let by_identifier = listed(&app, list(SUMMARY_TYPE)?).await?;
    let id = patient_id(&by_identifier)?;
    for reference in [format!("Patient%2F{id}"), id.clone()] {
        let by_id = listed(&app, search(&format!("patient={reference}{SUMMARY_TYPE}"))?).await?;
        assert_eq!(
            entries(&by_id, "match").len(),
            entries(&by_identifier, "match").len(),
            "the id resolves to the patient the identifier names: {by_id}"
        );
        assert_eq!(patient_id(&by_id)?, id);
    }
    let never = "a".repeat(52);
    unknown(&listed(&app, search(&format!("patient=Patient%2F{never}"))?).await?)?;
    executed(
        &server.plain_url("patient_id"),
        "UPDATE ferrofed.patient_id SET expires_at = now() - interval '1 second'",
    )
    .await?;
    members.proxy_a.clear_journal();
    members.proxy_b.clear_journal();
    unknown(&listed(&app, search(&format!("patient=Patient%2F{id}"))?).await?)?;
    assert!(
        members.proxy_a.journal().is_empty() && members.proxy_b.journal().is_empty(),
        "an expired id asks no member"
    );
    Ok(())
}
