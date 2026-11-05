// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's read of a member's document at its
//! `Binary/{token}` URL held to the patient of its treatment relationship
//! confirmation (eHDSI XCA Profile 8.0.1 §4.2.2): a URL listed under one TRC
//! and read under a TRC of another patient names a document "assigned to
//! another patient", so it is answered as no data, as an unknown patient's
//! summary is, asks no member, and is recorded as a security event. Read
//! under its own TRC, the document is served unchanged.

use http::StatusCode;
use serde_json::Value;

use super::TestResult;
use super::list::{
    DOCUMENT_A, DOCUMENT_B, Members, binary, by_custodian, entries, fhir_document, list, names,
};
use super::receive::{OTHER, OTHER_NAMESPACE};
use crate::auth::{bearing, minted};
use crate::contact_point::{HP_ID, TRC_CLAIM, claim_names, relaying};
use crate::facade::{NAMESPACE, PATIENT, settings_with_room};
use crate::support::{Logs, call};

/// A patient identifier the cross-reference knows at no member.
const UNKNOWN: &str = "SENTINEL-UNKNOWN-6h1d";

/// The contact point's token, its TRC binding `system`|`value`.
fn treating(system: &str, value: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut claims = relaying();
    claims
        .other
        .insert(TRC_CLAIM.to_owned(), format!("{system}|{value}"));
    minted(&claims)
}

/// The gateway over `members`, its issuer declared a national contact point.
fn gateway(
    members: &Members,
    dir: &std::path::Path,
) -> Result<axum::Router, Box<dyn std::error::Error>> {
    let mut server = settings_with_room();
    let declared = claim_names().resolve("trust.issuer[0].national_contact_point")?;
    for issuer in &mut server.auth.issuers {
        issuer.national_contact_point = Some(declared.clone());
    }
    members.gateway_serving(dir, (("", ""), ""), &server)
}

/// The URL the list names node B's document at, listed by the contact point
/// under the test patient's TRC.
async fn listed_b(app: axum::Router) -> Result<String, Box<dyn std::error::Error>> {
    let token = treating(NAMESPACE, PATIENT)?;
    let (status, text) = call(app, bearing(list("")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let own = by_custodian(&matches, "org-b").ok_or("node B's document")?;
    names(own, DOCUMENT_B)
}

#[tokio::test]
async fn a_document_listed_under_its_own_trc_is_read_unchanged() -> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let url = listed_b(app.clone()).await?;
    let token = treating(NAMESPACE, PATIENT)?;
    let (status, text) = call(app, bearing(binary(&url)?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(text.as_bytes(), DOCUMENT_B, "the member's bytes, unchanged");
    Ok(())
}

// conformance: track-10
#[tokio::test]
async fn a_document_read_under_another_patients_trc_is_no_data_recorded_and_fetched_nowhere()
-> TestResult {
    let members = Members::start(fhir_document(DOCUMENT_A), fhir_document(DOCUMENT_B)).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let url = listed_b(app.clone()).await?;
    let listed = members.proxy_b.journal().len();
    let unknown = call(
        app.clone(),
        bearing(
            http::Request::get(format!(
                "{}/Patient/$summary?identifier={}%7C{UNKNOWN}",
                super::FHIR,
                NAMESPACE.replace(':', "%3A")
            ))
            .body(axum::body::Body::empty())?,
            &treating(NAMESPACE, UNKNOWN)?,
        )?,
    )
    .await?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let answer = call(
        app.clone(),
        bearing(binary(&url)?, &treating(OTHER_NAMESPACE, OTHER)?)?,
    )
    .await?;
    drop(guard);
    assert_eq!(StatusCode::NOT_FOUND, answer.0, "{}", answer.1);
    assert_eq!(
        unknown, answer,
        "XCA §4.2.2: as an unknown patient, No Data"
    );
    let outcome: Value = serde_json::from_str(&answer.1)?;
    let coding = outcome
        .pointer("/issue/0/details/coding")
        .and_then(Value::as_array)
        .ok_or("details")?;
    for code in ["trc-patient-mismatch", "ERROR_PS_NOT_FOUND"] {
        assert!(
            coding.iter().any(|coding| coding["code"] == code),
            "{code}: {}",
            answer.1
        );
    }
    assert!(
        !answer.1.contains("synthetic-doc-b"),
        "no byte of the document"
    );
    assert_eq!(
        listed,
        members.proxy_b.journal().len(),
        "the member is not asked again"
    );
    let logged = logs.text();
    let line = logged
        .lines()
        .find(|line| line.contains("\"trc-patient-mismatch\""))
        .ok_or_else(|| format!("a security alert is recorded: {logged}"))?;
    assert!(line.contains(HP_ID), "it names the professional: {line}");
    assert!(
        !logged.contains(OTHER) && !logged.contains(PATIENT),
        "SR 5: {logged}"
    );
    Ok(())
}
