// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A national contact point's reads by id and dispensations held to the
//! patient of its treatment relationship confirmation (eHDSI XCA Profile
//! 8.0.1 §4.2.2: a document "assigned to another patient" is answered "No
//! Data", written to the audit trail and raised as a security alert;
//! §4.2.1: the request's patient "equals the patient identifier given in the
//! TRC assertion").
//!
//! A prescription or dispensation read by an id that names another patient's
//! EHR, its version read, a withdrawal of it, and a dispensation whose subject
//! is another patient each get the `404` an unknown patient gets, with
//! `trc-patient-mismatch` and `ERROR_EP_NOT_FOUND` (Requirements Catalogue
//! 10.0.0 §1.25), reach no member, and are recorded as a security event. A
//! read inside the TRC patient's EHR is served.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::issuer::Claims;
use http::{Request, StatusCode, header};
use serde_json::Value;

use super::dispense::{
    BY_IDENTIFIER, SERVED_BOTH, created, creates, dispensation, members, own_endpoint, post,
};
use super::withdraw::withdrawal;
use super::{AT_A, Members, RX_A, RX_B, face_id};
use crate::auth::minted;
use crate::contact_point::{HP_ID, bound, with};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, settings_with_room};
use crate::fhir::receive::{OTHER, OTHER_A, OTHER_NAMESPACE, rows};
use crate::fhir::{FHIR, TestResult, UNREACHED_SUPPLIER, text_registered};
use crate::support::{Logs, call};

/// A patient identifier the cross-reference knows at no member.
const UNKNOWN: &str = "SENTINEL-UNKNOWN-9t2k";

/// The gateway over `members`, both serving prescriptions and dispensations,
/// the test patient known at both and [`OTHER`] at node A, trusting the
/// test contact point.
fn gateway(members: &Members, dir: &Path) -> Result<Router, Box<dyn Error>> {
    gateway_with(members, dir, "")
}

/// [`gateway`], its configuration followed by the tables `tables`.
pub(super) fn gateway_with(
    members: &Members,
    dir: &Path,
    tables: &str,
) -> Result<Router, Box<dyn Error>> {
    let own = format!(
        "{}{}",
        own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
        own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
    );
    let others = rows(&[(OTHER_NAMESPACE, OTHER, "node-a", OTHER_A)]);
    let text = text_registered(
        dir,
        (&members.a.uri(), &members.b.uri(), UNREACHED_SUPPLIER),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own,
        ("", &format!("{SERVED_BOTH}{others}")),
    )?;
    let text = format!("{text}{tables}");
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    // A store opens its connections blocking, which a multi-threaded runtime takes off
    // its worker; the gateway without one is built in place, under any runtime.
    let build = || AppState::build(&settings);
    let state = Arc::new(if tables.is_empty() {
        build()?
    } else {
        tokio::task::block_in_place(build)?
    });
    let mut server = settings_with_room();
    server.auth = crate::contact_point::declared();
    Ok(ferrofed_server::router(state, &server))
}

/// The members, node A holding [`RX_A`] and node B [`RX_B`], each creating
/// a dispensation.
async fn holding() -> Result<Members, Box<dyn Error>> {
    members((&[RX_A], &[RX_B]), created()).await
}

/// The contact point's token, its TRC binding `system`|`value`.
fn treating(system: &str, value: &str) -> Result<String, Box<dyn Error>> {
    let claims: Claims = with("patient.id", Value::String(format!("{system}|{value}")));
    minted(&claims)
}

/// `GET` of `path` under the FHIR base.
fn get(path: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("{FHIR}/{path}")).body(Body::empty())
}

/// The face's id of `resource` in [`OTHER`]'s EHR at node A.
fn others(resource: &str) -> Result<String, Box<dyn Error>> {
    face_id("node-a-mpd", OTHER_A, resource)
}

/// The no-data answer the contact point gets for a patient no member
/// holds: its status and body.
async fn unknown_answer(app: Router) -> Result<(StatusCode, String), Box<dyn Error>> {
    let search = get(&format!(
        "MedicationRequest?patient.identifier={}%7C{UNKNOWN}",
        NAMESPACE.replace(':', "%3A")
    ))?;
    call(app, bound(search, &treating(NAMESPACE, UNKNOWN)?)?).await
}

