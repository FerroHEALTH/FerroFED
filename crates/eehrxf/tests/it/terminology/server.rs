// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The terminology server seam: every operation fails closed with no server,
//! a refusal carries the server's status, and `$translate` is taken only for
//! an `equivalent` or `equal` match (FHIR R4 `ConceptMapEquivalence`).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::terminology::Candidate;
use eehrxf::terminology::Operation;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use eehrxf::terminology::accepted;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponse;
use fhir_types::r4::operations::concept_map_translate::ConceptMapTranslateResponseMatch;
use fhir_types::r4::primitives::Boolean;
use fhir_types::r4::primitives::Code;
use http::StatusCode;

use super::LOCAL;
use super::PREFERRED;
use super::Script;
use super::Scripted;
use super::coding;

type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn every_operation_fails_closed_without_a_server() {
    let none = Terminology::none();
    assert!(!none.is_configured());
    assert!(matches!(
        none.display(LOCAL, "L-1").await,
        Err(TerminologyError::Unconfigured {
            operation: Operation::Lookup
        })
    ));
    let candidate = Candidate::Coding(Box::new(coding(LOCAL, "L-1")));
    assert!(matches!(
        none.validate("http://example.org/fhir/ValueSet/synthetic", &candidate)
            .await,
        Err(TerminologyError::Unconfigured {
            operation: Operation::ValidateCode
        })
    ));
}

#[tokio::test]
async fn a_lookup_answers_the_display() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Display("Synthetic term"))));
    assert_eq!(terminology.display(LOCAL, "L-1").await?, "Synthetic term");
    Ok(())
}

#[tokio::test]
async fn a_refused_lookup_carries_the_status() {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::GATEWAY_TIMEOUT,
    ))));
    assert!(matches!(
        terminology.display(LOCAL, "L-1").await,
        Err(TerminologyError::Status {
            operation: Operation::Lookup,
            status: StatusCode::GATEWAY_TIMEOUT
        })
    ));
}

#[tokio::test]
async fn a_translation_takes_only_equivalent_and_equal_matches() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Translate(vec![
        ("equal", PREFERRED, "1001"),
        ("relatedto", PREFERRED, "1002"),
        ("subsumes", PREFERRED, "1003"),
        ("specializes", PREFERRED, "1004"),
        ("unmatched", PREFERRED, "1005"),
        ("disjoint", PREFERRED, "1006"),
        ("equivalent", PREFERRED, "1007"),
    ]))));
    let found = terminology.translate(LOCAL, "L-1", PREFERRED).await?;
    assert_eq!(
        found,
        [coding(PREFERRED, "1001"), coding(PREFERRED, "1007")]
    );
    Ok(())
}

#[test]
fn a_translation_whose_result_is_false_gives_nothing() {
    let response = ConceptMapTranslateResponse {
        result: Boolean {
            value: Some(false),
            ..Boolean::default()
        },
        r#match: vec![ConceptMapTranslateResponseMatch {
            equivalence: Some(Code::from("equivalent")),
            concept: Some(coding(PREFERRED, "1001")),
            ..ConceptMapTranslateResponseMatch::default()
        }],
        ..ConceptMapTranslateResponse::default()
    };
    assert_eq!(
        accepted(&response),
        Vec::<fhir_types::r4::coding::Coding>::new()
    );
}

#[test]
fn every_operation_names_its_path() {
    assert_eq!(Operation::Translate.path(), "ConceptMap/$translate");
    assert_eq!(Operation::Lookup.path(), "CodeSystem/$lookup");
    assert_eq!(Operation::ValidateCode.path(), "ValueSet/$validate-code");
}
