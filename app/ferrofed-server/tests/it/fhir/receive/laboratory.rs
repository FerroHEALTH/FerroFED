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
use super::{VERSION_A, creating, gateway_listing, gateway_with, issue, post, rows, untouched};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, received, settings_with_room, wire,
};
use crate::fhir::TestResult;
use crate::fhir::list::{EHR_SYSTEM, binary, entries, list};
use crate::support::call;
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, ResponseTemplate};

/// The `[fhir]` tables of a face that receives laboratory reports at node
/// A, the mapping written into `dir`.
pub(super) fn receiving(dir: &Path) -> Result<String, Box<dyn Error>> {
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
    wire(&a).await?.assert_withholds_outside_bodies(
        &[PATIENT, PATIENT_TAIL],
        "N33: no identifier in the path or the headers",
    );
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

/// Returns the `DV_QUANTITY` of the canonical composition `value` whose
/// magnitude is `magnitude`.
fn quantity_of(value: &Value, magnitude: f64) -> Option<&Value> {
    match value {
        Value::Object(members) => {
            if value["_type"] == "DV_QUANTITY"
                && value["magnitude"]
                    .as_f64()
                    .is_some_and(|held| held.to_bits() == magnitude.to_bits())
            {
                return Some(value);
            }
            members
                .values()
                .find_map(|member| quantity_of(member, magnitude))
        }
        Value::Array(items) => items.iter().find_map(|item| quantity_of(item, magnitude)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => None,
    }
}

// NOTE: openEHR RM data_types §DV_ORDERED `normal_range`; hazard I-16 of the clinical
// safety risk file: a received range and order reach the member's structured record.
#[tokio::test]
async fn a_received_normal_range_and_order_reach_the_members_composition() -> TestResult {
    let (a, b) = (creating().await, Server::start().await);
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, post(&report()?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(
        !text.contains("no structured element"),
        "nothing is declared lost: {text}"
    );
    let requests = a.received_requests().await.ok_or("recording is on")?;
    let write = requests.first().ok_or("the write")?;
    let composition: Value = serde_json::from_slice(&write.body)?;
    let measured = quantity_of(&composition, 182.0).ok_or("the result's quantity")?;
    let (lower, upper) = laboratory::NORMAL_RANGE;
    for (bound, magnitude) in [("lower", lower), ("upper", upper)] {
        assert_eq!(
            measured["normal_range"][bound]["magnitude"].as_f64(),
            Some(magnitude),
            "{bound}: {measured}"
        );
        assert_eq!(
            measured["normal_range"][bound]["units"], "mg/dL",
            "{measured}"
        );
    }
    let protocol = composition
        .pointer("/content/0/protocol")
        .ok_or("the result's protocol")?;
    assert!(
        protocol.to_string().contains(laboratory::ORDER_IDENTIFIER),
        "the order filler's identifier is held: {protocol}"
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

/// The versioned object node A's laboratory composition is held under.
pub(super) const LAB_OBJECT: &str = "5b0c3f7e-0000-4000-8000-00000000c0a1";

/// The version of node A's laboratory composition, created at node A.
pub(super) const LAB_VERSION: &str = "5b0c3f7e-0000-4000-8000-00000000c0a1::cdr-a.example.org::1";

/// The archetype the laboratory report query selects compositions by.
const LAB_ARCHETYPE: &str = "openEHR-EHR-OBSERVATION.laboratory_test_result.v1";

/// The synthetic report as node A serves it, under [`LAB_VERSION`].
pub(super) fn lab_composition(dir: &Path) -> Result<String, Box<dyn Error>> {
    laboratory::composition(dir, &[(NAMESPACE, PATIENT)], LAB_VERSION)
}

/// A node answering the laboratory report query with `rows` and every other
/// query with none, and the read of its composition with `composition`.
pub(super) async fn holding(rows: &str, composition: Option<&str>) -> Server {
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
        .and(body_string_contains(LAB_ARCHETYPE))
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
            .and(path(format!("/v1/ehr/{EHR_A}/composition/{LAB_VERSION}")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(composition.as_bytes().to_vec(), "application/json"),
            )
            .mount(&server)
            .await;
    }
    server
}

/// Lists node A's laboratory composition and reads it through its
/// attachment, returning the list's reference, the served document and
/// node A.
async fn served(dir: &Path) -> Result<(Value, Value, Server), Box<dyn Error>> {
    let composition = lab_composition(dir)?;
    let row = format!(
        r#"[{composition},"{LAB_VERSION}","{}"]"#,
        laboratory::TEMPLATE
    );
    let (a, b) = (
        holding(&row, Some(&composition)).await,
        holding("", None).await,
    );
    let app = gateway(dir, &a, &b)?;
    let (status, text) = call(app.clone(), list("&type=11502-2")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let bundle: Value = serde_json::from_str(&text)?;
    let matches = entries(&bundle, "match");
    let [reference] = matches.as_slice() else {
        return Err(format!("one laboratory report listed: {bundle}").into());
    };
    let url = reference
        .pointer("/content/0/attachment/url")
        .and_then(Value::as_str)
        .ok_or("an attachment url")?;
    assert!(url.starts_with("Binary/"), "{url}");
    let (status, text) = call(app, binary(url)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let read = eehrxf::receive::ReceivedDocument::read(&text)?;
    assert_eq!(
        eehrxf::category::Category::Laboratory,
        read.category()?,
        "the served document is a laboratory report"
    );
    Ok(((*reference).clone(), serde_json::from_str(&text)?, a))
}

/// The resource of `kind` among the entries of the document `document`.
fn only<'a>(document: &'a Value, kind: &str) -> Result<&'a Value, Box<dyn Error>> {
    let found: Vec<&Value> = document["entry"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("resource"))
        .filter(|resource| resource["resourceType"] == kind)
        .collect();
    let [resource] = found.as_slice() else {
        return Err(format!("one {kind}: {document}").into());
    };
    Ok(resource)
}

#[tokio::test]
async fn a_members_laboratory_composition_is_listed_and_read_as_a_laboratory_report() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let (reference, document, a) = served(dir.path()).await?;
    assert_eq!(
        reference.pointer("/type/coding/0/code"),
        Some(&Value::from("11502-2"))
    );
    assert_eq!(
        reference.pointer("/category/0/coding/0/code"),
        Some(&Value::from("Laboratory-Reports"))
    );
    assert_eq!(
        reference.pointer("/masterIdentifier/value"),
        Some(&Value::from(format!("urn:uuid:{LAB_OBJECT}")))
    );
    assert_eq!(
        document.pointer("/identifier/value"),
        Some(&Value::from(format!("urn:uuid:{LAB_OBJECT}"))),
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
        "Observation",
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

// NOTE: R4 Observation.referenceRange and DiagnosticReport.basedOn; hazard I-18 of the
// clinical safety risk file: a served report keeps the member's normal range and order.
#[tokio::test]
async fn a_served_laboratory_report_carries_the_normal_range_and_the_order() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (_, document, _) = served(dir.path()).await?;
    let result = only(&document, "Observation")?;
    let (low, high) = laboratory::NORMAL_RANGE;
    for (bound, value) in [("low", low), ("high", high)] {
        let quantity = result
            .pointer(&format!("/referenceRange/0/{bound}"))
            .ok_or_else(|| format!("the range's {bound}: {result}"))?;
        assert_eq!(quantity["value"].as_f64(), Some(value), "{quantity}");
        assert_eq!(quantity["code"], "mg/dL", "{quantity}");
    }
    let report = only(&document, "DiagnosticReport")?;
    assert_eq!(
        report.pointer("/basedOn/0/identifier/value"),
        Some(&Value::from(laboratory::ORDER_IDENTIFIER)),
        "the order the report answers: {report}"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_silent_at_the_laboratory_query_fails_the_list_and_is_named() -> TestResult {
    let a = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&a)
        .await;
    let b = holding("", None).await;
    let dir = tempfile::tempdir()?;
    let app = gateway(dir.path(), &a, &b)?;
    let (status, text) = call(app, list("&type=11502-2")?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "§11.3: {text}");
    assert!(text.contains("node-a-pub"), "§11.4: {text}");
    Ok(())
}

#[tokio::test]
async fn a_member_serving_its_own_reports_is_left_out_of_the_laboratory_query() -> TestResult {
    let (a, b) = (holding("", None).await, holding("", None).await);
    let own = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            br#"{"resourceType":"Bundle","type":"searchset"}"#.to_vec(),
            "application/fhir+json",
        ))
        .mount(&own)
        .await;
    let members = format!(
        "\n[[endpoint]]\nid = \"node-a-lab\"\nnode = \"node-a\"\nurl = \"{}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [\"Laboratory-Reports\"]\nprofiles = [\"{}\"]\n\
         ehr_id_system = \"{EHR_SYSTEM}\"\n",
        own.uri(),
        eehrxf::document::laboratory::LAB_BUNDLE
    );
    let crossref = rows(&[
        (NAMESPACE, PATIENT, "node-a", EHR_A),
        (NAMESPACE, PATIENT, "node-b", EHR_B),
    ]);
    let dir = tempfile::tempdir()?;
    let app = gateway_listing(
        dir.path(),
        (&a, &b),
        &members,
        (&crossref, &receiving(dir.path())?),
        &settings_with_room(),
    )?;
    let (status, text) = call(app, list("&type=11502-2")?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let queried = |bodies: Vec<String>| {
        bodies
            .iter()
            .filter(|body| body.contains(LAB_ARCHETYPE))
            .count()
    };
    assert_eq!(0, queried(received(&a).await?), "node A serves its own");
    assert_eq!(1, queried(received(&b).await?), "node B is mapped");
    assert_eq!(
        1,
        own.received_requests().await.unwrap_or_default().len(),
        "node A is asked at its own endpoint"
    );
    Ok(())
}
