// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Discharge reports the FHIR face writes (Regulation (EU) 2025/327 Art
//! 14(1)(f), Annex II 2.1), end to end over FerroBRIDGE's ADL 2 discharge
//! report template declared in `[[fhir.report]]`: a member's discharge
//! composition listed and read as an HL7 Europe Hospital Discharge Report
//! 1.0.0 `bundle-eu-hdr`, the member asked by its own `ehr_id` alone
//! (§5.4.1, N33), and the table refused at load when it cannot work.

use std::error::Error;
use std::path::Path;

use eehrxf::category::Category;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::receive::ReceivedDocument;
use ferrofed_testkit::discharge;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pdqm_stub;
use http::StatusCode;
use serde_json::Value;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::super::{BIRTH_DATE, FAMILY, GIVEN, UNREACHED_SUPPLIER, fhir_tables_with, supplier};
use super::{gateway_with, rows};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire,
};
use crate::fhir::TestResult;
use crate::fhir::list::{binary, entries, list};
use crate::support::call;

/// The version of node A's discharge composition, created at node A.
const VERSION: &str = "5b0c3f7e-0000-4000-8000-00000000d1a1::cdr-a.example.org::1";

/// The versioned object of [`VERSION`].
const OBJECT: &str = "5b0c3f7e-0000-4000-8000-00000000d1a1";

/// The `[fhir]` tables of a face that writes discharge reports with
/// `report`, the `[[fhir.report]]` entry, the header asked of the PDQm
/// Supplier at `pdq`.
fn writing(pdq: &str, report: &str) -> String {
    fhir_tables_with(
        pdq,
        &format!("receive_namespaces = [\"{NAMESPACE}\"]"),
        report,
    )
}

/// The `[[fhir.report]]` entry of the vendored discharge report template.
fn declared() -> String {
    let path = |value: &Path| toml::Value::String(value.display().to_string()).to_string();
    let archetypes = discharge::archetype_paths()
        .iter()
        .map(|file| path(file))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\n[[fhir.report]]\ncategory = \"Discharge-Reports\"\ntemplate = {}\narchetypes = [{archetypes}]\n",
        path(&discharge::template_path()),
    )
}

/// The gateway writing discharge reports, the patient known at both
/// nodes, the header asked of the PDQm Supplier `pdq`.
fn gateway(
    dir: &Path,
    (a, b): (&Server, &Server),
    pdq: &Server,
) -> Result<axum::Router, Box<dyn Error>> {
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    gateway_with(
        dir,
        (a, b),
        (&crossref, &writing(&pdqm_stub::base_url(pdq), &declared())),
        &settings_with_room(),
    )
}

/// A node answering the discharge report query with `rows` and every other
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
        .and(body_string_contains(
            ferrofed_eehrxf::discharge::COMPOSITION,
        ))
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
async fn a_members_discharge_composition_is_listed_and_read_as_a_discharge_report() -> TestResult {
    let composition = discharge::composition(VERSION);
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, discharge::TEMPLATE);
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app.clone(), list("&type=18842-5")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one discharge report listed: {bundle}").into());
    };
    assert_eq!(
        reference.pointer("/type/coding/0/code"),
        Some(&Value::from("18842-5"))
    );
    assert_eq!(
        reference.pointer("/category/0/coding/0/code"),
        Some(&Value::from("Discharge-Reports"))
    );
    assert_eq!(
        reference.pointer("/content/0/format/code"),
        Some(&Value::from(HDR_BUNDLE))
    );
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    let (status, text) = call(app, binary(url)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read = ReceivedDocument::read(&text)?;
    assert_eq!(Category::Discharge, read.category()?);
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
        "Encounter",
        "Condition",
        "Procedure",
        "MedicationRequest",
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

/// The document the gateway serves from node A's `composition`, listed
/// and read through the signed URL.
async fn served(dir: &Path, composition: &str) -> Result<Value, Box<dyn Error>> {
    let pdq = supplier().await;
    let (status, text) = read_with(dir, composition, &pdqm_stub::base_url(&pdq)).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// The answer to the read of node A's `composition`, listed and read
/// through the signed URL, the header asked of the PDQm Supplier at `pdq`.
async fn read_with(
    dir: &Path,
    composition: &str,
    pdq: &str,
) -> Result<(StatusCode, String), Box<dyn Error>> {
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, discharge::TEMPLATE);
    let (a, b) = (
        holding(&row, Some(composition)).await,
        holding("", None).await,
    );
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    let app = gateway_with(
        dir,
        (&a, &b),
        (&crossref, &writing(pdq, &declared())),
        &settings_with_room(),
    )?;
    let (status, text) = call(app.clone(), list("&type=18842-5")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let url = entries(&bundle, "match")
        .first()
        .and_then(|reference| reference.pointer("/content/0/attachment/url"))
        .and_then(Value::as_str)
        .ok_or("an attachment url")?
        .to_owned();
    call(app, binary(&url)?).await
}

/// The resources the section of `document`'s `Composition` coded `code`
/// lists, each found by its `fullUrl`.
fn listed<'a>(document: &'a Value, code: &str) -> Result<Vec<&'a Value>, Box<dyn Error>> {
    let entries = document["entry"].as_array().ok_or("entries")?;
    let composition = entries
        .first()
        .and_then(|entry| entry.get("resource"))
        .ok_or("the Composition")?;
    let section = composition["section"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|section| section.pointer("/code/coding/0/code") == Some(&Value::from(code)))
        .ok_or_else(|| format!("the section {code}: {composition}"))?;
    section["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|reference| {
            let url = reference["reference"].as_str().ok_or("a reference")?;
            entries
                .iter()
                .find(|entry| entry["fullUrl"] == url)
                .and_then(|entry| entry.get("resource"))
                .ok_or_else(|| format!("{url} is in the document").into())
        })
        .collect()
}

