// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading a received document: the R4 decode, the document rules of the R4
//! `Bundle` (`bdl-9`, `bdl-10`, `bdl-11`), the subject, and a refusal for
//! each.

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

/// Reads `text`, expecting a refusal.
fn refused(text: &str) -> Result<ReceiveError, Box<dyn Error>> {
    match ReceivedDocument::read(text) {
        Ok(_) => Err("the document was read".into()),
        Err(error) => {
            assert!(
                !error.to_string().contains(PATIENT_IDENTIFIER),
                "the refusal quotes the patient identifier: {error}"
            );
            Ok(error)
        }
    }
}

#[test]
fn a_document_is_read_with_its_original_text_kept() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    assert_eq!(
        document.original(),
        DOCUMENT,
        "the text is kept byte for byte"
    );
    assert_eq!(
        document.composition().title.value.as_deref(),
        Some("Synthetic patient summary")
    );
    assert_eq!(document.patient().id.as_deref(), Some("synthetic-patient"));
    assert_eq!(document.bundle().entry.len(), 3);
    Ok(())
}

#[test]
fn a_subject_given_as_the_patient_entry_full_url_resolves() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" }"#,
        r#""subject": { "reference": "http://example.org/fhir/Patient/synthetic-patient" }"#,
    )?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(document.patient().id.as_deref(), Some("synthetic-patient"));
    Ok(())
}

#[test]
fn text_that_is_not_json_is_refused() -> Result<(), Box<dyn Error>> {
    let error = refused("{ not json")?;
    assert!(matches!(error, ReceiveError::Json { .. }), "{error:?}");
    Ok(())
}

#[test]
fn json_that_is_not_an_object_is_refused() -> Result<(), Box<dyn Error>> {
    let error = refused("[]")?;
    assert!(matches!(error, ReceiveError::NotAnObject), "{error:?}");
    Ok(())
}

#[test]
fn a_resource_that_is_not_a_bundle_is_refused() -> Result<(), Box<dyn Error>> {
    let error = refused(r#"{"resourceType":"Patient","id":"synthetic-patient"}"#)?;
    assert!(matches!(error, ReceiveError::NotABundle), "{error:?}");
    Ok(())
}

#[test]
fn a_property_r4_does_not_define_is_refused_with_its_path() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""title": "Synthetic patient summary","#,
        r#""title": "Synthetic patient summary", "synthetic": true,"#,
    )?;
    let error = refused(&text)?;
    let ReceiveError::Decode { ref source } = error else {
        return Err(format!("{error:?}").into());
    };
    assert_eq!(
        source.path, "Bundle.entry[0].resource.synthetic",
        "the refusal locates the property"
    );
    Ok(())
}

#[test]
fn a_bundle_that_is_not_a_document_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(r#""type": "document""#, r#""type": "collection""#)?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::NotADocument { found: Some(ref found) } if found == "collection"),
        "{error:?}"
    );
    Ok(())
}

// bdl-9: "A document must have an identifier with a system and a value".
#[test]
fn a_document_without_an_identifier_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000665" },"#,
        "",
    )?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::Unidentified), "{error:?}");
    Ok(())
}

#[test]
fn a_document_identifier_without_a_system_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000665" },"#,
        r#""identifier": { "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000665" },"#,
    )?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::Unidentified), "{error:?}");
    Ok(())
}

// bdl-10: "A document must have a date".
#[test]
fn a_document_without_a_timestamp_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(r#""timestamp": "2026-10-06T10:00:00Z","#, "")?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::Undated), "{error:?}");
    Ok(())
}

// bdl-11: "A document must have a Composition as the first resource".
#[test]
fn a_document_whose_first_entry_is_no_composition_is_refused() -> Result<(), Box<dyn Error>> {
    let first = DOCUMENT
        .find(
            r#"    {
      "fullUrl": "http://example.org/fhir/Composition/"#,
        )
        .ok_or("the Composition entry")?;
    let patient = DOCUMENT
        .find(
            r#"    {
      "fullUrl": "http://example.org/fhir/Patient/"#,
        )
        .ok_or("the Patient entry")?;
    let allergy = DOCUMENT
        .find(
            r#"    {
      "fullUrl": "http://example.org/fhir/AllergyIntolerance/"#,
        )
        .ok_or("the AllergyIntolerance entry")?;
    let composition = DOCUMENT.get(first..patient).ok_or("the Composition text")?;
    let patient_entry = DOCUMENT.get(patient..allergy).ok_or("the Patient text")?;
    let text = DOCUMENT.replacen(
        &format!("{composition}{patient_entry}"),
        &format!("{patient_entry}{composition}"),
        1,
    );
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::NoComposition), "{error:?}");
    Ok(())
}

#[test]
fn a_composition_without_a_subject_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" },"#,
        "",
    )?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::NoSubject), "{error:?}");
    Ok(())
}

#[test]
fn a_subject_no_entry_carries_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" }"#,
        r#""subject": { "reference": "Patient/synthetic-elsewhere" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectUnresolved),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_subject_that_names_an_entry_other_than_a_patient_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" }"#,
        r#""subject": { "reference": "AllergyIntolerance/synthetic-allergy" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::SubjectUnresolved),
        "{error:?}"
    );
    Ok(())
}
