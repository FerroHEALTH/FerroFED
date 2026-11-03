// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The summary's patient: the header the identity binding holds, an element
//! it does not hold left absent, and the members that serve their own summary
//! named in every section.

use std::error::Error;

use ferrofed_eehrxf::summary::error::AssemblyError;
use ferrofed_eehrxf::summary::{Answer, Covered, DATA_ABSENT_REASON, Elsewhere};
use ferrofed_identity::role::header::{PatientHeader, PersonName};
use ferrofed_testkit::eps;

use super::{answers, document, document_beside, document_headed, header, organisation, section};

/// The `Patient` entry of `bundle`.
fn patient(bundle: &serde_json::Value) -> Option<&serde_json::Value> {
    bundle["entry"]
        .as_array()?
        .iter()
        .map(|entry| &entry["resource"])
        .find(|resource| resource["resourceType"] == "Patient")
}

#[test]
fn the_header_is_the_one_the_identity_binding_holds() -> Result<(), Box<dyn Error>> {
    let assembled = document(&answers(&[], &Answer::Answered(Vec::new())))?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    ferrofed_testkit::emit::document("ferrofed-eehrxf-summary-header", &text)?;
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let patient = patient(&bundle).ok_or("a Patient")?;
    assert_eq!(
        patient["name"],
        serde_json::json!([{
            "use": "official",
            "family": "SYNTHETIC-FAMILY",
            "given": ["SYNTHETIC-GIVEN"]
        }]),
        "eHN PS A.1.1.2, A.1.1.3"
    );
    assert_eq!(patient["birthDate"], "1970-01-01", "eHN PS A.1.1.4");
    assert_eq!(patient["gender"], "unknown", "eHN PS A.1.1.5");
    assert_eq!(
        patient["address"],
        serde_json::json!([{
            "line": ["1 Synthetic Street"],
            "city": "Synthetic City",
            "country": "NL"
        }]),
        "eHN PS A.1.2.1"
    );
    assert_eq!(
        patient["telecom"],
        serde_json::json!([{
            "system": "email",
            "value": "patient@example.org",
            "use": "home"
        }]),
        "eHN PS A.1.2.1.8"
    );
    assert_eq!(
        patient["identifier"][0]["value"], "synthetic-subject-689",
        "the identifier the summary was asked by"
    );
    Ok(())
}

#[test]
fn an_element_the_binding_does_not_hold_is_absent_never_invented() -> Result<(), Box<dyn Error>> {
    let named = PatientHeader {
        names: vec![PersonName {
            text: Some(String::from("SYNTHETIC NAME")),
            ..PersonName::default()
        }],
        ..PatientHeader::default()
    };
    let assembled = document_headed(&answers(&[], &Answer::Answered(Vec::new())), named)?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    ferrofed_testkit::emit::document("ferrofed-eehrxf-summary-name-only", &text)?;
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    let patient = patient(&bundle).ok_or("a Patient")?;
    assert!(
        patient["birthDate"].is_null(),
        "no birth date is written: {patient}"
    );
    assert_eq!(
        patient["_birthDate"]["extension"],
        serde_json::json!([{ "url": DATA_ABSENT_REASON, "valueCode": "unknown" }]),
        "eHN PS Art 10(5): the required birth date is stated unknown"
    );
    for absent in ["gender", "address", "telecom"] {
        assert!(patient[absent].is_null(), "{absent} is left out: {patient}");
    }
    Ok(())
}

/// Node C, which serves its own patient summary, as its endpoint answered.
fn node_c(answer: Covered) -> Elsewhere {
    Elsewhere {
        endpoint: String::from("node-c-eps"),
        organisation: organisation("org-c"),
        answer,
    }
}

#[test]
fn a_member_serving_its_own_summary_is_named_in_every_section_and_mapped_in_none()
-> Result<(), Box<dyn Error>> {
    let assembled = document_beside(
        &answers(&[], &Answer::Answered(Vec::new())),
        header(),
        &[node_c(Covered::Document)],
    )?;
    let text = serde_json::to_string(&assembled.bundle)?;
    let findings = eps::profile::check(&text)?;
    assert!(findings.is_empty(), "{findings:#?}");
    ferrofed_testkit::emit::document("ferrofed-eehrxf-summary-beside-own-summary", &text)?;
    let bundle: serde_json::Value = serde_json::from_str(&text)?;
    for code in ["48765-2", "11450-4", "10160-0"] {
        let held = section(&bundle, code).ok_or("the section")?;
        let div = held["text"]["div"].as_str().ok_or("a narrative")?;
        assert!(
            div.contains("Not mapped here: the records of Synthetic org-c"),
            "the member is named as covered elsewhere: {div}"
        );
        assert_eq!(
            held["emptyReason"]["coding"][0]["code"], "unavailable",
            "a section cannot know that a member it did not map holds nothing"
        );
    }
    assert!(!text.contains("org-c\"}"), "the member authors nothing");
    Ok(())
}

#[test]
fn a_member_serving_no_summary_of_the_patient_is_named_as_holding_none()
-> Result<(), Box<dyn Error>> {
    let assembled = document_beside(
        &answers(&[], &Answer::Answered(Vec::new())),
        header(),
        &[node_c(Covered::Absent)],
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let allergies = section(&bundle, "48765-2").ok_or("the allergies section")?;
    let div = allergies["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(div.contains("holds none for this patient"), "{div}");
    Ok(())
}

#[test]
fn a_silent_member_serving_its_own_summary_is_named_as_giving_no_answer()
-> Result<(), Box<dyn Error>> {
    let assembled = document_beside(
        &answers(&[], &Answer::Answered(Vec::new())),
        header(),
        &[node_c(Covered::Silent {
            status: String::from("time-out"),
        })],
    )?;
    let bundle = serde_json::to_value(&assembled.bundle)?;
    let allergies = section(&bundle, "48765-2").ok_or("the allergies section")?;
    let div = allergies["text"]["div"].as_str().ok_or("a narrative")?;
    assert!(
        div.contains("No answer from: Synthetic org-c (time-out)."),
        "§11.1: {div}"
    );
    assert!(!div.contains("Not mapped here"), "{div}");
    Ok(())
}

#[test]
fn a_header_with_no_name_writes_no_document() -> Result<(), Box<dyn Error>> {
    let unnamed = PatientHeader {
        names: vec![PersonName {
            purpose: Some(String::from("official")),
            ..PersonName::default()
        }],
        birth_date: Some(String::from("1970-01-01")),
        ..PatientHeader::default()
    };
    let refused = document_headed(&answers(&[], &Answer::Answered(Vec::new())), unnamed);
    let error = refused.err().ok_or("a document with no name was written")?;
    assert!(
        matches!(
            error.downcast_ref::<AssemblyError>(),
            Some(AssemblyError::Unnamed)
        ),
        "EPS ips-pat-1: {error}"
    );
    Ok(())
}