/// The resource of `resources` whose `code` (or `medicationCodeableConcept`
/// at `pointer`) carries the coding `code`.
fn coded<'a>(
    resources: &[&'a Value],
    pointer: &str,
    code: &str,
) -> Result<&'a Value, Box<dyn Error>> {
    resources
        .iter()
        .copied()
        .find(|resource| resource.pointer(pointer) == Some(&Value::from(code)))
        .ok_or_else(|| format!("{code} is listed: {resources:?}").into())
}

#[tokio::test]
async fn a_full_discharge_composition_is_served_with_every_section_and_entry() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = served(dir.path(), &discharge::full_composition(VERSION)).await?;
    let diagnoses = listed(&document, "11535-2")?;
    assert_eq!(discharge::DIAGNOSES.len(), diagnoses.len(), "{document}");
    for expected in discharge::DIAGNOSES {
        let condition = coded(&diagnoses, "/code/coding/0/code", expected.code)?;
        assert_eq!(condition["resourceType"], "Condition", "{condition}");
        assert_eq!(condition["onsetDateTime"], expected.onset, "{condition}");
        assert_eq!(
            condition.pointer("/extension/0/valueDateTime"),
            Some(&Value::from(expected.recognised)),
            "the date recognised: {condition}"
        );
        assert_eq!(
            condition.pointer("/bodySite/0/text"),
            Some(&Value::from(expected.body_site)),
            "{condition}"
        );
        assert_eq!(
            condition.pointer("/note/0/text"),
            Some(&Value::from(expected.comment)),
            "{condition}"
        );
    }
    let procedures = listed(&document, "10185-7")?;
    assert_eq!(discharge::PROCEDURES.len(), procedures.len(), "{document}");
    for expected in discharge::PROCEDURES {
        let procedure = coded(&procedures, "/code/coding/0/code", expected.code)?;
        assert_eq!(procedure["resourceType"], "Procedure", "{procedure}");
        assert_eq!(procedure["status"], "completed", "{procedure}");
        assert_eq!(
            procedure["performedDateTime"], expected.performed,
            "{procedure}"
        );
        assert_eq!(
            procedure.pointer("/bodySite/0/text"),
            Some(&Value::from(expected.body_site)),
            "{procedure}"
        );
        assert_eq!(
            procedure.pointer("/note/0/text"),
            Some(&Value::from(expected.comment)),
            "{procedure}"
        );
    }
    let medicines = listed(&document, "75311-1")?;
    assert_eq!(discharge::MEDICINES.len(), medicines.len(), "{document}");
    for expected in discharge::MEDICINES {
        let request = coded(
            &medicines,
            "/medicationCodeableConcept/coding/0/code",
            expected.code,
        )?;
        assert_eq!(request["resourceType"], "MedicationRequest", "{request}");
        assert_eq!(
            request.pointer("/dosageInstruction/0/text"),
            Some(&Value::from(expected.directions)),
            "{request}"
        );
        assert_eq!(
            request.pointer("/note/0/text"),
            Some(&Value::from(expected.comment)),
            "{request}"
        );
    }
    let stay = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .find(|resource| resource["resourceType"] == "Encounter")
        .ok_or("the stay")?;
    assert_eq!(
        stay.pointer("/period/start"),
        Some(&Value::from(discharge::ADMITTED))
    );
    assert_eq!(
        stay.pointer("/period/end"),
        Some(&Value::from(discharge::DISCHARGED))
    );
    Ok(())
}

