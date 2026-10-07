// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The category a received document's `Composition.type` names, and the
//! profiles a document of it is checked against.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Category;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::category::Uncategorised;

use super::BUNDLE_PROFILE;
use super::COMPOSITION_PROFILE;
use super::DOCUMENT;
use super::document_with;

/// The `Composition.type` of the synthetic document.
const TYPE: &str = r#""type": { "coding": [{ "system": "http://loinc.org", "code": "60591-5" }] }"#;

#[test]
fn a_patient_summary_is_of_its_category_and_its_profiles() -> Result<(), Box<dyn Error>> {
    let category = ReceivedDocument::read(DOCUMENT)?.category()?;
    assert_eq!(category, Category::PatientSummary);
    let profiles = category.profiles().ok_or("the category is received")?;
    assert_eq!(profiles.bundle, BUNDLE_PROFILE);
    assert_eq!(profiles.composition, COMPOSITION_PROFILE);
    Ok(())
}

#[test]
fn a_category_is_named_by_its_hl7_europe_priority_code_exactly() {
    assert_eq!(Category::PatientSummary.code(), "Patient-Summaries");
    assert_eq!(
        Category::of_code("Patient-Summaries"),
        Some(Category::PatientSummary)
    );
    for other in [
        "patient-summaries",
        "Patient-Summaries ",
        "patient-summary",
        "",
    ] {
        assert_eq!(Category::of_code(other), None, "{other:?}");
    }
}

#[test]
fn another_coding_beside_the_category_still_names_it() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        TYPE,
        r#""type": { "coding": [{ "system": "http://example.org/types", "code": "summary" }, { "system": "http://loinc.org", "code": "60591-5" }] }"#,
    )?;
    assert_eq!(
        ReceivedDocument::read(&text)?.category()?,
        Category::PatientSummary
    );
    Ok(())
}

#[test]
fn a_type_no_category_fixes_is_uncategorised() -> Result<(), Box<dyn Error>> {
    for replaced in [
        r#""type": { "coding": [{ "system": "http://loinc.org", "code": "11502-2" }] }"#,
        r#""type": { "coding": [{ "system": "http://example.org/loinc", "code": "60591-5" }] }"#,
        r#""type": { "coding": [{ "code": "60591-5" }] }"#,
        r#""type": { "text": "Patient summary" }"#,
    ] {
        let text = document_with(TYPE, replaced)?;
        assert_eq!(
            ReceivedDocument::read(&text)?.category(),
            Err(Uncategorised::Unknown),
            "{replaced}"
        );
    }
    Ok(())
}
