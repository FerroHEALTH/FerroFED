// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Imaging reports the FHIR face writes (Regulation (EU) 2025/327 Art
//! 14(1)(d), Annex II 2.1), end to end over FerroBRIDGE's ADL 2 imaging
//! report template declared in `[[fhir.report]]`: a member's imaging
//! composition listed and read as an HL7 Europe Imaging Report 1.0.0-ballot
//! `BundleReportEuImaging`, the member asked by its own `ehr_id` alone
//! (§5.4.1, N33), and the table refused at load when it cannot work.

use std::error::Error;
use std::path::Path;

use eehrxf::category::Category;
use eehrxf::document::imaging::IMAGING_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_testkit::imaging;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::{UNREACHED_SUPPLIER, fhir_tables_with};
use super::{gateway_with, rows};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire,
};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::support::call;

/// The version of node A's imaging composition, created at node A.
const VERSION: &str = "5b0c3f7e-0000-4000-8000-00000000e1a1::cdr-a.example.org::1";

/// The versioned object of [`VERSION`].
const OBJECT: &str = "5b0c3f7e-0000-4000-8000-00000000e1a1";

/// The `[fhir]` tables of a face that writes imaging reports with
/// `report`, the `[[fhir.report]]` entry.
fn writing(report: &str) -> String {
    fhir_tables_with(
        UNREACHED_SUPPLIER,
        &format!("receive_namespaces = [\"{NAMESPACE}\"]"),
        report,
    )
}

/// The `[[fhir.report]]` entry of the vendored imaging report template.
fn declared() -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string()).to_string();
    let archetypes = imaging::archetype_paths()
        .iter()
        .map(|file| path(file))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n[[fhir.report]]\ncategory = \"Medical-Imaging\"\ntemplate = {}\narchetypes = [{archetypes}]\n",
        path(&imaging::template_path()),
    )
}

/// The gateway writing imaging reports, the patient known at both
/// nodes.
fn gateway(dir: &Path, a: &Server, b: &Server) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &writing(&declared())),
        &settings_with_room(),
    )
}

/// A node answering the imaging report query with `rows` and every other
/// query with none, and the read of its composition with `composition`.
async fn holding(rows: &str, composition: Option<&str>) -> Server {
    let server = Server::start().await;
    let columns = r#"[{"name":"composition","path":"c"},{"name":"uid","path":"c/uid/value"},{"name":"template_id","path":"c/archetype_details/template_id/value"}]"#;
    let answer = |rows: &str| {
        ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"q":"node","columns":{columns},"rows":[{rows}]}}"#).into_bytes(),
            "application/json",
        )
    };
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(body_string_contains(ferrofed_eehrxf::imaging::ARCHETYPE.id))
        .respond_with(answer(rows))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(answer(""))
        .mount(&server)
        .await;
    if let Some(composition) = composition {
        Mock::given(method("GET"))
            .and(path(format!("/v1/ehr/{EHR_A}/composition/{VERSION}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(composition.as_bytes().to_vec(), "application/json"),
            )
            .mount(&server)
            .await;
    }
    server
}

#[tokio::test]
async fn a_members_imaging_composition_is_listed_and_read_as_an_imaging_report() -> TestResult {
    let composition = written_composition()?;
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, imaging::TEMPLATE);
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app.clone(), list("&type=85430-7")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one imaging report listed: {bundle}").into());
    };
    assert_eq!(
        reference.pointer("/type/coding/0/code"),
        Some(&Value::from("85430-7"))
    );
    assert_eq!(
        reference.pointer("/category/0/coding/0/code"),
        Some(&Value::from("Medical-Imaging"))
    );
    assert_eq!(
        reference.pointer("/content/0/format/code"),
        Some(&Value::from(IMAGING_BUNDLE))
    );
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, text) = call(app, binary(url)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read = ReceivedDocument::read(&text)?;
    assert_eq!(Category::Imaging, read.category()?);
    let document: Value = serde_json::from_str(&text)?;
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{OBJECT}"))),
        "the report is named as the list names it"
    );
    let kinds: Vec<&str> = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.pointer("/resource/resourceType")?.as_str())
        .collect();
    for kind in [
        "Composition",
        "Patient",
        "DiagnosticReport",
        "ImagingStudy",
        "Provenance",
    ] {
        assert!(kinds.contains(&kind), "{kind}: {kinds:?}");
    }
    wire(&a).await?.assert_withholds(
        &[PATIENT, PATIENT_TAIL],
        "N33: no patient identifier reaches node A",
    );
    Ok(())
}

