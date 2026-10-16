// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The version the read of one prescription answers (FHIR R4 `http.html`
//! §3.1.0.2, §3.1.0.1.3 and §3.1.0.5): the member's `meta.versionId`, when
//! it is a FHIR `id`, is the read's weak `ETag`, and its `meta.lastUpdated`,
//! when it is a FHIR `instant`, is the read's `Last-Modified` as an RFC 9110
//! §5.6.7 HTTP-date; a prescription with neither is answered with no such
//! header, and one whose value has another form is too, logged by its shape
//! alone.

use std::error::Error;

use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{HeaderName, Request, Response, StatusCode, header};

use super::{AT_A, Members, RX_A, RX_B, face_id, page, searchset};
use crate::fhir::{FHIR, TestResult};
use crate::support::{Logs, send};

/// Node A's prescription at the member's version `3`.
const RX_A_VERSIONED: &str = r#"{"resourceType":"MedicationRequest","id":"rx-a1","meta":{"versionId":"3"},"status":"active","intent":"order","subject":{"reference":"Patient/p-a"},"medicationCodeableConcept":{"text":"Synthetic medicine A"}}"#;

/// A `versionId` that is no FHIR `id`, as JSON string text: a space, a
/// quote and a slash.
const UNFIT: &str = "SYNTHETIC v\\\"3/x";

/// What the read answered, and everything logged while it ran.
struct Answered {
    status: StatusCode,
    etag: Option<String>,
    modified: Option<String>,
    text: String,
    logs: String,
}

/// The value of the field `name` of `response`, when it has one.
fn field(response: &Response<Body>, name: &HeaderName) -> Result<Option<String>, Box<dyn Error>> {
    Ok(response
        .headers()
        .get(name)
        .map(|value| value.to_str().map(str::to_owned))
        .transpose()?)
}

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
    let etag = field(&response, &header::ETAG)?;
    let modified = field(&response, &header::LAST_MODIFIED)?;
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok(Answered {
        status,
        etag,
        modified,
        text: String::from_utf8(bytes.to_vec())?,
        logs: logs.text(),
    })
}

/// [`RX_A_VERSIONED`] with the `meta` member `meta` in place of its own.
fn with_meta(meta: &str) -> String {
    RX_A_VERSIONED.replace(r#""meta":{"versionId":"3"}"#, &format!(r#""meta":{meta}"#))
}

#[tokio::test]
async fn a_prescription_at_a_version_is_read_with_its_weak_etag() -> TestResult {
    let read = read_of(RX_A_VERSIONED).await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.text);
    assert_eq!(
        Some("W/\"3\""),
        read.etag.as_deref(),
        "FHIR R4 http.html §3.1.0.2, §3.1.0.1.3: the member's versionId as a weak ETag"
    );
    Ok(())
}

#[tokio::test]
async fn a_prescription_with_no_version_is_read_with_no_etag() -> TestResult {
    let read = read_of(RX_A).await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.text);
    assert_eq!(None, read.etag, "no versionId, no ETag");
    Ok(())
}

#[tokio::test]
async fn a_version_that_is_no_fhir_id_gives_no_etag_and_is_logged_by_shape() -> TestResult {
    let read = read_of(&with_meta(&format!(r#"{{"versionId":"{UNFIT}"}}"#))).await?;
    assert_eq!(
        StatusCode::OK,
        read.status,
        "the prescription is answered: {}",
        read.text
    );
    assert_eq!(
        None, read.etag,
        "FHIR R4 http.html §3.1.0.5: a versionId is valid as the id data type is"
    );
    let logs = &read.logs;
    assert!(logs.contains("no FHIR id"), "the shape is logged: {logs}");
    assert!(
        !logs.contains("SYNTHETIC v"),
        "the versionId never reaches the log: {logs}"
    );
    Ok(())
}

#[tokio::test]
async fn a_version_that_is_no_string_gives_no_etag_and_is_logged_by_shape() -> TestResult {
    let read = read_of(&with_meta(r#"{"versionId":3}"#)).await?;
    assert_eq!(
        StatusCode::OK,
        read.status,
        "the prescription is answered: {}",
        read.text
    );
    assert_eq!(
        None, read.etag,
        "a versionId that is no string gives no ETag"
    );
    let logs = &read.logs;
    assert!(logs.contains("no string"), "the shape is logged: {logs}");
    Ok(())
}

#[tokio::test]
async fn a_prescription_updated_at_an_instant_is_read_with_its_http_date() -> TestResult {
    let read = read_of(&with_meta(
        r#"{"lastUpdated":"2026-03-01T10:15:30.250+01:00"}"#,
    ))
    .await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.text);
    assert_eq!(
        Some("Sun, 01 Mar 2026 09:15:30 GMT"),
        read.modified.as_deref(),
        "FHIR R4 http.html §3.1.0.2, §3.1.0.1.3: lastUpdated as an RFC 9110 §5.6.7 HTTP-date"
    );
    Ok(())
}

#[tokio::test]
async fn a_prescription_with_no_last_updated_is_read_with_no_last_modified() -> TestResult {
    let read = read_of(RX_A_VERSIONED).await?;
    assert_eq!(StatusCode::OK, read.status, "{}", read.text);
    assert_eq!(None, read.modified, "no lastUpdated, no Last-Modified");
    let read = read_of(RX_A).await?;
    assert_eq!(None, read.modified, "no meta, no Last-Modified");
    Ok(())
}

#[tokio::test]
async fn a_last_updated_that_is_no_fhir_instant_gives_no_last_modified_and_is_logged_by_shape()
-> TestResult {
    for unfit in [
        r#""2026-03-01""#,
        r#""2026-03-01T10:15:30""#,
        r#""2026-03-01T10:15Z""#,
        "20260301",
    ] {
        let read = read_of(&with_meta(&format!(r#"{{"lastUpdated":{unfit}}}"#))).await?;
        assert_eq!(
            StatusCode::OK,
            read.status,
            "the prescription is answered: {}",
            read.text
        );
        assert_eq!(
            None, read.modified,
            "FHIR R4 datatypes instant: {unfit} is no instant, so no Last-Modified"
        );
        let logs = &read.logs;
        assert!(
            logs.contains("no FHIR instant"),
            "the shape is logged: {logs}"
        );
        assert!(
            !logs.contains("2026-03-01") && !logs.contains("20260301"),
            "the lastUpdated never reaches the log: {logs}"
        );
    }
    Ok(())
}
