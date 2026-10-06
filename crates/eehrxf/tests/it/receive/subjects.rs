// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one patient of a received document: every `subject` and `patient`
//! reference names it, no other entry or contained resource is a `Patient`,
//! and every `fullUrl` agrees with its resource (R4 `Bundle`, `bdl-7`,
//! `bdl-8`, `Bundle.entry.fullUrl`).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::receive::ReceiveError;
use eehrxf::receive::ReceivedDocument;

use super::DOCUMENT;
use super::PATIENT_IDENTIFIER;
use super::document_with;

/// Reads `text`, expecting a refusal that quotes no patient identifier.
fn refused(text: &str) -> Result<ReceiveError, Box<dyn Error>> {
    let error = ReceivedDocument::read(text)
        .err()
        .ok_or("the document was read")?;
    assert!(
        !error.to_string().contains(PATIENT_IDENTIFIER),
        "the refusal quotes the patient identifier: {error}"
    );
    Ok(error)
}

/// Returns the synthetic document with every `(from, to)` replaced once.
fn with_all(replacements: &[(&str, &str)]) -> Result<String, Box<dyn Error>> {
    let mut text = DOCUMENT.to_owned();
    for (from, to) in replacements {
        if text.matches(from).count() != 1 {
            return Err(format!("{from:?} is not in the document exactly once").into());
        }
        text = text.replacen(from, to, 1);
    }
    Ok(text)
}

/// The allergy's patient reference in the synthetic document.
const ALLERGY_PATIENT: &str = r#""patient": { "reference": "Patient/synthetic-patient" }"#;

#[test]
fn an_entry_that_names_another_patient_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r#""patient": { "reference": "Patient/synthetic-elsewhere" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectMismatch { ref location } if location == "Bundle.entry[2].resource.patient"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_entry_that_names_its_patient_by_identifier_alone_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r#""patient": { "identifier": { "system": "urn:oid:2.999.1.665", "value": "synthetic-0666" } }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectMismatch { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_entry_that_names_another_resource_of_the_document_as_its_patient_is_refused()
-> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r#""patient": { "reference": "Composition/synthetic-composition" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectMismatch { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_contained_patient_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r##""contained": [{ "resourceType": "Patient", "id": "shadow" }],
        "patient": { "reference": "#shadow" }"##,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::ContainedPatient { ref location } if location == "Bundle.entry[2].resource.contained[0]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_second_patient_entry_is_refused() -> Result<(), Box<dyn Error>> {
    let second = r#",
    {
      "fullUrl": "http://example.org/fhir/Patient/synthetic-elsewhere",
      "resource": { "resourceType": "Patient", "id": "synthetic-elsewhere" }
    }
  ]
}"#;
    let end = DOCUMENT.rfind("\n  ]\n}").ok_or("the end of the entries")?;
    let text = format!("{}{second}", DOCUMENT.get(..end).ok_or("the entries")?);
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SeveralPatients { entry: 3 }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn every_form_of_a_reference_to_the_one_patient_is_admitted() -> Result<(), Box<dyn Error>> {
    let text = with_all(&[
        (
            r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient","#,
            r#""fullUrl": "urn:uuid:0c3e1d20-0000-4000-8000-000000000667","#,
        ),
        (
            r#""subject": { "reference": "Patient/synthetic-patient" }"#,
            r#""subject": { "reference": "urn:uuid:0c3e1d20-0000-4000-8000-000000000667" }"#,
        ),
        (
            ALLERGY_PATIENT,
            r#""patient": { "reference": "urn:uuid:0c3e1d20-0000-4000-8000-000000000667" }"#,
        ),
    ])?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.patient().id.as_deref(), Some("synthetic-patient"));
    Ok(())
}

#[test]
fn a_relative_reference_does_not_resolve_against_a_urn() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient","#,
        r#""fullUrl": "urn:uuid:0c3e1d20-0000-4000-8000-000000000667","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectUnresolved),
        "{error:?}"
    );
    Ok(())
}

// bdl-8: "fullUrl cannot be a version specific reference".
#[test]
fn a_version_specific_full_url_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient","#,
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient/_history/1","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::VersionedFullUrl { entry: 1 }),
        "{error:?}"
    );
    Ok(())
}

// bdl-7: "FullUrl must be unique in a bundle".
#[test]
fn a_repeated_full_url_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy","#,
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::DuplicateFullUrl { entry: 2 }),
        "{error:?}"
    );
    Ok(())
}

// Bundle.entry.fullUrl: "The fullUrl SHALL NOT disagree with the id in the resource".
#[test]
fn a_full_url_that_disagrees_with_its_resource_id_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-patient","#,
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-other","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::FullUrlMismatch { entry: 1 }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_full_url_that_names_another_resource_type_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy","#,
        r#""fullUrl": "http://example.org/fhir/Patient/synthetic-allergy","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::FullUrlMismatch { entry: 2 }),
        "{error:?}"
    );
    Ok(())
}