#[tokio::test]
async fn an_imaging_composition_is_served_with_the_report_its_study_and_impression() -> TestResult {
    let composition = written_composition()?;
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, imaging::TEMPLATE);
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app.clone(), list("&type=85430-7")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let url = entries(&bundle, "match")
        .first()
        .and_then(|reference| reference.pointer("/content/0/attachment/url"))
        .and_then(Value::as_str)
        .ok_or("an attachment url")?
        .to_owned();
    let (status, text) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let document: Value = serde_json::from_str(&text)?;
    let resources: Vec<&Value> = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .collect();
    let only = |kind: &str| -> Result<&Value, Box<dyn Error>> {
        let found: Vec<&&Value> = resources
            .iter()
            .filter(|resource| resource["resourceType"] == kind)
            .collect();
        let [resource] = found.as_slice() else {
            return Err(format!("one {kind}: {document}").into());
        };
        Ok(resource)
    };
    let report = only("DiagnosticReport")?;
    assert_eq!(report["status"], "final", "{report}");
    assert_eq!(
        report.pointer("/code/coding/0/code"),
        Some(&Value::from(imaging::EXAMINATION)),
        "{report}"
    );
    assert_eq!(report["effectiveDateTime"], imaging::OBSERVED, "{report}");
    assert_eq!(
        report["conclusion"],
        imaging::CONCLUSION,
        "the impression: {report}"
    );
    assert_eq!(
        report.pointer("/identifier/0/value"),
        Some(&Value::from(imaging::REPORT_IDENTIFIER)),
        "{report}"
    );
    let study = only("ImagingStudy")?;
    assert_eq!(study["status"], "available", "{study}");
    assert_eq!(
        study.pointer("/identifier/0/value"),
        Some(&Value::from(imaging::STUDY_UID)),
        "{study}"
    );
    assert_eq!(
        study.pointer("/modality/0/code"),
        Some(&Value::from(imaging::MODALITY)),
        "{study}"
    );
    assert_eq!(study["started"], imaging::STARTED, "{study}");
    assert_eq!(study["description"], imaging::DESCRIPTION, "{study}");
    let study_url = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|entry| entry.pointer("/resource/resourceType") == Some(&Value::from("ImagingStudy")))
        .and_then(|entry| entry["fullUrl"].as_str())
        .ok_or("the study's fullUrl")?;
    let section = only("Composition")?["section"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|section| section.pointer("/code/coding/0/code") == Some(&Value::from("18726-0")))
        .ok_or("the imaging study section")?;
    assert_eq!(
        section.pointer("/entry/0/reference"),
        Some(&Value::from(study_url)),
        "{section}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_silent_at_the_imaging_query_fails_the_list_and_is_named() -> TestResult {
    let a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&a)
        .await;
    let b = holding("", None).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, list("&type=85430-7")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-pub"), "§11.4: {text}");
    Ok(())
}

#[tokio::test]
async fn a_list_of_another_type_asks_no_imaging_query() -> TestResult {
    let (a, b) = (holding("", None).await, holding("", None).await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, list("&type=18748-4")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let queried = received(&a)
        .await?
        .iter()
        .filter(|body| body.contains(ferrofed_eehrxf::imaging::ARCHETYPE.id))
        .count();
    assert_eq!(
        0, queried,
        "the gateway lists its report under 85430-7 alone"
    );
    Ok(())
}

/// The synthetic imaging composition as node A serves it, under
/// [`VERSION`].
fn written_composition() -> Result<String, Box<dyn Error>> {
    let written = imaging::template()?;
    let template = Template::Adl2 {
        template: &written.template,
        archetypes: &written.archetypes,
    };
    imaging::composition(&template.source()?, VERSION)
}

/// The refusal of a gateway written with the `[[fhir.report]]` entry
/// `report`.
async fn refused(report: &str) -> Result<String, Box<dyn Error>> {
    let (a, b) = (holding("", None).await, holding("", None).await);
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    match gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &writing(report)),
        &settings_with_room(),
    ) {
        Ok(_) => Err(format!("accepted: {report}").into()),
        Err(error) => Ok(format!("{error:?}")),
    }
}

#[tokio::test]
async fn a_report_entry_of_a_category_the_gateway_writes_no_report_of_is_refused() -> TestResult {
    let error = refused(&declared().replace("Medical-Imaging", "Patient-Summaries")).await?;
    assert!(error.contains("Category {"), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_template_the_imaging_mapping_does_not_compile_against_is_refused() -> TestResult {
    let opt = ferrofed_testkit::laboratory::opt();
    let report = format!(
        "\n[[fhir.report]]\ncategory = \"Medical-Imaging\"\ntemplate = {}\n",
        toml::Value::String(opt.display().to_string())
    );
    let error = refused(&report).await?;
    assert!(error.contains("Imaging {"), "{error}");
    Ok(())
}

#[tokio::test]
async fn the_written_imaging_list_and_read_are_recorded_under_the_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let composition = written_composition()?;
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, imaging::TEMPLATE);
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    let tables = format!(
        "{}\n{}",
        writing(&declared()),
        audit_tables(&repository, "")
    );
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &tables),
        &settings_with_room(),
    )?;
    let (_, text) = call(app.clone(), list("&type=85430-7")?).await?;
    let bundle: Value = serde_json::from_str(&text)?;
    let url = entries(&bundle, "match")
        .first()
        .and_then(|reference| reference.pointer("/content/0/attachment/url"))
        .and_then(Value::as_str)
        .ok_or("an attachment url")?
        .to_owned();
    let (status, _) = call(app, binary(&url)?).await?;
    assert_eq!(StatusCode::OK, status);
    let records: Vec<String> = repository
        .wait_for(2, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(2, records.len(), "the list and the read: {records:#?}");
    for record in &records {
        for named in ["node-a-pub", EHR_A, "Medical-Imaging"] {
            assert!(record.contains(named), "{named}: {record}");
        }
    }
    Ok(())
}
