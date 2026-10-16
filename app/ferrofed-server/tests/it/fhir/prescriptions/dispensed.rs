// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The read of a dispensation the face received (Regulation (EU) 2025/327
//! Art 14(1)(c)): `GET {fhir-base}/MedicationDispense/{id}` reads it by the
//! id the receipt's `Location` named, at the member that id names alone, by
//! that member's own `ehr_id` and its own id (FHIR R4 `http.html` §3.1.0.2,
//! §3.1.0.8; §5.4.1, N33).

use axum::body::Body;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::list::EHR_SYSTEM;
use super::super::receive::issue;
use super::dispense::{CREATED, created, gateway, members};
use super::{RX_A, RX_B, face_id, page};
use crate::facade::{EHR_A, PATIENT};
use crate::fhir::{FHIR, TestResult};
use crate::support::{call, send};

/// The dispensation node A holds under [`CREATED`], at its version `1`.
const HELD: &str = r#"{"resourceType":"MedicationDispense","id":"md-1","meta":{"versionId":"1","lastUpdated":"2026-10-16T10:00:00Z"},"status":"completed","medicationCodeableConcept":{"text":"Synthetic medicine A"},"subject":{"reference":"Patient/p-a"}}"#;

/// A `searchset` holding the dispensation `resource`.
fn dispensed(resource: &str) -> String {
    format!(
        r#"{{"resourceType":"Bundle","type":"searchset","total":1,"entry":[{{"fullUrl":"https://cdr-a.example.org/fhir/MedicationDispense/md-1","resource":{resource},"search":{{"mode":"match"}}}}]}}"#
    )
}

/// `GET {fhir-base}/MedicationDispense/{id}`.
fn read(id: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/MedicationDispense/{id}")).body(Body::empty())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_dispensation_a_receipt_located_is_read_at_its_member_by_its_ehr_id() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(page(dispensed(HELD)))
        .mount(&members.own_a)
        .await;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let response = send(gateway(&members, dir.path())?, read(&id)?).await?;
    assert_eq!(StatusCode::OK, response.status());
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
    let resource: serde_json::Value = serde_json::from_slice(&bytes)?;
    assert_eq!(
        resource["id"],
        serde_json::Value::from(id.as_str()),
        "FHIR R4 http.html §3.1.0.2: the id read"
    );
    assert_eq!(resource["status"], "completed");
    let journal = members.proxy_a.journal();
    let [asked] = journal.as_slice() else {
        return Err(format!("one request at node A, got {}", journal.len()).into());
    };
    assert_eq!("/fhir/MedicationDispense", asked.path);
    let pairs: Vec<(String, String)> =
        url::form_urlencoded::parse(asked.query.as_deref().unwrap_or_default().as_bytes())
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
        "N33: the member's ehr_id and its own id alone"
    );
    assert!(!members.proxy_a.journal_contains(PATIENT.as_bytes()));
    assert!(
        members.proxy_b.journal().is_empty(),
        "node B is never asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_dispensation_id_with_no_owner_is_not_found_and_asks_no_member() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let dir = tempfile::tempdir()?;
    let prescription = face_id("node-z-mpd", EHR_A, CREATED)?;
    for id in ["synthetic-md-1", prescription.as_str()] {
        let (status, text) = call(gateway(&members, dir.path())?, read(id)?).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{id}: {text}");
        assert_eq!(
            "no dispensation is known by this id",
            issue(&text)?.1,
            "one text whatever the reason"
        );
    }
    assert!(members.unasked());
    Ok(())
}

#[tokio::test]
async fn a_member_that_does_not_answer_the_dispensation_read_is_named() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    Mock::given(method("GET"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&members.own_a)
        .await;
    let dir = tempfile::tempdir()?;
    let id = face_id("node-a-mpd", EHR_A, CREATED)?;
    let (status, text) = call(gateway(&members, dir.path())?, read(&id)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(
        issue(&text)?.1.contains("node-a-mpd (node-error)"),
        "{text}"
    );
    Ok(())
}
