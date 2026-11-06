// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's request on the FHIR face held to the patient
//! its treatment relationship confirmation binds (eHDSI XCA Profile 8.0.1
//! §4.2.1, §4.2.2; SAML Profile 10.0.0 §4.3).
//!
//! The TRC patient's own summary and document list are served. Another
//! patient is answered as no data before the demographics supplier or any
//! member is asked, with the `trc-patient-mismatch` refusal and the
//! service's Requirements Catalogue 10.0.0 §1.25 `*_NOT_FOUND` code, and the
//! answer is the one an unknown patient gets. A token naming no TRC patient
//! is `trc-missing`, while the identification lookup needs none. A mismatch
//! is a security event naming the professional and both patients by
//! pseudonym alone (eHDSI Section I Security Policies 10.0.0 SR 5).

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use http::{Request, StatusCode};
use serde_json::Value;
use wiremock::matchers::any;
use wiremock::{Mock, ResponseTemplate};

use super::receive::{OTHER, OTHER_A, OTHER_NAMESPACE, rows};
use super::{FHIR, TestResult, UID_A, UID_B, node_holding, summary, supplier, text_registered};
use crate::auth::{bearing, minted};
use crate::contact_point::{HP_ID, TRC_CLAIM, claim_names, relaying};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, received, settings_with_room};
use crate::support::{Logs, call};

/// The `ehr_id` of [`OTHER`] at node B.
const OTHER_B: &str = "5555eeee-5555-4555-8555-555555555555";

/// A patient identifier the cross-reference knows at no member.
const UNKNOWN: &str = "SENTINEL-UNKNOWN-4p8w";

/// The face over node A and node B, the test patient and [`OTHER`] known at
/// both, the summary header asked of `pdq`, its issuer declared a national
/// contact point.
fn gateway(dir: &Path, servers: (&Server, &Server, &Server)) -> Result<Router, Box<dyn Error>> {
    let others = rows(&[
        (OTHER_NAMESPACE, OTHER, "node-a", OTHER_A),
        (OTHER_NAMESPACE, OTHER, "node-b", OTHER_B),
    ]);
    gateway_with(
        dir,
        servers,
        (&[("node-a", EHR_A), ("node-b", EHR_B)], &others),
    )
}

/// The face of [`gateway`] with the test patient's cross-reference rows
/// `held` and the other rows `others`.
fn gateway_with(
    dir: &Path,
    (a, b, pdq): (&Server, &Server, &Server),
    (held, others): (&[(&str, &str)], &str),
) -> Result<Router, Box<dyn Error>> {
    let text = text_registered(
        dir,
        (&a.uri(), &b.uri(), &pdqm_stub::base_url(pdq)),
        held,
        "",
        ("", others),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let state = Arc::new(AppState::build(&settings)?);
    let mut server = settings_with_room();
    let declared = claim_names().resolve("trust.issuer[0].national_contact_point")?;
    for issuer in &mut server.auth.issuers {
        issuer.national_contact_point = Some(declared.clone());
    }
    Ok(ferrofed_server::router(state, &server))
}

/// The contact point's claims, its TRC binding `system`|`value`.
fn treating(system: &str, value: &str) -> Claims {
    let mut claims = relaying();
    claims
        .other
        .insert(TRC_CLAIM.to_owned(), format!("{system}|{value}"));
    claims
}

/// The contact point's claims with no TRC patient.
fn untreated() -> Claims {
    let mut claims = relaying();
    claims.other.remove(TRC_CLAIM);
    claims
}

/// `system`|`value` percent-encoded for a query.
fn encoded(system: &str, value: &str) -> String {
    format!("{}%7C{value}", system.replace(':', "%3A"))
}

/// `GET {fhir-base}/Patient/$summary` for `system`|`value`.
fn summary_of(system: &str, value: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "{FHIR}/Patient/$summary?identifier={}",
        encoded(system, value)
    ))
    .body(Body::empty())
}

/// `GET {fhir-base}/DocumentReference` for `system`|`value`.
fn documents_of(system: &str, value: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "{FHIR}/DocumentReference?patient.identifier={}",
        encoded(system, value)
    ))
    .body(Body::empty())
}

