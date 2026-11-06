// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A dispensation of a prescription a map-mode member owns (A102 point 2b):
//! mapped into one composition whose `ACTION` names the order's
//! `OBJECT_VERSION_ID`, created once at the owner by its own `ehr_id` alone
//! (ITS-REST 1.1.0 `composition_create`; §5.4.1, N33); a dispensation the
//! owner already keeps is refused with nothing written, the owner's refusal
//! is `409` and its silence `504`.

use std::error::Error;
use std::time::Duration;

use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::dispense::{DISPENSATION_SYSTEM, DISPENSATION_VALUE, post};
use super::mapped::{OWN_UID, gateway_with, order, query_api, served_with};
use super::{RX_B, own, page, searchset};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, wire};
use crate::fhir::TestResult;
use crate::support::call;

/// The archetype the medication action query selects compositions by.
const ACTION: &str = "openEHR-EHR-ACTION.medication.v1";

/// The business identifier of node A's mapped prescription.
const PRESCRIPTION: &str = "synthetic-rx-own";

/// A dispensation of node A's mapped prescription, by its identifier, with
/// its patient and its product contained.
fn dispensation() -> String {
    format!(
        r##"{{"resourceType":"MedicationDispense","meta":{{"profile":["{MPD_MEDICATION_DISPENSE}"]}},
  "extension":[{{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-10-16T10:00:00Z"}}],
  "contained":[{{"resourceType":"Patient","id":"pat","identifier":[{{"system":"{NAMESPACE}","value":"{PATIENT}"}}]}},
    {{"resourceType":"Medication","id":"med","meta":{{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd"]}},
      "code":{{"coding":[{{"system":"http://example.org/fhir/CodeSystem/ferrofed-products","code":"SYN-PRODUCT-0001","display":"Synthetic tablet 500 mg"}}]}}}}],
  "identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}],
  "status":"completed","medicationReference":{{"reference":"#med"}},
  "subject":{{"reference":"#pat"}},
  "performer":[{{"actor":{{"display":"Synthetic pharmacist"}}}}],
  "authorizingPrescription":[{{"identifier":{{"system":"{}","value":"{PRESCRIPTION}"}}}}],
  "quantity":{{"value":20,"unit":"tablet","system":"http://unitsofmeasure.org","code":"{{tbl}}"}},
  "whenHandedOver":"2026-10-16T10:05:00Z"}}"##,
        ferrofed_testkit::medication::PRESCRIPTION_SYSTEM
    )
}

/// The `[[fhir.serve]]` tables declaring node A in the map mode for both
/// categories, with the language and territory its compositions are
/// written in.
fn served() -> String {
    served_with(
        ", language = \"en\", territory = \"NL\"",
        "\n[[fhir.serve]]\ncategory = \"Electronic-Dispensations\"\nmembers = [\"node-a\"]\n",
    )
}

/// Node A's Query API holding its own order, answering the action query
/// with `actions`, and creating a composition with `created`.
async fn node_a(actions: &str, created: ResponseTemplate) -> Result<Server, Box<dyn Error>> {
    let a = query_api(&[(order(OWN_UID, PRESCRIPTION)?, OWN_UID)]).await;
    let columns = r#"[{"name":"composition","path":"c"},{"name":"uid","path":"c/uid/value"},{"name":"template_id","path":"c/archetype_details/template_id/value"}]"#;
    let body = format!(r#"{{"q":"node","columns":{columns},"rows":[{actions}]}}"#);
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(ACTION))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body.into_bytes(), "application/json"),
        )
        .mount(&a)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/v1/ehr/{EHR_A}/composition")))
        .respond_with(created)
        .mount(&a)
        .await;
    Ok(a)
}

/// A composition created under the version `uid`.
fn created(uid: &str) -> ResponseTemplate {
    ResponseTemplate::new(201)
        .insert_header("ETag", format!("\"{uid}\"").as_str())
        .insert_header(
            "Location",
            format!("https://cdr-a.example.org/ehr/{EHR_A}/composition/{uid}").as_str(),
        )
}

/// The compositions node A received to create.
async fn writes(a: &Server) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut written = Vec::new();
    for request in a.received_requests().await.unwrap_or_default() {
        if request.method == wiremock::http::Method::POST
            && request.url.path() == format!("/v1/ehr/{EHR_A}/composition")
        {
            written.push(serde_json::from_slice(&request.body)?);
        }
    }
    Ok(written)
}

