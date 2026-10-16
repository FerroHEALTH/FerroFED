// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version read of a prescription and a dispensation (FHIR R4
//! `http.html` §3.1.0.3, vread): `GET {fhir-base}/MedicationDispense/{id}/_history/{vid}`
//! and its `MedicationRequest` twin read as the read does, at the member the
//! id names alone, by its own `ehr_id` and its own id (§5.4.1, N33), and
//! answer the resource only when the member holds it at that
//! `meta.versionId` now; any other version is `404`. The `Location` a
//! dispensation's receipt answers reads back the dispensation it located.

use axum::body::Body;
use http::{Request, StatusCode, header};
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::list::EHR_SYSTEM;
use super::super::receive::issue;
use super::dispense::{BY_IDENTIFIER, CREATED, created, dispensation, gateway, members, post};
use super::{AT_A, Members, RX_A, RX_B, face_id, page, searchset};
use crate::facade::{EHR_A, PATIENT};
use crate::fhir::{FHIR, TestResult};
use crate::support::{call, send};

/// The dispensation node A holds under [`CREATED`], at its version `1`.
const HELD: &str = r#"{"resourceType":"MedicationDispense","id":"md-1","meta":{"versionId":"1","lastUpdated":"2026-10-16T10:00:00Z"},"status":"completed","medicationCodeableConcept":{"text":"Synthetic medicine A"},"subject":{"reference":"Patient/p-a"}}"#;

/// Node A's prescription at the member's version `3`.
const RX_A_VERSIONED: &str = r#"{"resourceType":"MedicationRequest","id":"rx-a1","meta":{"versionId":"3"},"status":"active","intent":"order","subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A"}}"#;

/// A `searchset` holding the dispensation `resource`.
fn dispensed(resource: &str) -> String {
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","total":1,"entry":[{{"fullUrl":"https://cdr-a.example.org/fhir/MedicationDispense/md-1","resource":{resource},"search":{{"mode":"match"}}}}]}}"#
    )
}

/// Mounts `resource` as the dispensation node A answers a search with.
async fn holding(members: &Members, resource: &str) {
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(page(dispensed(resource)))
        .mount(&members.own_a)
        .await;
}

/// `GET {fhir-base}/MedicationDispense/{id}/_history/{version}` of node A's
/// dispensation.
fn vread(version: &str) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    Ok(
        Request::get(format!("{FHIR}/MedicationDispense/{id}/_history/{version}"))
            .body(Body::empty())?,
    )
}

/// The search requests that reached node A's own endpoint for a
/// dispensation.
fn dispensation_reads(members: &Members) -> usize {
    members
        .proxy_a
        .journal()
        .iter()
        .filter(|capture| capture.method == "GET" && capture.path == "/fhir/MedicationDispense")
        .count()
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_location_a_receipt_answered_reads_back_the_dispensation_it_located() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    holding(&members, HELD).await;
    let dir = tempfile::tempdir()?;
    let response = send(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let location = response
        .headers()
        .get(header::LOCATION)
        .ok_or("a Location")?
        .to_str()?
        .to_owned();
    let located = url::Url::parse(&location)?;
    let mut target = located.path().to_owned();
    if let Some(query) = located.query() {
        target = format!("{target}?{query}");
    }
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    assert_eq!(
        format!("{FHIR}/MedicationDispense/{id}/_history/1"),
        target,
        "FHIR R4 http.html §3.1.0.8: the face's id and the member's version"
    );
    let response = send(
        gateway(&members, dir.path())?,
        Request::get(target).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        response.status(),
        "FHIR R4 http.html §3.1.0.3: the Location the face answered is one it reads"
    );
    let field = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    assert_eq!(Some("W/\"1\"".to_owned()), field(header::ETAG));
    assert_eq!(
        Some("Fri, 16 Oct 2026 10:00:00 GMT".to_owned()),
        field(header::LAST_MODIFIED)
    );
    assert_eq!(
        Some("node-a-mpd".to_owned()),
        field(header::HeaderName::from_static(
            "openehr-federation-endpoint"
        ))
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let resource: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        Value::from(id.as_str()),
        resource["id"],
        "FHIR R4 http.html §3.1.0.3: the id read"
    );
    assert_eq!(
        "1", resource["meta"]["versionId"],
        "FHIR R4 http.html §3.1.0.3: the vid read"
    );
    let journal = members.proxy_a.journal();
    let read = journal
        .iter()
        .find(|capture| capture.method == "GET" && capture.path == "/fhir/MedicationDispense")
        .ok_or("the dispensation is asked of node A")?;
    let pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(read.query.as_deref().unwrap_or_default().as_bytes())
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
    assert_eq!(
        pairs,
        [
            (
                "patient.identifier".to_owned(),
                format!("{EHR_SYSTEM}|{EHR_A}")
            ),
            ("_id".to_owned(), CREATED.to_owned()),
        ],
        "N33: the member's ehr_id and its own id alone, never the version"
    );
    assert!(
        !read.contains(PATIENT.as_bytes()),
        "N33: the version read carries no patient identifier"
    );
    assert!(
        !members
            .proxy_b
            .journal()
            .iter()
            .any(|capture| capture.path == "/fhir/MedicationDispense"),
        "node B is never asked for the dispensation"
    );
    Ok(())
}

#[tokio::test]
async fn a_created_version_that_is_no_fhir_id_leaves_an_unversioned_location_that_reads_back()
-> TestResult {
    let unfit = ResponseTemplate::new(201).insert_header(
        "location",
        format!("https://cdr.example.org/fhir/MedicationDispense/{CREATED}/_history/1_2"),
    );
    let members = members((&[RX_A], &[RX_B]), unfit).await?;
    holding(&members, HELD).await;
    let dir = tempfile::tempdir()?;
    let response = send(
        gateway(&members, dir.path())?,
        post(&dispensation(BY_IDENTIFIER))?,
    )
    .await?;
    assert_eq!(StatusCode::CREATED, response.status());
    let location = response
        .headers()
        .get(header::LOCATION)
        .ok_or("a Location")?
        .to_str()?
        .to_owned();
    let located = url::Url::parse(&location)?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    assert_eq!(
        format!("{FHIR}/MedicationDispense/{id}"),
        located.path(),
        "FHIR R4 http.html §3.1.0.3: a vid that is no id is left out of the Location"
    );
    assert_eq!(None, located.query());
    let (status, text) = call(
        gateway(&members, dir.path())?,
        Request::get(located.path()).body(Body::empty())?,
    )
    .await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the unversioned Location reads back: {text}"
    );
    let resource: Value = serde_json::from_str(&text)?;
    assert_eq!(Value::from(id.as_str()), resource["id"]);
    Ok(())
}

