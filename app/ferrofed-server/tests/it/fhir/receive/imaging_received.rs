// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An imaging report received on the FHIR face (Regulation (EU) 2025/327
//! Art 14(1)(d), Annex II 2.2 and 2.3), end to end: held to the HL7 Europe
//! Imaging Report 1.0.0-ballot document profiles over the package's
//! dependency closure, mapped by the imaging project `eehrxf` ships over
//! FerroBRIDGE's ADL 2 imaging report template, and written as one
//! composition to the member declared for the category, by its own `ehr_id`
//! alone (§12.4, N23, §5.4.1, N33), with its access record.

use std::error::Error;
use std::path::Path;

use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_testkit::imaging;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::{IMAGING, VERSION_A, creating, gateway_with, issue, post, receiving_adl2, rows};
use crate::facade::{EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire};
use crate::fhir::TestResult;
use crate::support::call;

/// The synthetic imaging report of [`PATIENT`] a radiology system sends,
/// written over FerroBRIDGE's imaging report template.
fn report() -> Result<String, Box<dyn Error>> {
    let written = imaging::template()?;
    let source = Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    }
    .source()?;
    imaging::received_document(&source, &[(NAMESPACE, PATIENT)])
}

/// The gateway receiving imaging reports at node A, the patient known at
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
        (&crossref, &receiving_adl2(dir, &IMAGING, beside)?),
        &settings_with_room(),
    )
}

#[tokio::test]
async fn an_imaging_report_is_written_to_its_member_by_its_ehr_id_alone() -> TestResult {
    let sent = report()?;
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("informational", code);
    for named in ["node-a", "node-a-pub", VERSION_A, "Medical-Imaging"] {
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
        Some(imaging::TEMPLATE)
    );
    let mapped = composition.to_string();
    for value in [
        imaging::CONCLUSION,
        imaging::STUDY_UID,
        imaging::EXAMINATION,
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
async fn an_imaging_report_that_breaks_its_profiles_reaches_no_member() -> TestResult {
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
async fn an_imaging_receipt_is_recorded_once_as_a_creation_of_its_category() -> TestResult {
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
        "eehrxf-document-priority-category-cs|Medical-Imaging:construction",
        "node-a-pub",
        EHR_A,
        VERSION_A,
        "create Bundle",
    ] {
        assert!(record.contains(expected), "{expected}: {record}");
    }
    Ok(())
}

/// `sent` with a `Practitioner` beside the organization that performed its
/// report, as a second performer.
fn performed_also_by_a_practitioner(sent: &str) -> Result<String, Box<dyn Error>> {
    let mut document: Value = serde_json::from_str(sent)?;
    let entries = document
        .get_mut("entry")
        .and_then(Value::as_array_mut)
        .ok_or("the entries")?;
    let practitioner = "http://example.org/fhir/Practitioner/synthetic-radiologist";
    entries.push(serde_json::json!({
        "fullUrl": practitioner,
        "resource": {"resourceType": "Practitioner", "id": "synthetic-radiologist"}
    }));
    let performers = entries
        .iter_mut()
        .find(|entry| {
            entry
                .pointer("/resource/resourceType")
                .and_then(Value::as_str)
                == Some("DiagnosticReport")
        })
        .and_then(|entry| entry.pointer_mut("/resource/performer"))
        .and_then(Value::as_array_mut)
        .ok_or("the report's performers")?;
    performers.push(serde_json::json!({ "reference": practitioner }));
    Ok(document.to_string())
}

/// The expressions of the `incomplete` issues of the outcome `text`.
fn declared(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let outcome: Value = serde_json::from_str(text)?;
    Ok(outcome
        .get("issue")
        .and_then(Value::as_array)
        .ok_or("the issues")?
        .iter()
        .filter(|issue| issue.get("code").and_then(Value::as_str) == Some("incomplete"))
        .filter_map(|issue| issue.pointer("/expression/0").and_then(Value::as_str))
        .map(str::to_owned)
        .collect())
}

// NOTE: hazard I-22 of the clinical safety risk file: what a received report carries into the
// member's structured record, and what it keeps in the original document alone.
#[tokio::test]
async fn a_received_imaging_report_reaches_the_members_composition_and_declares_what_it_loses()
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
    for element in [
        imaging::EXAMINATION,
        imaging::MODALITY,
        imaging::STARTED,
        imaging::OBSERVED,
        imaging::DESCRIPTION,
        imaging::STUDY_UID,
        imaging::CONCLUSION,
        imaging::PERFORMER,
        "openEHR-EHR-CLUSTER.organisation.v1",
    ] {
        assert!(mapped.contains(element), "{element} is carried: {mapped}");
    }
    assert!(
        sent.contains(imaging::REPORT_IDENTIFIER) && !mapped.contains(imaging::REPORT_IDENTIFIER),
        "the report identifier reaches the member in the original document alone: {mapped}"
    );
    let declared = declared(&text)?;
    assert!(
        declared
            .iter()
            .any(|held| held.starts_with("DiagnosticReport.identifier")),
        "the report identifier is relayed to the sender as not carried: {declared:?}"
    );
    for carried in [
        "DiagnosticReport.code",
        "DiagnosticReport.conclusion",
        "DiagnosticReport.performer",
        "ImagingStudy.modality",
    ] {
        assert!(
            !declared.iter().any(|held| held.starts_with(carried)),
            "{carried} is carried, not declared: {declared:?}"
        );
    }
    Ok(())
}

// NOTE: hazard I-22: HL7 Europe Imaging Report 1.0.0-ballot DiagnosticReportEuImaging.performer admits
// a Practitioner beside the organization slice, and the Imaging service slot holds an organisation alone.
#[tokio::test]
async fn a_performer_that_is_no_organization_is_declared_to_the_sender() -> TestResult {
    let sent = performed_also_by_a_practitioner(&report()?)?;
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), (&a, &b), "")?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let declared = declared(&text)?;
    assert!(
        declared.contains(&String::from("DiagnosticReport.performer[1]")),
        "the practitioner is relayed to the sender as not carried: {declared:?}"
    );
    assert!(
        !declared.contains(&String::from("DiagnosticReport.performer[0]")),
        "the organization is carried, not declared: {declared:?}"
    );
    Ok(())
}
