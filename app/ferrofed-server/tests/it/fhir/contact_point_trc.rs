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
use crate::auth::minted;
use crate::contact_point::{HP_ID, bound, with};
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
    server.auth = crate::contact_point::declared();
    Ok(ferrofed_server::router(state, &server))
}

/// The contact point's claims, its TRC binding `system`|`value`.
fn treating(system: &str, value: &str) -> Claims {
    with("patient.id", Value::String(format!("{system}|{value}")))
}

/// The contact point's claims with no TRC patient.
fn untreated() -> Claims {
    crate::contact_point::without("patient.id")
}

/// The claims of the discovery-phase token: the identity assertion's
/// fields alone, no TRC and no patient (eHDSI SAML Profile 10.0.0 §4: the
/// TRC follows patient identification).
fn discovery() -> Claims {
    let mut claims = untreated();
    for field in ["trc.id", "trc.not_before", "trc.not_on_or_after"] {
        claims.ehdsi.remove(field);
    }
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
    let (status, text) = call(app, bound(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

#[tokio::test]
async fn another_patient_is_no_data_with_the_typed_refusal_and_nothing_asked() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bound(summary_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
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
        bound(summary_of(NAMESPACE, UNKNOWN)?, &unknown)?,
    )
    .await?;
    let mismatch = minted(&treating(NAMESPACE, PATIENT))?;
    let (other_status, other_text) =
        call(app, bound(summary_of(OTHER_NAMESPACE, OTHER)?, &mismatch)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, unknown_status, "{unknown_text}");
    assert_eq!(unknown_status, other_status);
    assert_eq!(
        unknown_text, other_text,
        "nothing tells an unknown patient from another one"
    );
    Ok(())
}

/// eHDSI XCA Profile 8.0.1 §4.2.2, through the whole stack: the error layer
/// outside the gate reads the caller from the mark the gate sets on its
/// answer, so a contact point's `404` for a patient no member holds is the
/// no-data answer, and the same `404` to a caller that is no contact point
/// is not (#1691).
#[tokio::test]
async fn only_a_contact_points_not_found_is_the_no_data_answer() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let relayed = minted(&treating(NAMESPACE, UNKNOWN))?;
    let (status, text) = call(
        app.clone(),
        bound(summary_of(NAMESPACE, UNKNOWN)?, &relayed)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let codes = details(&serde_json::from_str(&text)?);
    assert!(
        codes.contains(&String::from("trc-patient-mismatch"))
            && codes.contains(&String::from("ERROR_PS_NOT_FOUND")),
        "the no-data answer: {codes:?}"
    );
    let own = minted(&crate::support::caller::claims())?;
    let (status, text) = call(
        app,
        crate::auth::bearing(summary_of(NAMESPACE, UNKNOWN)?, &own)?,
    )
    .await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "{text}");
    let codes = details(&serde_json::from_str(&text)?);
    assert!(
        !codes.contains(&String::from("trc-patient-mismatch"))
            && !codes.contains(&String::from("ERROR_PS_NOT_FOUND")),
        "no no-data answer for a caller that is no contact point: {codes:?}"
    );
    Ok(())
}

#[tokio::test]
async fn another_patients_document_list_is_no_data() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bound(documents_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
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
    for claims in [
        untreated(),
        with("patient.id", Value::String(String::from("no-system"))),
    ] {
        let token = minted(&claims)?;
        let (status, text) = call(app.clone(), bound(summary("")?, &token)?).await?;
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
    let (status, text) = call(app, bound(lookup, &token)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "XCPD identification precedes the TRC: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn the_discovery_phase_token_reaches_the_lookup_and_no_patient_data() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&discovery())?;
    let (status, text) = call(app.clone(), bound(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    let outcome: Value = serde_json::from_str(&text)?;
    let diagnostics = outcome
        .pointer("/issue/0/diagnostics")
        .and_then(Value::as_str)
        .ok_or("diagnostics")?;
    assert!(diagnostics.starts_with("trc-missing"), "{diagnostics}");
    assert!(nobody_asked(&a, &b, &pdq).await?);
    let lookup = Request::get(format!(
        "{FHIR}/Patient?identifier={}",
        encoded(NAMESPACE, PATIENT)
    ))
    .body(Body::empty())?;
    let (status, text) = call(app, bound(lookup, &token)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the discovery phase identifies the patient: {text}"
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
    let (status, _) = call(app, bound(summary_of(OTHER_NAMESPACE, OTHER)?, &token)?).await?;
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
    let (status, _) = call(app, bound(summary_of(NAMESPACE, PATIENT)?, &token)?).await?;
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

/// MyHealth@EU RC 10.0.0 §1.25 requirement 11.01.02: a contact point's
/// request with no TRC patient misses an assertion attribute, so it is
/// refused with the `trc-missing` refusal and
/// `ERROR_SEC_DATA_INTEGRITY_NOT_ENSURED` in the outcome's details, its
/// national connector's code, and no legacy appendix 6.4 number.
#[tokio::test]
async fn trc_missing_names_the_data_integrity_code() -> TestResult {
    let (a, b, pdq) = servers().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&untreated())?;
    let (status, text) = call(app, bound(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            String::from("trc-missing"),
            String::from("ERROR_SEC_DATA_INTEGRITY_NOT_ENSURED")
        ],
        details(&outcome),
        "{text}"
    );
    Ok(())
}

/// MyHealth@EU RC 10.0.0 §1.25 requirement 05.01: members that answer the
/// summary's questions with an error are the national infrastructure that
/// cannot answer, `ERROR_REGISTRY_NOT_AVAILABLE` in the outcome's details.
#[tokio::test]
async fn failing_members_name_the_registry_not_available_code() -> TestResult {
    let failing = || async {
        let server = Server::start().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        server
    };
    let (a, b, pdq) = (failing().await, failing().await, supplier().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b, &pdq))?;
    let token = minted(&treating(NAMESPACE, PATIENT))?;
    let (status, text) = call(app, bound(summary("")?, &token)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    let outcome: Value = serde_json::from_str(&text)?;
    assert!(
        details(&outcome).contains(&String::from("ERROR_REGISTRY_NOT_AVAILABLE")),
        "{text}"
    );
    Ok(())
}
