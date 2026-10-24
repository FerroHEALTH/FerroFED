// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member restricting the patient leaves no trace in any answer the
//! national connector reads: the summary, the document list and the
//! prescription search answer as they do where that member does not know the
//! patient, and a read by id of the restricting member's prescription answers
//! as an id the face never wrote, with that member asked nothing (Regulation
//! (EU) 2025/327 Art 8, Art 11(5); §11.3).
//!
//! The restriction is the consent pre-filter's denial of node B (§14,
//! Step 1). The IHE binding carries no pre-filter of its own, so this test
//! resolves the patient through the development cross-reference, whose
//! `[[dev.consent_denied]]` row carries the denial.

use std::error::Error;

use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_server::fhir::locator::Locator;
use ferrofed_testkit::containers;
use ferrofed_testkit::national_connector::{Answer, NationalConnector};
use http::StatusCode;
use serde_json::Value;

use super::{Harness, TestResult, national, patient_id};
use crate::e2e::{EHR_A, EHR_B, PATIENT};

/// The development cross-reference knowing the patient at the members
/// `rows` name, with the `[[dev.consent_denied]]` rows of `denied`.
fn crossref(rows: &[(&str, uuid::Uuid)], denied: &[&str]) -> String {
    let (namespace, value) = (PATIENT.namespace(), PATIENT.value());
    let mut text = String::from("profile = \"development\"\n");
    for (member, ehr_id) in rows {
        text.push_str(&crossref_row(&namespace, &value, member, *ehr_id));
    }
    for member in denied {
        text.push_str(&denied_row(&namespace, &value, member));
    }
    // NOTE: §14 Step 1: a read by id names no patient, so the pre-filter is asked about the
    // patient the binding names behind the id's ehr_id, in these namespaces.
    text.push_str(&namespaces_table(&namespace));
    text
}

/// One `[[dev.crossref]]` row knowing the patient at `member`.
fn crossref_row(namespace: &str, value: &str, member: &str, ehr_id: uuid::Uuid) -> String {
    format!(
        "\n[[dev.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
    )
}

/// The `[access_log]` table naming `namespace` as the one the patient behind
/// an `ehr_id` is named in.
fn namespaces_table(namespace: &str) -> String {
    format!("\n[access_log]\npatient_namespaces = [\"{namespace}\"]\n")
}

/// One `[[dev.consent_denied]]` row denying the patient at `member`.
fn denied_row(namespace: &str, value: &str, member: &str) -> String {
    format!(
        "\n[[dev.consent_denied]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\n"
    )
}

/// Whether `text` has the form of a UUID's hex groups somewhere in it.
fn holds_uuid(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_hexdigit() || c == '-'))
        .any(|word| uuid::Uuid::parse_str(word).is_ok())
}

/// Whether `text` reads as an RFC 3339 date and time.
fn is_instant(text: &str) -> bool {
    text.parse::<jiff::Timestamp>().is_ok()
}

/// `text` with the loopback origin each harness gateway listens on
/// replaced, since the two gateways of a test listen on two ports.
fn without_origin(text: &str) -> String {
    const LOOPBACK: &str = "http://127.0.0.1:";
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find(LOOPBACK) {
        out.push_str(rest.get(..at).unwrap_or_default());
        out.push_str("<origin>");
        let port = rest.get(at + LOOPBACK.len()..).unwrap_or_default();
        rest = port.trim_start_matches(|c: char| c.is_ascii_digit());
    }
    out.push_str(rest);
    out
}

