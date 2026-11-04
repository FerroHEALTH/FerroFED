// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A laboratory report received on the FHIR face (Regulation (EU) 2025/327
//! Art 14(1)(e), Annex II 2.2 and 2.3), end to end: held to the HL7 Europe
//! Laboratory Report 2.0.0 document profiles, mapped by the laboratory
//! mapping `eehrxf` ships over `KDS_Laborbericht`, and written as one
//! composition to the member declared for the category, by its own
//! `ehr_id` alone (§12.4, N23, §5.4.1, N33).

use std::error::Error;
use std::path::Path;

use ferrofed_testkit::fhir_packages;
use ferrofed_testkit::laboratory;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;

use super::super::{UNREACHED_SUPPLIER, fhir_tables_with};
use super::write::composed;
use super::{VERSION_A, creating, gateway_with, issue, post, rows, untouched};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room};
use crate::fhir::TestResult;
use crate::support::call;

/// The `[fhir]` tables of a face that receives laboratory reports at node
/// A, the mapping written into `dir`.
fn receiving(dir: &Path) -> Result<String, Box<dyn Error>> {
    let path = |value: &Path| toml::Value::String(value.display().to_string()).to_string();
    let mapping = dir.join("laboratory");
    std::fs::create_dir_all(&mapping)?;
    let files = laboratory::mapping_files(&mapping)?
        .iter()
        .map(|file| path(file))
        .collect::<Vec<_>>()
        .join(", ");
    let dependencies = fhir_packages::dependencies()?
        .iter()
        .map(|dependency| path(dependency))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(fhir_tables_with(
        UNREACHED_SUPPLIER,
        &format!("receive_namespaces = [\"{NAMESPACE}\"]"),
        &format!(
            "\n[[fhir.receive]]\ncategory = \"Laboratory-Reports\"\nmember = \"node-a\"\n\
             package = {}\ndependencies = [{dependencies}]\ntemplate = {}\n\
             files = [{files}]\ncontext = \"{}\"\nlanguage = \"en\"\nterritory = \"NL\"\n",
            path(&laboratory::package()),
            path(&laboratory::opt()),
            laboratory::context()?,
        ),
    ))
}

/// The gateway receiving laboratory reports at node A, the patient known at
/// both nodes.
fn gateway(dir: &Path, a: &Server, b: &Server) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &receiving(dir)?),
        &settings_with_room(),
    )
}

/// The synthetic laboratory report of [`PATIENT`].
fn report() -> Result<String, Box<dyn Error>> {
    laboratory::received_document(&[(NAMESPACE, PATIENT)])
}

#[tokio::test]
async fn a_laboratory_report_is_written_to_its_member_by_its_ehr_id_alone() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let sent = report()?;
    let (status, text) = call(app, post(&sent)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("informational", code);
    for named in ["node-a", "node-a-pub", VERSION_A, "Laboratory-Reports"] {
        assert!(diagnostics.contains(named), "{named}: {diagnostics}");
    }
    let requests = a.received_requests().await.ok_or("recording is on")?;
    assert_eq!(1, requests.len(), "one write");
    let write = requests.first().ok_or("the write")?;
    assert_eq!(write.url.path(), format!("/v1/ehr/{EHR_A}/composition"));
    assert_eq!(write.url.query(), None, "N33: no query string");
    let wire = composed(&a).await?;
    for withheld in [PATIENT, PATIENT_TAIL] {
        assert!(
            !wire.contains(withheld),
            "N33: no identifier in the path or the headers: {wire}"
        );
    }
    let composition: Value = serde_json::from_slice(&write.body)?;
    assert_eq!(
        composition
            .pointer("/archetype_details/template_id/value")
            .and_then(Value::as_str),
        Some(laboratory::TEMPLATE)
    );
    let mapped = composition.to_string();
    for value in [
        laboratory::ANALYTE,
        laboratory::CONCLUSION,
        laboratory::REPORT_IDENTIFIER,
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
async fn a_laboratory_report_that_breaks_its_profiles_reaches_no_member() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    // NOTE: FHIR R4 Composition.status binds composition-status as required, and the
    // composition is the document's first entry, so an unknown code breaks its profile.
    let broken = report()?.replacen("\"status\":\"final\"", "\"status\":\"synthetic\"", 1);
    let (status, text) = call(app, post(&broken)?).await?;
    assert_eq!(StatusCode::UNPROCESSABLE_ENTITY, status, "{text}");
    assert!(
        text.contains("Composition.status"),
        "the broken element: {text}"
    );
    assert!(untouched(&a, &b).await?, "nothing reaches a member");
    Ok(())
}

#[tokio::test]
async fn a_laboratory_report_of_a_patient_no_member_holds_is_refused() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let other = laboratory::received_document(&[(NAMESPACE, "SENTINEL-NOBODY-3k9q")])?;
    let (status, text) = call(app, post(&other)?).await?;
    assert_eq!(
        StatusCode::UNPROCESSABLE_ENTITY,
        status,
        "§5.2: never written under a guess: {text}"
    );
    let (code, diagnostics) = issue(&text)?;
    assert_eq!("not-found", code);
    assert!(diagnostics.contains("node-a"), "{diagnostics}");
    assert!(
        !text.contains("SENTINEL-NOBODY-3k9q"),
        "§5.4.3: never echoed: {text}"
    );
    assert!(untouched(&a, &b).await?, "nothing reaches a member");
    Ok(())
}
