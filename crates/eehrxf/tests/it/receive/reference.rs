// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one resolver of a reference inside a received document, form by
//! form: what each spelling resolves to, and every refusal
//! (<https://hl7.org/fhir/R4/bundle.html#references>,
//! <https://hl7.org/fhir/R4/references.html>).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::reference::ReferenceError;
use eehrxf::receive::reference::Target;
use fhir_types::codec::Json;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::reference::Reference;
use proptest::prelude::*;

use super::DOCUMENT;
use super::document_with;

/// The index of the Composition, the Patient and the allergy entries.
const COMPOSITION: usize = 0;
const PATIENT: usize = 1;
const ALLERGY: usize = 2;

/// The allergy's patient reference in the synthetic document.
const ALLERGY_PATIENT: &str = r#""patient": { "reference": "Patient/synthetic-patient" }"#;

/// Decodes a `Reference` from its JSON.
fn reference(json: &str) -> Result<Reference, Box<dyn Error>> {
    let Value::Object(object) = serde_json::from_str::<Value>(json)? else {
        return Err("a Reference is a JSON object".into());
    };
    Ok(Reference::from_json(&object, &mut Path::root("Reference"))?)
}

/// Decodes a `Reference` whose `reference` is `text`.
fn literal(text: &str) -> Result<Reference, Box<dyn Error>> {
    reference(&format!(
        r#"{{"reference":{}}}"#,
        serde_json::to_string(text)?
    ))
}

/// Resolves `json`, held by the entry at `holder`, in `document`.
fn resolved(
    document: &ReceivedDocument,
    holder: usize,
    json: &str,
) -> Result<Result<Target, ReferenceError>, Box<dyn Error>> {
    Ok(document.resolve(holder, &reference(json)?))
}

#[test]
fn every_canonical_form_resolves_to_its_one_entry() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let table = [
        (
            r#"{"reference":"Patient/synthetic-patient"}"#,
            Target::Entry(PATIENT),
        ),
        (
            r#"{"reference":"http://example.org/fhir/Patient/synthetic-patient"}"#,
            Target::Entry(PATIENT),
        ),
        (
            r#"{"reference":"AllergyIntolerance/synthetic-allergy"}"#,
            Target::Entry(ALLERGY),
        ),
        (r##"{"reference":"#"}"##, Target::Entry(COMPOSITION)),
        (
            r#"{"reference":"Patient/synthetic-patient","type":"Patient"}"#,
            Target::Entry(PATIENT),
        ),
        (
            r#"{"reference":"Patient/synthetic-patient","identifier":{"system":"urn:oid:2.999.1.665","value":"synthetic-0665"}}"#,
            Target::Entry(PATIENT),
        ),
    ];
    for (json, expected) in table {
        assert_eq!(
            resolved(&document, COMPOSITION, json)?,
            Ok(expected),
            "{json}"
        );
    }
    Ok(())
}

#[test]
fn every_other_spelling_is_refused_with_its_reason() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let not_in_bundle = |kind: Option<&str>| ReferenceError::NotInBundle {
        kind: kind.map(str::to_owned),
    };
    let table = [
        (" Patient/synthetic-patient", ReferenceError::NonCanonical),
        ("Patient/synthetic-patient ", ReferenceError::NonCanonical),
        (
            "Patient/synthetic-patient?_format=json",
            ReferenceError::NonCanonical,
        ),
        ("Patient/synthetic%2Dpatient", ReferenceError::NonCanonical),
        ("Patient/synthetic-patient/", ReferenceError::NonCanonical),
        ("patient/synthetic-patient", ReferenceError::NonCanonical),
        ("PATIENT/synthetic-patient", ReferenceError::NonCanonical),
        ("./Patient/synthetic-patient", ReferenceError::NonCanonical),
        (
            "HTTP://example.org/fhir/Patient/synthetic-patient",
            ReferenceError::NonCanonical,
        ),
        (
            "http://Example.org/fhir/Patient/synthetic-patient",
            ReferenceError::NonCanonical,
        ),
        (
            "http://example.org/fhir//Patient/synthetic-patient",
            ReferenceError::NonCanonical,
        ),
        (
            "http://example.org/fhir/./Patient/synthetic-patient",
            ReferenceError::NonCanonical,
        ),
        (
            "http://example.org/fhir/Patient/synthetic-patient#fragment",
            ReferenceError::NonCanonical,
        ),
        (
            "ftp://example.org/fhir/Patient/synthetic-patient",
            ReferenceError::NonCanonical,
        ),
        (
            "urn:uuid:0C3E1D20-0000-4000-8000-000000000999",
            ReferenceError::NonCanonical,
        ),
        ("urn:oid:2.0999", ReferenceError::NonCanonical),
        (
            "urn:uuid:0c3e1d20-0000-4000-8000-000000000999",
            not_in_bundle(None),
        ),
        ("urn:oid:2.999.1.665", not_in_bundle(None)),
        (
            "Patient/synthetic-elsewhere",
            not_in_bundle(Some("Patient")),
        ),
        (
            "https://elsewhere.example.org/fhir/Patient/synthetic-patient",
            not_in_bundle(Some("Patient")),
        ),
        (
            "Patient/synthetic-patient/_history/1",
            ReferenceError::VersionMismatch,
        ),
        ("#missing", ReferenceError::LocalMissing),
        ("#not an id", ReferenceError::NonCanonical),
    ];
    for (text, expected) in table {
        assert_eq!(
            document.resolve(COMPOSITION, &literal(text)?),
            Err(expected),
            "{text:?}"
        );
    }
    Ok(())
}

