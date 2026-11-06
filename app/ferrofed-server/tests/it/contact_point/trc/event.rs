// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A break of the TRC binding on the ITS-REST face is the security event the
//! FHIR face records (eHDSI XCA Profile 8.0.1 §4.2.2: an audit trail and a
//! security alert), naming the professional and both patients by pseudonym
//! alone (eHDSI Section I Security Policies 10.0.0 SR 5). A TRC patient
//! the cross-reference cannot resolve is its own event, never the one of a
//! `patient/` grant.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the test reads the security event's JSON log line"
)]

use axum::body::Body;
use http::{Request, StatusCode};
use serde_json::Value;

use super::{OTHER, OTHER_A, UNKNOWN, answered, as_treating, gateway, unresolving};
use crate::contact_point::HP_ID;
use crate::facade::{NAMESPACE, PATIENT, body, post};
use crate::support::Logs;

/// The fields of the `trc-patient-mismatch` event of the log `logged`.
fn event(logged: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let line = logged
        .lines()
        .find(|line| line.contains("\"trc-patient-mismatch\""))
        .ok_or_else(|| format!("a security event is recorded: {logged}"))?;
    let event: Value = serde_json::from_str(line)?;
    Ok(event.get("fields").cloned().unwrap_or(event))
}

/// Sends `request` to the suite's gateway and returns its status with what
/// the log recorded.
async fn logged_by(
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    logged_at(&gateway().await?, request).await
}

/// Sends `request` to `bound` and returns its status with what the log
/// recorded.
async fn logged_at(
    bound: &super::Bound,
    request: Request<Body>,
) -> Result<(StatusCode, String), Box<dyn std::error::Error>> {
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, _) = answered(bound, request).await?;
    drop(guard);
    Ok((status, logs.text()))
}

// NOTE: no specification governs this: our own design; a TRC patient that cannot be
// resolved names the TRC in the security log, never a `patient/` grant it does not hold.
#[tokio::test]
async fn a_trc_patient_that_cannot_be_resolved_is_its_own_security_event()
-> Result<(), Box<dyn std::error::Error>> {
    let bound = unresolving().await?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let request = as_treating(post(body(&aql)?)?, PATIENT)?;
    let (status, logged) = logged_at(&bound, request).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{logged}");
    let line = logged
        .lines()
        .find(|line| line.contains("\"trc-patient-unresolved\""))
        .ok_or_else(|| format!("the TRC's own event is recorded: {logged}"))?;
    assert!(
        line.contains("treatment relationship confirmation"),
        "the text names the TRC: {line}"
    );
    assert!(
        !logged.contains("\"patient-context-unavailable\""),
        "no patient/ grant is named: {logged}"
    );
    assert!(!logged.contains(PATIENT), "SR 5: {logged}");
    bound.nobody_asked().await?;
    Ok(())
}

#[tokio::test]
async fn another_patients_query_is_a_security_event_naming_pseudonyms_alone()
-> Result<(), Box<dyn std::error::Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{OTHER}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, logged) = logged_by(as_treating(post(body(&aql)?)?, PATIENT)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let fields = event(&logged)?;
    assert_eq!(fields.get("hp_identifier"), Some(&Value::from(HP_ID)));
    assert_eq!(fields.get("what"), Some(&Value::from("patient")));
    let pseudonym = |name: &str| {
        fields
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let (asked, bound) = (pseudonym("asked"), pseudonym("bound"));
    for value in [&asked, &bound] {
        assert_eq!(32, value.len(), "a pseudonym: {fields}");
        assert!(
            value.bytes().all(|byte| byte.is_ascii_hexdigit()),
            "{fields}"
        );
    }
    assert_ne!(asked, bound, "the two patients are told apart: {fields}");
    for withheld in [PATIENT, OTHER] {
        assert!(!logged.contains(withheld), "SR 5: {logged}");
    }
    Ok(())
}

#[tokio::test]
async fn another_patients_ehr_is_a_security_event_naming_no_ehr_id()
-> Result<(), Box<dyn std::error::Error>> {
    let read = Request::get(format!("/v1/ehr/{OTHER_A}")).body(Body::empty())?;
    let (status, logged) = logged_by(as_treating(read, PATIENT)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let fields = event(&logged)?;
    assert_eq!(fields.get("what"), Some(&Value::from("ehr")));
    assert!(!logged.contains(OTHER_A), "SR 5: {logged}");
    Ok(())
}

#[tokio::test]
async fn a_trc_patient_no_member_holds_is_no_data_and_no_security_event()
-> Result<(), Box<dyn std::error::Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{UNKNOWN}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    let (status, logged) = logged_by(as_treating(post(body(&aql)?)?, UNKNOWN)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    assert!(
        !logged.contains("\"trc-patient-mismatch\""),
        "the TRC's own patient breaks no binding: {logged}"
    );
    Ok(())
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: the subject is compared before it is resolved, so
// the event names the patient asked, never the resource a resolution found.
#[tokio::test]
async fn another_patients_read_by_subject_is_a_security_event_before_any_lookup()
-> Result<(), Box<dyn std::error::Error>> {
    let namespace = NAMESPACE.replace(':', "%3A");
    let read = Request::get(format!(
        "/v1/ehr?subject_id={OTHER}&subject_namespace={namespace}"
    ))
    .header(openehr_federation::headers::ENDPOINT, "node-a-pub")
    .body(Body::empty())?;
    let (status, logged) = logged_by(as_treating(read, PATIENT)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status);
    let fields = event(&logged)?;
    assert_eq!(
        fields.get("what"),
        Some(&Value::from("patient")),
        "{fields}"
    );
    assert!(!logged.contains(OTHER), "SR 5: {logged}");
    Ok(())
}