/// `value` with every value minted per answer replaced by a placeholder: a
/// string holding a UUID, an instant, and the gateway's loopback origin.
fn stable(value: &Value) -> Value {
    match value {
        Value::String(text) if holds_uuid(text) => Value::from("<minted>"),
        Value::String(text) if is_instant(text) => Value::from("<instant>"),
        Value::String(text) => Value::from(without_origin(text)),
        Value::Array(items) => Value::Array(items.iter().map(stable).collect()),
        Value::Object(members) => Value::Object(
            members
                .iter()
                .map(|(key, member)| (key.clone(), stable(member)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// What the connector read, its status and its body with every minted value
/// replaced.
fn read_as(answer: &Answer) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let body: Value = if answer.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&answer.body)?
    };
    Ok((answer.status, stable(&body)))
}

/// The three answers the connector reads of one gateway by the patient's
/// identifier: the summary, the document list and the prescription search.
async fn answers(connector: &NationalConnector) -> Result<Vec<Answer>, Box<dyn Error>> {
    let held = national(&PATIENT.namespace(), &PATIENT.value());
    let patient = patient_id(&held);
    Ok(vec![
        connector.summary(patient).await?,
        connector.documents(patient).await?,
        connector.prescriptions(patient).await?,
    ])
}

// conformance: CP-26
#[tokio::test]
async fn a_restricting_member_leaves_no_trace_in_any_answer() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let harness = Box::pin(Harness::start()).await?;
    let restricted_dir = tempfile::tempdir()?;
    let unknown_dir = tempfile::tempdir()?;
    let restricted = harness
        .gateway(
            restricted_dir.path(),
            &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)], &["node-b"]),
        )
        .await?;
    let unknown = harness
        .gateway(unknown_dir.path(), &crossref(&[("node-a", EHR_A)], &[]))
        .await?;

    harness.clear_journals();
    let read_restricted = answers(&restricted.connector).await?;
    let asked_at_b: Vec<String> = harness
        .nodes
        .b
        .proxy
        .journal()
        .iter()
        .chain(harness.proxy_b.journal().iter())
        .map(|capture| format!("{} {}", capture.method, capture.path))
        .collect();
    assert!(
        asked_at_b.is_empty(),
        "the restricting member is never asked: {asked_at_b:?}"
    );
    let read_unknown = answers(&unknown.connector).await?;

    let labels = ["summary", "document list", "prescription search"];
    for ((label, restricted), unknown) in labels.iter().zip(&read_restricted).zip(&read_unknown) {
        assert!(
            restricted.status.is_success() || restricted.status == StatusCode::NOT_FOUND,
            "{label}: {} {}",
            restricted.status,
            restricted.body
        );
        assert_eq!(
            read_as(unknown)?,
            read_as(restricted)?,
            "Art 8, Art 11(5): the {label} of a restricted patient reads as where the member does not know the patient"
        );
        let lower = restricted.body.to_ascii_lowercase();
        for trace in ["consent", "restrict", "denied", "withheld"] {
            assert!(
                !lower.contains(trace),
                "{label} shows {trace}: {}",
                restricted.body
            );
        }
    }

    // NOTE: §14 Step 1, N27a: a read by id asks the pre-filter about the patient behind the id's
    // ehr_id, so the restricting member answers as an id the face never wrote.
    let at_b = Locator::new(
        EndpointId::new("node-b-mpd")?,
        EhrId::new(EHR_B.to_string())?,
        "rx-b1",
    )
    .id()
    .ok_or("the parts fit an id")?;
    harness.clear_journals();
    let denied = restricted.connector.prescription(&at_b).await?;
    assert!(
        harness.proxy_b.journal().is_empty() && harness.nodes.b.proxy.journal().is_empty(),
        "a read by id asks the restricting member nothing"
    );
    let never_written = restricted
        .connector
        .prescription("not-an-id-the-face-wrote")
        .await?;
    assert_eq!(StatusCode::NOT_FOUND, denied.status, "{}", denied.body);
    assert_eq!(
        read_as(&never_written)?,
        read_as(&denied)?,
        "Art 8: a read at the restricting member answers as an unknown id"
    );
    let lower = denied.body.to_ascii_lowercase();
    for trace in ["consent", "restrict", "denied", "withheld"] {
        assert!(
            !lower.contains(trace),
            "the read shows {trace}: {}",
            denied.body
        );
    }
    Ok(())
}
