// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! One reading of a received document: a text a lenient JSON reader and the
//! strict R4 model could read as two documents is refused before anything
//! reads it.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::receive::ReceiveError;
use eehrxf::receive::ReceivedDocument;

use super::document_with;

/// Reads `text`, expecting a refusal.
fn refused(text: &str) -> Result<ReceiveError, Box<dyn Error>> {
    ReceivedDocument::read(text)
        .err()
        .ok_or_else(|| "the document was read".into())
}

// RFC 8259 §4: a reader meeting a repeated name may keep either value.
#[test]
fn a_repeated_subject_is_refused_before_the_document_is_read() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" },"#,
        r#""subject": { "reference": "Patient/synthetic-patient" },
        "subject": { "reference": "Patient/synthetic-elsewhere" },"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::RepeatedName { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_name_repeated_through_an_escape_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""subject": { "reference": "Patient/synthetic-patient" },"#,
        r#""subject": { "reference": "Patient/synthetic-patient" },
        "subject": { "reference": "Patient/synthetic-elsewhere" },"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::RepeatedName { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_repeated_name_deep_in_an_entry_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""patient": { "reference": "Patient/synthetic-patient" }"#,
        r#""patient": { "reference": "Patient/synthetic-patient", "reference": "Patient/synthetic-elsewhere" }"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::RepeatedName { .. }),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_null_a_lenient_reader_would_skip_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""title": "Synthetic patient summary","#,
        r#""title": "Synthetic patient summary", "language": null,"#,
    )?;
    let error = refused(&text)?;
    let ReceiveError::Decode { ref source } = error else {
        return Err(format!("{error:?}").into());
    };
    assert_eq!(source.path, "Bundle.entry[0].resource.language");
    Ok(())
}

#[test]
fn an_empty_array_a_lenient_reader_would_skip_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""title": "Synthetic patient summary","#,
        r#""title": "Synthetic patient summary", "extension": [],"#,
    )?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::Decode { .. }), "{error:?}");
    Ok(())
}

#[test]
fn an_empty_primitive_sibling_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""status": "final","#,
        r#""status": "final", "_status": {},"#,
    )?;
    let error = refused(&text)?;
    assert!(matches!(error, ReceiveError::Decode { .. }), "{error:?}");
    Ok(())
}

// The R4 model keeps a resource of a type it does not define as an opaque
// body, so its contents would reach no rule, check or mapping.
#[test]
fn an_entry_of_a_type_r4_does_not_define_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""resourceType": "AllergyIntolerance","#,
        r#""resourceType": "SyntheticIntolerance","#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::UnknownResource { ref location } if location == "Bundle.entry[2].resource"),
        "{error:?}"
    );
    Ok(())
}

#[test]
fn a_contained_resource_of_a_type_r4_does_not_define_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""code": { "text": "Synthetic substance one" },"#,
        r#""code": { "text": "Synthetic substance one" },
        "contained": [{ "resourceType": "SyntheticPatient", "id": "shadow" }],"#,
    )?;
    let error = refused(&text)?;
    assert!(
        matches!(error, ReceiveError::UnknownResource { ref location } if location == "Bundle.entry[2].resource.contained[0]"),
        "{error:?}"
    );
    Ok(())
}
