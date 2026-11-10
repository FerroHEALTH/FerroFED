// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A discharge report received on the FHIR face (Regulation (EU) 2025/327
//! Art 14(1)(f), Annex II 2.2 and 2.3), end to end: held to the HL7 Europe
//! Hospital Discharge Report 1.0.0 document profiles over the package's
//! dependency closure, mapped by the HDR project `eehrxf` ships over
//! FerroBRIDGE's ADL 2 discharge report template, and written as one
//! composition to the member declared for the category, by its own `ehr_id`
//! alone (§12.4, N23, §5.4.1, N33), with its access record.

use std::error::Error;
use std::path::Path;

use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_testkit::discharge;
use ferrofed_testkit::fhir_packages;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::{Adl2Category, VERSION_A, creating, gateway_with, issue, post, receiving_adl2, rows};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire};
use crate::fhir::TestResult;
use crate::support::call;

/// The `[[fhir.receive]]` entry of a face that receives discharge reports
/// at node A over FerroBRIDGE's ADL 2 template, the mapping written into
/// `dir`, with the tables `beside` after it.
fn receiving(dir: &Path, beside: &str) -> Result<String, Box<dyn Error>> {
    let mapping = dir.join("hdr");
    std::fs::create_dir_all(&mapping)?;
    Ok(receiving_adl2(
        &Adl2Category {
            category: "Discharge-Reports",
            package: discharge::package(),
            dependencies: fhir_packages::discharge_dependencies()?,
            template: discharge::template_path(),
            archetypes: discharge::archetype_paths(),
            files: discharge::mapping_files(&mapping)?,
            context: discharge::CONTEXT,
        },
        beside,
    ))
}

/// The synthetic discharge report of [`PATIENT`] a hospital system sends,
/// written over FerroBRIDGE's discharge report template.
fn report() -> Result<String, Box<dyn Error>> {
    let written = discharge::template()?;
    let source = Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    }
    .source()?;
    discharge::received_document(&source, &[(NAMESPACE, PATIENT)])
}

/// The gateway receiving discharge reports at node A, the patient known at
/// node A, with the tables `beside`.
fn gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    beside: &str,
) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &receiving(dir, beside)?),
        &settings_with_room(),
    )
}

#[tokio::test]
async fn a_discharge_report_is_written_to_its_member_by_its_ehr_id_alone() -> TestResult {
    let sent = report()?;
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("informational", code);
    for named in ["node-a", "node-a-pub", VERSION_A, "Discharge-Reports"] {
        assert!(diagnostics.contains(named), "{named}: {diagnostics}");
    }
    let requests = a.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "one write");
    let write = requests.first().ok_or("the write")?;
    assert_eq!(write.url.path(), format!("/v1/ehr/{EHR_A}/composition"));
    assert_eq!(write.url.query(), None, "N33: no query string");
    wire(&a).await?.assert_withholds_outside_bodies(
        &[PATIENT, PATIENT_TAIL],
        "N33: no identifier in the path or the headers",
    );
    let composition: Value = serde_json::from_slice(&write.body)?;
    assert_eq!(
        composition
            .pointer("/archetype_details/template_id/value")
            .and_then(Value::as_str),
        Some(discharge::TEMPLATE)
    );
    let mapped = composition.to_string();
    for value in [
        discharge::ADMITTED,
        discharge::DISCHARGED,
        discharge::DIAGNOSIS,
        discharge::PROCEDURE,
        discharge::MEDICINE,
    ] {
        assert!(mapped.contains(value), "{value} is mapped: {mapped}");
    }
    assert_eq!(
        composition
            .pointer("/feeder_audit/original_content/value")
            .and_then(Value::as_str),
        Some(sent.as_str()),
        "the received report is kept byte for byte"
    );
    assert!(
        received(&b).await?.is_empty(),
        "N23: no other member is asked"
    );
    Ok(())
}