#[test]
fn a_reference_without_a_literal_is_refused() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    assert_eq!(
        resolved(
            &document,
            COMPOSITION,
            r#"{"identifier":{"system":"urn:oid:2.999.1.665","value":"synthetic-0665"}}"#
        )?,
        Err(ReferenceError::Unreferenced { identifier: true })
    );
    assert_eq!(
        resolved(&document, COMPOSITION, r#"{"display":"Synthetic"}"#)?,
        Err(ReferenceError::Unreferenced { identifier: false })
    );
    Ok(())
}

#[test]
fn a_type_or_identifier_that_disagrees_with_the_target_is_refused() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let table = [
        (
            r#"{"reference":"Patient/synthetic-patient","type":"Practitioner"}"#,
            ReferenceError::TypeDisagrees,
        ),
        (
            r#"{"reference":"Patient/synthetic-patient","type":"SyntheticType"}"#,
            ReferenceError::TypeDisagrees,
        ),
        (
            r#"{"reference":"Patient/synthetic-patient","identifier":{"system":"urn:oid:2.999.1.665","value":"synthetic-0666"}}"#,
            ReferenceError::IdentifierDisagrees,
        ),
        (
            r#"{"reference":"Patient/synthetic-patient","identifier":{"system":"urn:oid:2.999.1.666","value":"synthetic-0665"}}"#,
            ReferenceError::IdentifierDisagrees,
        ),
    ];
    for (json, expected) in table {
        assert_eq!(
            resolved(&document, COMPOSITION, json)?,
            Err(expected),
            "{json}"
        );
    }
    Ok(())
}

#[test]
fn a_urn_resolves_only_by_equality_and_a_relative_reference_needs_a_rest_base()
-> Result<(), Box<dyn Error>> {
    let mut text = DOCUMENT.to_owned();
    for (from, to) in [
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
        (
            r#""fullUrl": "http://example.org/fhir/Composition/synthetic-composition","#,
            r#""fullUrl": "urn:uuid:0c3e1d20-0000-4000-8000-000000000668","#,
        ),
        (
            r#""entry": [{ "reference": "AllergyIntolerance/synthetic-allergy" }]"#,
            r#""entry": [{ "reference": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy" }]"#,
        ),
    ] {
        text = text.replacen(from, to, 1);
    }
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(
        document.resolve(
            COMPOSITION,
            &literal("urn:uuid:0c3e1d20-0000-4000-8000-000000000667")?
        ),
        Ok(Target::Entry(PATIENT))
    );
    assert_eq!(
        document.resolve(
            COMPOSITION,
            &literal("AllergyIntolerance/synthetic-allergy")?
        ),
        Err(ReferenceError::RelativeWithoutBase)
    );
    assert_eq!(
        document.resolve(ALLERGY, &literal("AllergyIntolerance/synthetic-allergy")?),
        Ok(Target::Entry(ALLERGY))
    );
    Ok(())
}