#[tokio::test]
async fn the_served_patient_carries_the_header_the_demographics_binding_holds() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = served(dir.path(), &discharge::composition(VERSION)).await?;
    let patient = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .find(|resource| resource["resourceType"] == "Patient")
        .ok_or("the Patient")?;
    // NOTE: HL7 Europe Base 2.0.1 patient-eu-core sets Patient.name and Patient.birthDate 1..1.
    assert_eq!(
        patient.pointer("/name/0/family"),
        Some(&Value::from(FAMILY))
    );
    assert_eq!(
        patient.pointer("/name/0/given/0"),
        Some(&Value::from(GIVEN))
    );
    assert_eq!(patient["birthDate"], BIRTH_DATE, "{patient}");
    Ok(())
}

#[tokio::test]
async fn a_report_whose_demographics_binding_does_not_answer_is_refused_and_not_written()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let (status, text) = read_with(
        dir.path(),
        &discharge::composition(VERSION),
        UNREACHED_SUPPLIER,
    )
    .await?;
    assert_eq!(StatusCode::BAD_GATEWAY, status, "{text}");
    assert!(text.contains("OperationOutcome"), "{text}");
    assert!(
        !text.contains("\"Patient\""),
        "no document is written: {text}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_silent_at_the_discharge_query_fails_the_list_and_is_named() -> TestResult {
    let a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&a)
        .await;
    let b = holding("", None).await;
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app, list("&type=18842-5")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-pub"), "§11.4: {text}");
    Ok(())
}

#[tokio::test]
async fn a_list_of_another_type_asks_no_discharge_query() -> TestResult {
    let (a, b) = (holding("", None).await, holding("", None).await);
    let dir = tempfile::tempdir()?;
    let pdq = supplier().await;
    let app = gateway(dir.path(), (&a, &b), &pdq)?;
    let (status, text) = call(app, list("&type=100719-4")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let queried = received(&a)
        .await?
        .iter()
        .filter(|body| body.contains(ferrofed_eehrxf::discharge::COMPOSITION))
        .count();
    assert_eq!(
        0, queried,
        "the gateway lists its report under 18842-5 alone"
    );
    Ok(())
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
        (&crossref, &writing(UNREACHED_SUPPLIER, report)),
        &settings_with_room(),
    ) {
        Ok(_) => Err(format!("accepted: {report}").into()),
        Err(error) => Ok(format!("{error:?}")),
    }
}

#[tokio::test]
async fn a_report_entry_of_a_category_the_gateway_writes_no_report_of_is_refused() -> TestResult {
    let error = refused(&declared().replace("Discharge-Reports", "Patient-Summaries")).await?;
    assert!(error.contains("Category {"), "{error}");
    Ok(())
}

#[tokio::test]
async fn a_template_the_discharge_mapping_does_not_compile_against_is_refused() -> TestResult {
    let opt = ferrofed_testkit::laboratory::opt();
    let report = format!(
        "\n[[fhir.report]]\ncategory = \"Discharge-Reports\"\ntemplate = {}\n",
        toml::Value::String(opt.display().to_string())
    );
    let error = refused(&report).await?;
    assert!(error.contains("Discharge {"), "{error}");
    Ok(())
}

#[tokio::test]
async fn the_written_discharge_list_and_read_are_recorded_under_the_category() -> TestResult {
    use crate::feed_audit::{SETTLE, audit_tables};
    use ferrofed_testkit::atna_feed::FeedRepository;

    let composition = discharge::composition(VERSION);
    let row = format!(r#"[{composition},"{VERSION}","{}"]"#, discharge::TEMPLATE);
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
    let pdq = supplier().await;
    let tables = format!(
        "{}\n{}",
        writing(&pdqm_stub::base_url(&pdq), &declared()),
        audit_tables(&repository, "")
    );
    let app = gateway_with(
        dir.path(),
        (&a, &b),
        (&crossref, &tables),
        &settings_with_room(),
    )?;
    let (_, text) = call(app.clone(), list("&type=18842-5")?).await?;
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
        // NOTE: PDQm §2:3.78.5.1; the header's ITI-78 exchange the read asks is audited too.
        .wait_for(3, SETTLE)
        .await
        .into_iter()
        .filter(|record| record.contains("ehds-categories"))
        .collect();
    assert_eq!(2, records.len(), "the list and the read: {records:#?}");
    for record in &records {
        for named in ["node-a-pub", EHR_A, "Discharge-Reports"] {
            assert!(record.contains(named), "{named}: {record}");
        }
    }
    Ok(())
}