#[tokio::test]
async fn a_discharge_report_that_breaks_its_profiles_reaches_no_member() -> TestResult {
    // NOTE: FHIR R4 Composition.status binds composition-status as required, and the
    // composition is the document's first entry, so an unknown code breaks its profile.
    let broken = report()?.replacen("\"status\":\"final\"", "\"status\":\"synthetic\"", 1);
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&broken)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(
        a.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "no write reaches the member"
    );
    Ok(())
}

#[tokio::test]
async fn a_discharge_receipt_is_recorded_once_as_a_creation_of_its_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let sent = report()?;
    let (a, b) = (creating().await, Server::start().await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), &audit_tables(&repository, ""))?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let records: Vec<String> = repository
        .wait_for(1, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    let [record] = records.as_slice() else {
        return Err(format!("one access record per receipt: {records:#?}").into());
    };
    for expected in [
        "eehrxf-document-priority-category-cs|Discharge-Reports:construction",
        "node-a-pub",
        EHR_A,
        VERSION_A,
        "create Bundle",
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}

// NOTE: hazard I-21 of the clinical safety risk file: what a full received report carries into
// the member's structured record, and the hospital course it keeps in the original alone.
#[tokio::test]
async fn a_full_received_discharge_report_reaches_the_members_composition_but_its_course()
-> TestResult {
    let sent = report()?;
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let write = requests.first().ok_or("the write")?;
    let mut composition: Value = serde_json::from_slice(&write.body)?;
    let original = composition
        .pointer_mut("/feeder_audit/original_content/value")
        .map(Value::take)
        .ok_or("the original document")?;
    assert_eq!(original.as_str(), Some(sent.as_str()), "kept byte for byte");
    let mapped = composition.to_string();
    let elements = discharge::DIAGNOSES
        .iter()
        .flat_map(|diagnosis| {
            [
                diagnosis.code,
                diagnosis.onset,
                diagnosis.recognised,
                diagnosis.body_site,
                diagnosis.comment,
            ]
        })
        .chain(discharge::PROCEDURES.iter().flat_map(|procedure| {
            [
                procedure.code,
                procedure.performed,
                procedure.body_site,
                procedure.comment,
            ]
        }))
        .chain(
            discharge::MEDICINES
                .iter()
                .flat_map(|medicine| [medicine.code, medicine.directions, medicine.comment]),
        )
        .chain([discharge::ADMITTED, discharge::DISCHARGED]);
    for element in elements {
        assert!(mapped.contains(element), "{element} is carried: {mapped}");
    }
    assert!(
        sent.contains(discharge::HOSPITAL_COURSE) && !mapped.contains(discharge::HOSPITAL_COURSE),
        "the hospital course reaches the member in the original document alone: {mapped}"
    );
    let outcome: Value = serde_json::from_str(&text)?;
    let declared: Vec<&str> = outcome["issue"]
        .as_array()
        .ok_or("the issues")?
        .iter()
        .filter(|issue| issue["code"] == "incomplete")
        .filter_map(|issue| issue.pointer("/expression/0").and_then(Value::as_str))
        .collect();
    for expression in ["Condition.severity", "Condition.verificationStatus.coding"] {
        assert!(
            declared.contains(&expression),
            "{expression} is relayed as not carried: {declared:?}"
        );
    }
    let document: Value = serde_json::from_str(&sent)?;
    let course = document
        .pointer("/entry/0/resource/section")
        .and_then(Value::as_array)
        .ok_or("the sections")?
        .iter()
        .position(|section| {
            section
                .pointer("/code/coding/0/code")
                .and_then(Value::as_str)
                == Some("8648-8")
        })
        .ok_or("the hospital course section")?;
    let section = format!("Composition.section[{course}]");
    assert!(
        declared.contains(&section.as_str()),
        "the hospital course, {section}, is relayed as not carried (eehrxf 0.3.2 HDR accounted): {declared:?}"
    );
    Ok(())
}