#[test]
fn a_version_specific_reference_resolves_on_the_entry_version() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""id": "synthetic-patient","#,
        r#""id": "synthetic-patient", "meta": { "versionId": "1" },"#,
    )?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(
        document.resolve(
            COMPOSITION,
            &literal("Patient/synthetic-patient/_history/1")?
        ),
        Ok(Target::Entry(PATIENT))
    );
    assert_eq!(
        document.resolve(
            COMPOSITION,
            &literal("http://example.org/fhir/Patient/synthetic-patient/_history/1")?
        ),
        Ok(Target::Entry(PATIENT))
    );
    assert_eq!(
        document.resolve(
            COMPOSITION,
            &literal("Patient/synthetic-patient/_history/2")?
        ),
        Err(ReferenceError::VersionMismatch)
    );
    Ok(())
}

#[test]
fn a_local_reference_resolves_to_its_contained_resource() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""code": { "text": "Synthetic substance one" },"#,
        r##""code": { "text": "Synthetic substance one" },
        "contained": [{ "resourceType": "Practitioner", "id": "recorder" }],
        "recorder": { "reference": "#recorder" },"##,
    )?;
    let document = ReceivedDocument::read(&text)?;
    assert_eq!(
        document.resolve(ALLERGY, &literal("#recorder")?),
        Ok(Target::Contained {
            entry: ALLERGY,
            id: String::from("recorder")
        })
    );
    assert_eq!(
        document.resolve(COMPOSITION, &literal("#recorder")?),
        Err(ReferenceError::LocalMissing)
    );
    Ok(())
}

/// The spellings the generator combines: valid parts, near misses, and the
/// tricks a lenient reader would forgive.
fn spelling() -> impl Strategy<Value = String> {
    let prefix = prop::sample::select(vec![
        "",
        " ",
        "./",
        "/",
        "http://example.org/fhir/",
        "https://example.org/fhir/",
        "HTTP://example.org/fhir/",
        "http://EXAMPLE.org/fhir/",
        "http://example.org/fhir//",
        "http://example.org/FHIR/",
        "urn:uuid:",
        "#",
    ]);
    let kind = prop::sample::select(vec![
        "Patient",
        "patient",
        "PATIENT",
        "Patient%20",
        "AllergyIntolerance",
        "Composition",
        "",
    ]);
    let id = prop::sample::select(vec![
        "synthetic-patient",
        "Synthetic-Patient",
        "synthetic%2Dpatient",
        "synthetic-patient%00",
        "synthetic-elsewhere",
        "synthetic-allergy",
        "",
    ]);
    let suffix = prop::sample::select(vec![
        "",
        "/",
        " ",
        "?_format=json",
        "#x",
        "/_history/1",
        "/_history/",
        "%2F",
    ]);
    prop_oneof![
        (prefix, kind, id, suffix)
            .prop_map(|(prefix, kind, id, suffix)| format!("{prefix}{kind}/{id}{suffix}")),
        "\\PC{0,48}",
    ]
}

/// The two spellings that name the patient entry from the composition.
const PATIENT_SPELLINGS: &[&str] = &[
    "Patient/synthetic-patient",
    "http://example.org/fhir/Patient/synthetic-patient",
];

proptest! {
    #[test]
    fn no_spelling_but_the_canonical_ones_resolves_to_the_patient(text in spelling()) {
        let document = ReceivedDocument::read(DOCUMENT).map_err(|error| TestCaseError::fail(error.to_string()))?;
        // A spelling the R4 model does not decode never reaches a resolver:
        // the reading of the document refuses it, which the next property holds.
        let Ok(reference) = literal(&text) else {
            return Ok(());
        };
        for holder in [COMPOSITION, PATIENT, ALLERGY] {
            if document.resolve(holder, &reference) == Ok(Target::Entry(PATIENT)) {
                prop_assert!(PATIENT_SPELLINGS.contains(&text.as_str()), "{text:?} from {holder}");
            }
        }
    }

    #[test]
    fn a_document_is_read_only_when_its_patient_reference_names_the_subject(text in spelling()) {
        let quoted = serde_json::to_string(&text).map_err(|error| TestCaseError::fail(error.to_string()))?;
        let document = DOCUMENT.replacen(ALLERGY_PATIENT, &format!(r#""patient": {{ "reference": {quoted} }}"#), 1);
        if ReceivedDocument::read(&document).is_ok() {
            prop_assert!(PATIENT_SPELLINGS.contains(&text.as_str()), "{text:?}");
        }
    }
}
