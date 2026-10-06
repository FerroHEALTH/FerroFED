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
use super::document_with_entry;

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

/// Reads the document with `entry` added, expecting a refusal at
/// `location`, `subject` for a subject element.
fn refused_at(entry: &str, location: &str, subject: bool) -> Result<(), Box<dyn Error>> {
    let error = refused(&document_with_entry(entry)?)?;
    let at = match error {
        ReceiveError::SubjectMismatch { ref location } if subject => location,
        ReceiveError::PatientReference { ref location } if !subject => location,
        ref other => return Err(format!("{other:?}").into()),
    };
    assert_eq!(at, location);
    Ok(())
}

#[test]
fn a_beneficiary_that_names_another_patient_is_refused() -> Result<(), Box<dyn Error>> {
    refused_at(
        r#"{ "fullUrl": "http://example.org/fhir/Coverage/synthetic-coverage", "resource": { "resourceType": "Coverage", "id": "synthetic-coverage", "status": "active", "beneficiary": { "reference": "Patient/synthetic-elsewhere" }, "payor": [{ "display": "Synthetic payer" }] } }"#,
        "Bundle.entry[3].resource.beneficiary",
        true,
    )
}

#[test]
fn a_task_for_another_patient_is_refused() -> Result<(), Box<dyn Error>> {
    refused_at(
        r#"{ "fullUrl": "http://example.org/fhir/Task/synthetic-task", "resource": { "resourceType": "Task", "id": "synthetic-task", "status": "draft", "intent": "order", "for": { "reference": "Patient/synthetic-elsewhere" } } }"#,
        "Bundle.entry[3].resource.for",
        true,
    )
}

#[test]
fn a_subject_by_an_absolute_reference_to_another_server_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r#""patient": { "reference": "https://elsewhere.example.org/fhir/Patient/synthetic-patient" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectMismatch { ref location } if location == "Bundle.entry[2].resource.patient"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_subject_by_a_urn_no_entry_carries_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        ALLERGY_PATIENT,
        r#""patient": { "reference": "urn:uuid:0c3e1d20-0000-4000-8000-000000000999" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectMismatch { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn another_patient_inside_a_backbone_element_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""title": "Synthetic patient summary","#,
        r#""title": "Synthetic patient summary",
        "attester": [{ "mode": "personal", "party": { "reference": "Patient/synthetic-elsewhere" } }],"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::PatientReference { ref location } if location == "Bundle.entry[0].resource.attester[0].party"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn another_patient_inside_a_section_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""entry": [{ "reference": "AllergyIntolerance/synthetic-allergy" }]"#,
        r#""entry": [{ "reference": "AllergyIntolerance/synthetic-allergy" }, { "reference": "Patient/synthetic-elsewhere" }]"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::PatientReference { ref location } if location == "Bundle.entry[0].resource.section[1].entry[1]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn another_patient_in_an_extension_of_a_primitive_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""status": "final","#,
        r#""status": "final",
        "_status": { "extension": [{ "url": "http://example.org/fhir/StructureDefinition/synthetic", "valueReference": { "reference": "Patient/synthetic-elsewhere" } }] },"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::PatientReference { ref location } if location == "Bundle.entry[0].resource._status.extension[0].valueReference"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_reference_typed_patient_by_identifier_alone_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""author": [{ "display": "Synthetic author" }],"#,
        r#""author": [{ "type": "Patient", "identifier": { "system": "urn:oid:2.999.1.665", "value": "synthetic-0666" } }],"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::PatientReference { ref location } if location == "Bundle.entry[0].resource.author[0]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_reference_whose_target_is_unknown_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""author": [{ "display": "Synthetic author" }],"#,
        r#""author": [{ "reference": "urn:uuid:0c3e1d20-0000-4000-8000-000000000999" }],"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::PatientReference { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_reference_whose_type_disagrees_with_its_target_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""author": [{ "display": "Synthetic author" }],"#,
        r#""author": [{ "reference": "Patient/synthetic-patient", "type": "Practitioner" }],"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::ReferenceType { ref location } if location == "Bundle.entry[0].resource.author[0]"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_local_reference_to_no_contained_resource_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""author": [{ "display": "Synthetic author" }],"#,
        r##""author": [{ "reference": "#nowhere" }],"##,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::LocalUnresolved { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn an_author_elsewhere_and_a_contained_author_are_admitted() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""author": [{ "display": "Synthetic author" }],"#,
        r##""contained": [{ "resourceType": "Practitioner", "id": "author" }],
        "author": [{ "reference": "#author" }, { "reference": "Practitioner/synthetic-elsewhere" }, { "reference": "https://elsewhere.example.org/fhir/Organization/synthetic" }],"##,
    )?;
    ReceivedDocument::read(&text)?;
    Ok(())
}

#[test]
fn a_patient_entry_beyond_the_subject_is_refused_whatever_names_it() -> Result<(), Box<dyn Error>> {
    let error = refused(&document_with_entry(
        r#"{ "fullUrl": "urn:uuid:0c3e1d20-0000-4000-8000-000000000668", "resource": { "resourceType": "Patient", "id": "synthetic-unnamed" } }"#,
    )?)?;
    assert!(
        matches!(error, ReceiveError::SeveralPatients { entry: 3 }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_document_with_no_patient_entry_is_refused() -> Result<(), Box<dyn Error>> {
    let start = DOCUMENT
        .find("    {\n      \"fullUrl\": \"http://example.org/fhir/Patient/")
        .ok_or("the Patient entry")?;
    let end = DOCUMENT
        .find("    {\n      \"fullUrl\": \"http://example.org/fhir/AllergyIntolerance/")
        .ok_or("the AllergyIntolerance entry")?;
    let patient = DOCUMENT.get(start..end).ok_or("the Patient text")?;
    let text = DOCUMENT.replacen(patient, "", 1);
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectUnresolved),
        "{error:?}"
    );
    Ok(())
}
