// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The MyHealth@EU national contact point API `DocumentReference` search
//! parameters the
//! gateway evaluates on every entry it lists (`myhealth.eu.fhir.ncp-api`
//! 9.1.1-ci, `CapabilityStatementMyHealthEu`; #1975): each one selecting a
//! member's own imaging report it describes and excluding it otherwise,
//! repetitions all required, a malformed value refused with nothing sent,
//! and the parameters IHE MHD 4.2.2 ITI-67 defines passed to the member as
//! a narrowing the gateway evaluates again (#1982).

use eehrxf::document::imaging::IMAGING_BUNDLE;
use http::StatusCode;
use serde_json::Value;

use super::super::TestResult;
use super::own_member::{Declared, Members};
use super::{entries, list};
use crate::facade::PATIENT_TAIL;
use crate::support::call;

/// Both imaging document types, percent-encoded.
const IMAGING_TYPE: &str =
    "&type=http%3A%2F%2Floinc.org%7C85430-7,http%3A%2F%2Floinc.org%7C18748-4";

/// The member's own imaging report, with every element a parameter reads.
const DOCUMENT: &str = r#"{"category":[{"coding":[{"code":"Medical-Imaging","system":"http://hl7.eu/fhir/health-data-api/CodeSystem/eehrxf-document-priority-category-cs"}]}],"content":[{"attachment":{"contentType":"application/pdf","creation":"2026-10-20T09:30:00Z","size":63,"url":"Binary/img-1"}}],"context":{"period":{"start":"2026-10-01","end":"2026-10-05"}},"date":"2026-10-21T08:00:00Z","extension":[{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.modality","valueCodeableConcept":{"coding":[{"code":"MR","system":"http://dicom.nema.org/resources/ontology/DCM"}]}},{"extension":[{"url":"concept","valueCodeableConcept":{"coding":[{"code":"72696002","system":"http://snomed.info/sct"}]}}],"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-DocumentReference.bodySite"}],"id":"doc-img-1","masterIdentifier":{"system":"urn:ietf:rfc:3986","value":"urn:uuid:1a900000-0000-4000-8000-0000000000b1"},"resourceType":"DocumentReference","status":"current","subject":{"reference":"Patient/pat-a"},"type":{"coding":[{"code":"85430-7","system":"http://loinc.org"}]}}"#;

/// Node A's own endpoint, serving imaging reports.
const IMAGING: Declared = Declared {
    endpoint: "node-a-img",
    category: "Medical-Imaging",
    profile: IMAGING_BUNDLE,
    binary: "img-1",
    document: "doc-img-1",
    kind: None,
};

/// Whether the list `query` asks for names the member's report.
async fn names_the_report(
    members: &Members,
    query: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    let (status, text) = call(app, list(&format!("{IMAGING_TYPE}&{query}"))?).await?;
    assert_eq!(StatusCode::OK, status, "{query}: {text}");
    let bundle: Value = serde_json::from_str(&text)?;
    Ok(entries(&bundle, "match").iter().any(|reference| {
        reference.pointer("/masterIdentifier/value")
            == Some(&Value::from(
                "urn:uuid:1a900000-0000-4000-8000-0000000000b1",
            ))
    }))
}

#[tokio::test]
async fn each_parameter_selects_the_report_it_describes() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    for query in [
        "category=http%3A%2F%2Fhl7.eu%2Ffhir%2Fhealth-data-api%2FCodeSystem%2Feehrxf-document-priority-category-cs%7CMedical-Imaging",
        "contenttype=application%2Fpdf",
        "status=current,superseded",
        "modality=http%3A%2F%2Fdicom.nema.org%2Fresources%2Fontology%2FDCM%7CMR",
        "bodysite=72696002",
        "creation=2026-10-20",
        "date=ge2026-10-21",
        "period=2026-10",
        "status=current&modality=MR",
    ] {
        assert!(names_the_report(&members, query).await?, "{query}");
    }
    Ok(())
}

#[tokio::test]
async fn each_parameter_excludes_a_report_it_does_not_describe() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    for query in [
        "category=Laboratory-Reports",
        "contenttype=image%2Fpng",
        "status=superseded",
        "modality=CT",
        "bodysite=http%3A%2F%2Fsnomed.info%2Fsct%7C1",
        "creation=lt2026-10-20",
        "date=2026-10-22",
        "period=sa2026-10-02",
        "status=current&modality=CT",
    ] {
        assert!(!names_the_report(&members, query).await?, "{query}");
    }
    Ok(())
}

#[tokio::test]
async fn a_malformed_value_or_a_modifier_is_refused_and_nothing_is_sent() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let dir = tempfile::tempdir()?;
    let app = members.gateway(dir.path(), ("", ""))?;
    for query in [
        "date=2026-13-01",
        "period=xx2026",
        "contenttype=a%7Cb%7Cc",
        "status=%7C",
        "status:not=current",
        "category:text=imaging",
    ] {
        let (status, text) = call(app.clone(), list(&format!("&{query}"))?).await?;
        assert_eq!(StatusCode::BAD_REQUEST, status, "{query}: {text}");
        let outcome: Value = serde_json::from_str(&text)?;
        assert_eq!(outcome["resourceType"], "OperationOutcome", "{text}");
        assert!(!text.contains(PATIENT_TAIL), "{text}");
    }
    assert!(members.proxy.journal().is_empty(), "nothing is sent");
    Ok(())
}

#[tokio::test]
async fn the_narrowing_reaches_the_member_and_the_gateway_still_decides() -> TestResult {
    let members = Members::holding(IMAGING, DOCUMENT).await?;
    let query = "category=Medical-Imaging&creation=ge2026-10-01&date=ge2026-10-21&period=2026-10\
                 &status=superseded&modality=MR&contenttype=application%2Fpdf&bodysite=72696002";
    assert!(
        !names_the_report(&members, query).await?,
        "the member answers its current report whatever it is asked, and status=superseded excludes it"
    );
    let asked = members.asked();
    let (_, search) = asked
        .iter()
        .find(|(path, _)| path == "/fhir/DocumentReference")
        .ok_or("the member's DocumentReference search")?;
    for (name, value) in [
        ("category", "Medical-Imaging"),
        ("creation", "ge2026-10-01"),
        ("date", "ge2026-10-21"),
        ("period", "2026-10"),
        ("status", "superseded"),
    ] {
        assert!(
            search.contains(&(name.to_owned(), value.to_owned())),
            "{name}={value} reaches the member: {search:?}"
        );
    }
    for name in [
        "modality",
        "contenttype",
        "bodysite",
        "patient.identifier",
        "_id",
    ] {
        assert!(
            !search.iter().any(|(asked, _)| asked == name),
            "ITI-67 defines no {name}, or it names the patient: {search:?}"
        );
    }
    Ok(())
}
