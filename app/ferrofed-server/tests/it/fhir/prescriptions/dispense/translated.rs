// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation coded as a national contact point sends it is translated
//! into the code systems its owner records before it is written, under
//! `[eehrxf.terminology] transcode = "myhealth-eu"` (Implementing Regulation
//! (EU) 2026/2083 Art 3(4)); a concept no exact translation reaches is
//! refused `422` by its element path and system, with nothing written.

use std::error::Error;
use std::path::Path;

use axum::Router;
use ferrofed_testkit::dispensation_codes::{self, ATC, RECORDED, SENT, SNOMED};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{BY_IDENTIFIER, created, creates, dispensation, members, own_endpoint, post};
use crate::facade::{EHR_A, EHR_B};
use crate::fhir::TestResult;
use crate::fhir::prescriptions::{Members, RX_A, RX_B, built};
use crate::fhir::receive::issue;
use crate::support::call;

/// The `[[fhir.serve]]` tables declaring both members for both categories,
/// node A recording SNOMED CT.
const SERVED: &str = "\n[[fhir.serve]]\ncategory = \"Electronic-Prescriptions\"\nmembers = [\"node-a\", \"node-b\"]\n\n\
     [[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [{ member = \"node-a\", systems = [\"http://snomed.info/sct\"] }, \"node-b\"]\n";

/// The gateway over `members`, translating through `terminology`.
fn gateway(members: &Members, dir: &Path, terminology: &Server) -> Result<Router, Box<dyn Error>> {
    let own = format!(
        "{}{}",
        own_endpoint("node-a-mpd", ("node-a", "org-a"), members.proxy_a.origin()),
        own_endpoint("node-b-mpd", ("node-b", "org-b"), members.proxy_b.origin())
    );
    built(
        dir,
        (&members.a.uri(), &members.b.uri()),
        &[("node-a", EHR_A), ("node-b", EHR_B)],
        &own,
        (
            "",
            &format!("{SERVED}{}", dispensation_codes::table(terminology)),
        ),
    )
}

/// The test dispensation with its medicine coded [`SENT`] in ATC.
fn coded() -> String {
    dispensation(BY_IDENTIFIER).replace(
        r#""medicationCodeableConcept":{"text":"Synthetic medicine A"}"#,
        &format!(
            r#""medicationCodeableConcept":{}"#,
            dispensation_codes::medication()
        ),
    )
}

/// The requests the member's own endpoint `server` received for its
/// dispensations, reads and writes alike.
async fn dispensation_requests(server: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(ferrofed_testkit::mock::recorded(server)
        .await?
        .iter()
        .filter(|request| request.url.path() == "/fhir/MedicationDispense")
        .count())
}

#[tokio::test]
async fn a_medicine_coded_in_atc_reaches_the_owner_with_its_snomed_ct_coding() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let terminology = dispensation_codes::start().await;
    let dir = tempfile::tempdir()?;
    let body = coded();
    assert!(body.contains(SENT.0), "the fixture is coded in ATC");
    let (status, text) = call(gateway(&members, dir.path(), &terminology)?, post(&body)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    let writes = creates(&members.own_a).await?;
    let [written] = writes.as_slice() else {
        return Err("one create at the owner".into());
    };
    let written: Value = serde_json::from_slice(written)?;
    let codings: Vec<(&str, &str)> = written
        .pointer("/medicationCodeableConcept/coding")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|coding| {
            Some((
                coding.get("system")?.as_str()?,
                coding.get("code")?.as_str()?,
            ))
        })
        .collect();
    assert_eq!(
        vec![(ATC, SENT.0), (SNOMED, RECORDED.0)],
        codings,
        "2026/2083 Art 3(4): the member's coding after the one sent"
    );
    let sent: Value = serde_json::from_str(&body)?;
    assert_eq!(
        sent.get("identifier"),
        written.get("identifier"),
        "the business identifier the duplicate check reads stays"
    );
    Ok(())
}

#[tokio::test]
async fn a_concept_no_exact_translation_reaches_is_refused_with_nothing_sent_to_the_owner()
-> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let terminology = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"{"resourceType":"Parameters","parameter":[{"name":"result","valueBoolean":false}]}"#,
            "application/fhir+json",
        ))
        .mount(&terminology)
        .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path(), &terminology)?,
        post(&coded())?,
    )
    .await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("code-invalid", code);
    assert!(
        diagnostics.contains("MedicationDispense.medicationCodeableConcept")
            && diagnostics.contains(ATC),
        "the element path and the system: {diagnostics}"
    );
    assert!(!diagnostics.contains(SENT.0), "no code: {diagnostics}");
    assert_eq!(
        0,
        dispensation_requests(&members.own_a).await?,
        "nothing reaches the owner"
    );
    assert_eq!(0, dispensation_requests(&members.own_b).await?);
    Ok(())
}

#[tokio::test]
async fn a_terminology_server_that_refuses_writes_nothing() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let terminology = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/ConceptMap/$translate"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&terminology)
        .await;
    let dir = tempfile::tempdir()?;
    let (status, text) = call(
        gateway(&members, dir.path(), &terminology)?,
        post(&coded())?,
    )
    .await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{text}");
    assert_eq!(0, dispensation_requests(&members.own_a).await?);
    Ok(())
}

#[tokio::test]
async fn a_member_that_names_no_systems_receives_the_body_unchanged() -> TestResult {
    let members = members((&[RX_A], &[RX_B]), created()).await?;
    let terminology = dispensation_codes::start().await;
    let dir = tempfile::tempdir()?;
    let body = coded()
        .replace("SYNTHETIC-RX-WITHHELD-A1", "SYNTHETIC-RX-B1")
        .replace("urn:oid:2.999.5.1", "urn:oid:2.999.5.2");
    let (status, text) = call(gateway(&members, dir.path(), &terminology)?, post(&body)?).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert_eq!(
        vec![body.into_bytes()],
        creates(&members.own_b).await?,
        "node B names no systems, so its body is byte for byte"
    );
    assert!(
        ferrofed_testkit::mock::recorded(&terminology)
            .await?
            .is_empty()
    );
    Ok(())
}
