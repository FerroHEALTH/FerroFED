// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version the read of one prescription answers: the member's
//! `meta.versionId`, when it gives one that is a FHIR `id`, is the read's
//! weak `ETag` (FHIR R4 `http.html` §3.1.0.2, §3.1.0.1.3 and §3.1.0.5); a
//! prescription with none, or with one that is no FHIR `id`, is answered
//! with no `ETag`, and the second is logged by its shape alone.

use std::error::Error;

use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{Request, StatusCode, header};

use super::{AT_A, Members, RX_A, RX_B, face_id, page, searchset};
use crate::fhir::{FHIR, TestResult};
use crate::support::{Logs, send};

/// Node A's prescription at the member's version `3`.
const RX_A_VERSIONED: &str = r#"{"resourceType":"MedicationRequest","id":"rx-a1","meta":{"versionId":"3"},"status":"active","intent":"order","subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A"}}"#;

/// A `versionId` that is no FHIR `id`, as JSON string text: a space, a
/// quote and a slash.
const UNFIT: &str = "SYNTHETIC v\\\"3/x";

/// What the read answered: the status, the `ETag`, the body and everything
/// logged.
type Answered = (StatusCode, Option<String>, String, String);

/// The read of `rx-a1` at node A when its own endpoint answers the
/// prescription `resource`.
async fn read_of(resource: &str) -> Result<Answered, Box<dyn Error>> {
    let members = Members::start(
        page(searchset("https://cdr-a.example.org", &[resource], None)),
        page(searchset("https://cdr-b.example.org", &[RX_B], None)),
    )
    .await?;
    let dir = tempfile::tempdir()?;
    let app = members.both(dir.path())?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let request = Request::get(format!("{FHIR}/MedicationRequest/{id}")).body(Body::empty())?;
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let response = send(app, request).await;
    drop(guard);
    let response = response?;
    let status = response.status();
    let etag = response
        .headers()
        .get(header::ETAG)
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()?;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((
        status,
        etag,
        String::from_utf8(bytes.to_vec())?,
        logs.text(),
    ))
}

#[tokio::test]
async fn a_prescription_at_a_version_is_read_with_its_weak_etag() -> TestResult {
    let (status, etag, text, _) = read_of(RX_A_VERSIONED).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        Some("W/\"3\""),
        etag.as_deref(),
        "FHIR R4 http.html §3.1.0.2, §3.1.0.1.3: the member's versionId as a weak ETag"
    );
    Ok(())
}

#[tokio::test]
async fn a_prescription_with_no_version_is_read_with_no_etag() -> TestResult {
    let (status, etag, text, _) = read_of(RX_A).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(None, etag, "no versionId, no ETag");
    Ok(())
}

#[tokio::test]
async fn a_version_that_is_no_fhir_id_gives_no_etag_and_is_logged_by_shape() -> TestResult {
    let unfit = RX_A_VERSIONED.replace(r#""versionId":"3""#, &format!(r#""versionId":"{UNFIT}""#));
    let (status, etag, text, logs) = read_of(&unfit).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the prescription is answered: {text}"
    );
    assert_eq!(
        None, etag,
        "FHIR R4 http.html §3.1.0.5: a versionId is valid as the id data type is"
    );
    assert!(logs.contains("no FHIR id"), "the shape is logged: {logs}");
    assert!(
        !logs.contains("SYNTHETIC v"),
        "the versionId never reaches the log: {logs}"
    );
    Ok(())
}

#[tokio::test]
async fn a_version_that_is_no_string_gives_no_etag_and_is_logged_by_shape() -> TestResult {
    let unfit = RX_A_VERSIONED.replace(r#""versionId":"3""#, r#""versionId":3"#);
    let (status, etag, text, logs) = read_of(&unfit).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the prescription is answered: {text}"
    );
    assert_eq!(None, etag, "a versionId that is no string gives no ETag");
    assert!(logs.contains("no string"), "the shape is logged: {logs}");
    Ok(())
}