/// The codes of the first issue's `details` of `outcome`.
fn details(outcome: &Value) -> Vec<String> {
    outcome
        .pointer("/issue/0/details/coding")
        .and_then(Value::as_array)
        .map(|codings| {
            codings
                .iter()
                .filter_map(|coding| coding.get("code").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// The three servers: node A, node B and the PDQm Supplier.
async fn servers() -> (Server, Server, Server) {
    (
        node_holding(UID_A).await,
        node_holding(UID_B).await,
        supplier().await,
    )
}

/// Whether nothing was asked of `a`, `b` or `pdq`.
async fn nobody_asked(a: &Server, b: &Server, pdq: &Server) -> Result<bool, Box<dyn Error>> {
    Ok(received(a).await?.is_empty()
        && received(b).await?.is_empty()
        && pdqm_stub::searches(pdq).await == 0)
}

#[tokio::test]
async fn the_trc_patients_own_summary_is_served() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bearing(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn another_patient_is_no_data_with_the_typed_refusal_and_nothing_asked() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bearing(summary_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "XCA §4.2.2 No Data: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(
        outcome.pointer("/issue/0/code"),
        Some(&Value::from("not-found"))
    );
    let codes = details(&outcome);
    assert!(
        codes.contains(&String::from("trc-patient-mismatch")),
        "{codes:?}"
    );
    assert!(
        codes.contains(&String::from("ERROR_PS_NOT_FOUND")),
        "RC 10.0.0 §1.25: {codes:?}"
    );
    assert!(
        !text.contains(OTHER),
        "the refusal names no patient: {text}"
    );
    assert!(
        nobody_asked(&a, &b, &pdq).await?,
        "XCA §4.2.1: compared before anything is looked up"
    );
    Ok(())
}

#[tokio::test]
async fn an_unknown_patient_and_another_patient_answer_alike() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let unknown = minted(&treating(NAMESPACE, UNKNOWN))?;
    let (unknown_status, unknown_text) = call(
        app.clone(),
        bearing(summary_of(NAMESPACE, UNKNOWN)?, &unknown)?,
    )
    .await?;
    let mismatch = minted(&treating(NAMESPACE, PATIENT))?;
    let (other_status, other_text) = call(
        app,
        bearing(summary_of(OTHER_NAMESPACE, OTHER)?, &mismatch)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, unknown_status, "{unknown_text}");
    assert_eq!(unknown_status, other_status);
    assert_eq!(
        unknown_text, other_text,
        "nothing tells an unknown patient from another one"
    );
    Ok(())
}

#[tokio::test]
async fn another_patients_document_list_is_no_data() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bearing(documents_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let codes = details(&serde_json::from_str(&text)?);
    assert!(
        codes.contains(&String::from("ERROR_PS_NOT_FOUND")),
        "{codes:?}"
    );
    assert!(nobody_asked(&a, &b, &pdq).await?);
    Ok(())
}

#[tokio::test]
async fn a_token_naming_no_trc_patient_is_trc_missing_with_nothing_asked() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    for claims in [untreated(), {
        let mut malformed = relaying();
        malformed
            .other
            .insert(TRC_CLAIM.to_owned(), String::from("no-system"));
        malformed
    }] {
        let token = minted(&claims)?;
        let (status, text) = call(app.clone(), bearing(summary("")?, &token)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
        let outcome: Value = serde_json::from_str(&text)?;
        let diagnostics = outcome
            .pointer("/issue/0/diagnostics")
            .and_then(Value::as_str)
            .ok_or("diagnostics")?;
        assert!(diagnostics.starts_with("trc-missing"), "{diagnostics}");
    }
    assert!(nobody_asked(&a, &b, &pdq).await?);
    Ok(())
}

#[tokio::test]
async fn the_identification_lookup_needs_no_trc_patient() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&untreated())?;
    let lookup = Request::get(format!(
        "{FHIR}/Patient?identifier={}",
        encoded(NAMESPACE, PATIENT)
    ))
    .body(Body::empty())?;
    let (status, text) = call(app, bearing(lookup, &token)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "XCPD identification precedes the TRC: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_mismatch_is_a_security_event_naming_the_professional_and_pseudonyms() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, _) = call(app, bearing(summary_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
    drop(guard);
    assert_eq!(StatusCode::NOT_FOUND, status);
    let logged = logs.text();
    let line = logged
        .lines()
        .find(|line| line.contains("\"trc-patient-mismatch\""))
        .ok_or_else(|| format!("a security event is recorded: {logged}"))?;
    let event: Value = serde_json::from_str(line)?;
    let fields = event.get("fields").unwrap_or(&event);
    assert_eq!(fields.get("hp_identifier"), Some(&Value::from(HP_ID)));
    let pseudonym = |name: &str| {
        fields
            .get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_default()
    };
    let (asked, bound) = (pseudonym("asked"), pseudonym("bound"));
    for value in [&asked, &bound] {
        assert_eq!(32, value.len(), "a pseudonym: {line}");
        assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()), "{line}");
    }
    assert_ne!(asked, bound, "the two patients are told apart: {line}");
    for withheld in [PATIENT, OTHER] {
        assert!(
            !logged.contains(withheld),
            "SR 5: no patient identifier in the log: {logged}"
        );
    }
    Ok(())
}

// NOTE: no specification governs this: our own design; a TRC patient that cannot be
// resolved names the TRC in the security log, never a `patient/` grant it does not hold.
#[tokio::test]
async fn a_trc_patient_that_cannot_be_resolved_is_its_own_security_event() -> TestResult {
    let (a, b, pdq) = servers().await;
    let failing = Server::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(500))
        .mount(&failing)
        .await;
    let resolver = format!(
        "\n[[pixm.manager]]\nurl = \"{}/fhir/\"\n\n[pixm.manager.members]\n\
         \"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n",
        failing.uri()
    );
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), (&a, &b, &pdq), (&[], &resolver))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, _) = call(app, bearing(summary_of(NAMESPACE, PATIENT)?, &token)?).await?;
    drop(guard);
    let logged = logs.text();
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
    assert!(nobody_asked(&a, &b, &pdq).await?, "nothing is sent");
    Ok(())
}