#[tokio::test]
async fn a_version_the_member_does_not_hold_now_is_not_found() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    holding(&members, HELD).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(&members, dir.path())?, vread("2")?).await?;
    assert_eq!(
        StatusCode::NOT_FOUND,
        status,
        "FHIR R4 http.html §3.1.0.3: a version the face does not hold is 404: {text}"
    );
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-found", code);
    assert!(diagnostics.contains("keeps no history"), "{diagnostics}");
    assert!(!text.contains("Synthetic medicine A"), "{text}");
    assert_eq!(1, dispensation_reads(&members), "the member is asked once");
    Ok(())
}

#[tokio::test]
async fn a_dispensation_with_no_version_is_not_found_by_any_version() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    holding(&members, &HELD.replace(r#""versionId":"1","#, "")).await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(&members, dir.path())?, vread("1")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert!(!text.contains("Synthetic medicine A"), "{text}");
    Ok(())
}

#[tokio::test]
async fn a_version_that_is_no_fhir_id_asks_no_member() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    holding(&members, HELD).await;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    for tail in [
        "_history/1_2",
        "_history/1/x",
        "_history/",
        "_history",
        "_versions/1",
    ] {
        let request =
            Request::get(format!("{FHIR}/MedicationDispense/{id}/{tail}")).body(Body::empty())?;
        let (status, text) = call(gateway(&members, dir.path())?, request).await?;
        assert_ne!(StatusCode::OK, status, "{tail}: {text}");
    }
    assert!(members.unasked());
    Ok(())
}

#[tokio::test]
async fn a_prescription_is_read_at_the_version_it_holds_and_no_other() -> TestResult {
    let members = Members::start(
        page(searchset(
            "https://cdr-a.example.org",
            &[RX_A_VERSIONED],
            None,
        )),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let at = |version: &str| {
        Request::get(format!("{FHIR}/MedicationRequest/{id}/_history/{version}"))
            .body(Body::empty())
    };
    let response = send(members.both(dir.path())?, at("3")?).await?;
    assert_eq!(StatusCode::OK, response.status());
    assert_eq!(
        Some("W/\"3\""),
        response
            .headers()
            .get(header::ETAG)
            .and_then(|value| value.to_str().ok())
    );
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let resource: Value = serde_json::from_slice(&bytes)?;
    assert_eq!(Value::from(id.as_str()), resource["id"]);
    assert_eq!("3", resource["meta"]["versionId"]);
    let (status, text) = call(members.both(dir.path())?, at("2")?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    assert_eq!("not-found", issue(&text)?.0);
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_answer_the_version_read_is_named() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&members.own_a)
        .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(gateway(&members, dir.path())?, vread("1")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        issue(&text)?.1.contains("node-a-mpd (node-error)"),
        "{text}"
    );
    Ok(())
}
