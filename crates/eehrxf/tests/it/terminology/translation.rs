// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The concepts of a mapped resource in no preferred system, translated
//! through a scripted terminology server: an `equivalent` or `equal` match
//! added after the original coding, a concept in a preferred system left
//! alone, no server leaving the code as recorded (eHN PS Art 10(7)), and a
//! refusal failing the translation.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Operation;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::translation::TranslationError;
use eehrxf::terminology::translation::Translator;
use fhir_types::r4::allergy_intolerance::AllergyIntolerance;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use http::StatusCode;

use super::LOCAL;
use super::PREFERRED;
use super::Script;
use super::Scripted;
use super::coding;

type TestResult = Result<(), Box<dyn Error>>;

/// Whether `system` is preferred.
fn preferred(system: &str) -> bool {
    system == PREFERRED
}

/// The systems translated into.
const TARGETS: [&str; 1] = [PREFERRED];

/// An allergy whose code carries `codings`.
fn allergy(codings: Vec<Coding>) -> Resource {
    Resource::AllergyIntolerance(Box::new(AllergyIntolerance {
        code: Some(CodeableConcept {
            coding: codings,
            ..CodeableConcept::default()
        }),
        patient: Reference {
            reference: Some("urn:uuid:5a6b7c8d-0000-4000-8000-000000000002".into()),
            ..Reference::default()
        },
        ..AllergyIntolerance::default()
    }))
}

/// The codings of the allergy's code, each as system and code.
fn codes(resource: &Resource) -> Vec<(String, String)> {
    let Resource::AllergyIntolerance(allergy) = resource else {
        return Vec::new();
    };
    allergy
        .code
        .iter()
        .flat_map(|concept| concept.coding.iter())
        .filter_map(|coding| {
            Some((
                coding.system.as_ref()?.value.clone()?,
                coding.code.as_ref()?.value.clone()?,
            ))
        })
        .collect()
}

fn pair(system: &str, code: &str) -> (String, String) {
    (system.to_owned(), code.to_owned())
}

/// The terminology answered by `script`.
fn scripted(script: Script) -> Terminology {
    Terminology::server(Arc::new(Scripted(script)))
}

#[tokio::test]
async fn an_equivalent_match_is_added_after_the_original_coding() -> TestResult {
    let terminology = scripted(Script::Translate(vec![
        ("equivalent", PREFERRED, "P-1"),
        ("wider", PREFERRED, "P-2"),
        ("equal", PREFERRED, "P-3"),
    ]));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let translated = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await?;
    assert_eq!(
        codes(&translated),
        [
            pair(LOCAL, "L-1"),
            pair(PREFERRED, "P-1"),
            pair(PREFERRED, "P-3")
        ],
        "FHIR R4 ConceptMapEquivalence: only equivalent and equal are a translation"
    );
    Ok(())
}

#[tokio::test]
async fn a_concept_in_a_preferred_system_is_not_asked() -> TestResult {
    let terminology = scripted(Script::Refuse(StatusCode::INTERNAL_SERVER_ERROR));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1"), coding(PREFERRED, "P-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded);
    Ok(())
}

#[tokio::test]
async fn with_no_server_the_code_is_transferred_as_recorded() -> TestResult {
    let terminology = Terminology::none();
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded, "eHN PS Art 10(7)");
    Ok(())
}

#[tokio::test]
async fn no_accepted_match_leaves_the_code_as_recorded() -> TestResult {
    let terminology = scripted(Script::Translate(vec![("narrower", PREFERRED, "P-2")]));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let recorded = allergy(vec![coding(LOCAL, "L-1")]);
    let translated = translator.translate(&recorded, preferred).await?;
    assert_eq!(translated, recorded);
    Ok(())
}

#[tokio::test]
async fn a_refused_translation_fails_and_is_never_no_translation() {
    let terminology = scripted(Script::Refuse(StatusCode::SERVICE_UNAVAILABLE));
    let mut translator = Translator::new(&terminology, &TARGETS);
    let refused = translator
        .translate(&allergy(vec![coding(LOCAL, "L-1")]), preferred)
        .await;
    assert!(
        matches!(
            refused,
            Err(TranslationError::Terminology(TerminologyError::Status {
                operation: Operation::Translate,
                status: StatusCode::SERVICE_UNAVAILABLE,
            }))
        ),
        "{refused:?}"
    );
}