/// The codes of the first issue's `details` of the outcome `text`.
fn codes(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    Ok(outcome
        .pointer("/issue/0/details/coding")
        .and_then(Value::as_array)
        .map(|codings| {
            codings
                .iter()
                .filter_map(|coding| coding.get("code").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default())
}

/// Holds `(status, text)` to the no-data answer of the prescription
/// service, byte for byte the answer `unknown` an unknown patient got.
fn assert_no_data(
    what: &str,
    (status, text): &(StatusCode, String),
    unknown: &(StatusCode, String),
) -> TestResult {
    assert_eq!(StatusCode::NOT_FOUND, *status, "{what}: {text}");
    let found = codes(text)?;
    for code in ["trc-patient-mismatch", "ERROR_EP_NOT_FOUND"] {
        assert!(found.iter().any(|found| found == code), "{what}: {found:?}");
    }
    assert_eq!(
        unknown,
        &(*status, text.clone()),
        "{what}: as an unknown patient"
    );
    assert!(!text.contains(OTHER), "{what} names no patient: {text}");
    Ok(())
}

// conformance: track-10
#[tokio::test]
async fn a_read_by_id_of_another_patients_prescription_is_no_data_recorded_and_sent_nowhere()
-> TestResult {
    let members = holding().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let unknown = unknown_answer(app.clone()).await?;
    let logs = Logs::default();
    let capture = crate::support::console::subscriber(
        ferrohealth_telemetry::section::Rendering::Json,
        "debug",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let token = treating(NAMESPACE, PATIENT)?;
    let answer = call(
        app,
        bound(
            get(&format!("MedicationRequest/{}", others("rx-o1")?))?,
            &token,
        )?,
    )
    .await?;
    drop(guard);
    assert_no_data("the read", &answer, &unknown)?;
    assert!(
        members.unasked(),
        "XCA §4.2.2: no document of another patient is fetched"
    );
    let logged = logs.text();
    let line = logged
        .lines()
        .find(|line| line.contains("\"trc-patient-mismatch\""))
        .ok_or_else(|| format!("XCA §4.2.2: a security alert is recorded: {logged}"))?;
    assert!(line.contains("\"resource\""), "it names a resource: {line}");
    assert!(line.contains(HP_ID), "it names the professional: {line}");
    for withheld in [OTHER_A, PATIENT, OTHER] {
        assert!(
            !logged.contains(withheld),
            "SR 5: pseudonymised, no {withheld}: {logged}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_read_by_id_inside_the_trc_patients_ehr_is_served() -> TestResult {
    let members = holding().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let id = face_id(AT_A.0, AT_A.1, "rx-a1")?;
    let token = treating(NAMESPACE, PATIENT)?;
    let (status, text) = call(
        app,
        bound(get(&format!("MedicationRequest/{id}"))?, &token)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(text.contains("\"MedicationRequest\""), "{text}");
    Ok(())
}

#[tokio::test]
async fn every_read_and_withdrawal_of_another_patients_resource_is_no_data() -> TestResult {
    let members = holding().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let unknown = unknown_answer(app.clone()).await?;
    let token = treating(NAMESPACE, PATIENT)?;
    let (prescription, dispensed) = (others("rx-o1")?, others("md-1")?);
    let withdraw = Request::put(format!("{FHIR}/MedicationDispense/{dispensed}"))
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .body(Body::from(withdrawal(&dispensed)))?;
    for (what, request) in [
        (
            "a prescription's version read",
            get(&format!("MedicationRequest/{prescription}/_history/1"))?,
        ),
        (
            "a dispensation's read",
            get(&format!("MedicationDispense/{dispensed}"))?,
        ),
        (
            "a dispensation's version read",
            get(&format!("MedicationDispense/{dispensed}/_history/1"))?,
        ),
        ("a withdrawal", withdraw),
    ] {
        let answer = call(app.clone(), bound(request, &token)?).await?;
        assert_no_data(what, &answer, &unknown)?;
    }
    assert!(
        members.unasked(),
        "no member is asked for another patient's resource"
    );
    Ok(())
}

#[tokio::test]
async fn a_dispensation_for_another_patient_is_no_data_and_writes_nothing() -> TestResult {
    let members = holding().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let unknown = unknown_answer(app.clone()).await?;
    let token = treating(NAMESPACE, PATIENT)?;
    let body = dispensation(BY_IDENTIFIER)
        .replace(NAMESPACE, OTHER_NAMESPACE)
        .replace(PATIENT, OTHER);
    let answer = call(app, bound(post(&body)?, &token)?).await?;
    assert_no_data("the dispensation", &answer, &unknown)?;
    assert!(members.unasked(), "no member is asked to look up or write");
    assert!(
        creates(&members.own_a).await?.is_empty() && creates(&members.own_b).await?.is_empty(),
        "nothing is written"
    );
    Ok(())
}

#[tokio::test]
async fn a_dispensation_for_the_trc_patient_is_written() -> TestResult {
    let members = holding().await?;
    let dir = tempfile::tempdir()?;
    let app = gateway(&members, dir.path())?;
    let token = treating(NAMESPACE, PATIENT)?;
    let (status, text) = call(app, bound(post(&dispensation(BY_IDENTIFIER))?, &token)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(
        1,
        creates(&members.own_a).await?.len(),
        "written at its owner"
    );
    Ok(())
}