/// The response to the test dispensation at the gateway over `a`.
async fn dispensed(a: &Server) -> Result<(StatusCode, String), Box<dyn Error>> {
    let b = Server::start().await;
    let own_b = own(page(searchset("https://cdr-b.example.org", &[RX_B], None))).await;
    let dir = tempfile::tempdir()?;
    let router = gateway_with(dir.path(), (a, &b, &own_b), &served())?;
    call(router, post(&dispensation())?).await
}

#[tokio::test]
async fn a_dispensation_of_a_mapped_prescription_is_one_composition_at_its_owner_naming_the_order()
-> TestResult {
    let version = "9e1d2a3b-0000-4000-8000-0000000000d1::cdr-a.example.org::1";
    let a = node_a("", created(version)).await?;
    let (status, text) = dispensed(&a).await?;
    assert_eq!(StatusCode::CREATED, status, "{text}");
    assert!(
        text.contains(version),
        "the outcome names the version: {text}"
    );
    let written = writes(&a).await?;
    let [composition] = written.as_slice() else {
        return Err(format!("one composition written, got {}", written.len()).into());
    };
    let instructed = composition
        .pointer("/content/0/instruction_details/instruction_id/id/value")
        .or_else(|| composition.pointer("/content/1/instruction_details/instruction_id/id/value"));
    assert_eq!(
        instructed.and_then(Value::as_str),
        Some(OWN_UID),
        "RM INSTRUCTION_DETAILS.instruction_id: the order's version"
    );
    let kept = composition
        .pointer("/feeder_audit/original_content/value")
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        kept.contains(DISPENSATION_VALUE),
        "the received dispensation is kept"
    );
    for request in a.received_requests().await.unwrap_or_default() {
        let path = request.url.path();
        if path.starts_with("/v1/ehr/") {
            assert!(path.contains(EHR_A), "N33: the owner's ehr_id: {path}");
        }
    }
    wire(&a).await?.assert_withholds_outside_bodies(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier in the path, the query or a header",
    );
    Ok(())
}

#[tokio::test]
async fn a_dispensation_the_owner_already_keeps_is_refused_with_nothing_written() -> TestResult {
    let kept = format!(
        r#"{{"resourceType":"Bundle","type":"collection","entry":[{{"resource":{{"resourceType":"MedicationDispense","identifier":[{{"system":"{DISPENSATION_SYSTEM}","value":"{DISPENSATION_VALUE}"}}]}}}}]}}"#
    );
    let action = serde_json::json!({
        "_type": "COMPOSITION",
        "feeder_audit": {"original_content": {"_type": "DV_PARSABLE", "value": kept, "formalism": "application/fhir+json"}}
    });
    let uid = "9e1d2a3b-0000-4000-8000-0000000000d0::cdr-a.example.org::1";
    let row = format!(
        r#"[{action},"{uid}","{}"]"#,
        ferrofed_testkit::medication::TEMPLATE
    );
    let a = node_a(&row, created(uid)).await?;
    let (status, text) = dispensed(&a).await?;
    assert_eq!(StatusCode::CONFLICT, status, "I-12: {text}");
    assert!(text.contains("duplicate"), "{text}");
    assert!(writes(&a).await?.is_empty(), "nothing is written");
    Ok(())
}

#[tokio::test]
async fn the_owners_refusal_is_a_conflict_naming_it() -> TestResult {
    let a = node_a("", ResponseTemplate::new(422)).await?;
    let (status, text) = dispensed(&a).await?;
    assert_eq!(StatusCode::CONFLICT, status, "{text}");
    assert!(text.contains("node-a-pub"), "the owner is named: {text}");
    assert_eq!(1, writes(&a).await?.len(), "sent once");
    Ok(())
}

#[tokio::test]
async fn an_owner_that_does_not_answer_is_asked_once_and_the_outcome_is_unknown() -> TestResult {
    let a = node_a(
        "",
        created("x::cdr-a.example.org::1").set_delay(Duration::from_secs(5)),
    )
    .await?;
    let (status, text) = dispensed(&a).await?;
    assert_eq!(StatusCode::GATEWAY_TIMEOUT, status, "{text}");
    assert!(text.contains("unknown"), "{text}");
    Ok(())
}
