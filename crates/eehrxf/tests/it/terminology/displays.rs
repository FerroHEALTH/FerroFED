// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The displays of a received document: a coding with no display is
//! refused with no terminology server, given its display by `$lookup` with
//! one, and a failed lookup is an error carrying the server's status; the
//! original document is kept as it was received (RM Release 1.1.0
//! `DV_CODED_TEXT.value`, FHIR R4 `CodeSystem/$lookup`).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::displays::DisplayError;
use eehrxf::terminology::Terminology;
use eehrxf::terminology::TerminologyError;
use fhir_types::r4::resource::Resource;
use http::StatusCode;

use super::Script;
use super::Scripted;
use crate::receive::DOCUMENT;
use crate::receive::PATIENT_IDENTIFIER;
use crate::receive::document_with;

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn every_coding_without_a_display_is_found() -> TestResult {
    let missing = ReceivedDocument::read(DOCUMENT)?.missing_displays()?;
    let locations: Vec<&str> = missing
        .iter()
        .map(|missing| missing.location.as_str())
        .collect();
    assert_eq!(
        locations.first().copied(),
        Some("Bundle.entry[0].resource.section[0].code.coding[0]")
    );
    assert_eq!(locations.len(), 6, "the type and the five section codes");
    Ok(())
}

#[test]
fn a_coding_with_a_display_needs_none() -> TestResult {
    let text = document_with(
        r#""code": "60591-5" }"#,
        r#""code": "60591-5", "display": "Synthetic display" }"#,
    )?;
    let missing = ReceivedDocument::read(&text)?.missing_displays()?;
    assert_eq!(missing.len(), 5);
    Ok(())
}

#[tokio::test]
async fn a_coding_without_a_display_is_refused_with_no_server() -> TestResult {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let refused = document.displayed(&Terminology::none()).await;
    let Err(error) = refused else {
        return Err("the document was displayed with no server".into());
    };
    assert!(
        matches!(
            &error,
            DisplayError::Missing {
                location,
                source: TerminologyError::Unconfigured { .. },
            } if location == "Bundle.entry[0].resource.section[0].code.coding[0]"
        ),
        "{error:?}"
    );
    assert!(!error.to_string().contains(PATIENT_IDENTIFIER));
    Ok(())
}

#[tokio::test]
async fn a_server_gives_every_missing_display() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Display("Synthetic term"))));
    let document = ReceivedDocument::read(DOCUMENT)?;
    let displayed = document.displayed(&terminology).await?;
    assert_eq!(
        displayed.missing_displays()?,
        Vec::<eehrxf::receive::displays::MissingDisplay>::new()
    );
    assert_eq!(displayed.original(), DOCUMENT, "the original is kept");
    let Some(Resource::Composition(composition)) = displayed
        .bundle()
        .entry
        .first()
        .and_then(|entry| entry.resource.as_ref())
    else {
        return Err("the first entry is no Composition".into());
    };
    let display = composition
        .r#type
        .coding
        .first()
        .and_then(|coding| coding.display.as_ref())
        .and_then(|display| display.value.as_deref());
    assert_eq!(display, Some("Synthetic term"));
    assert_eq!(
        displayed.composition().r#type,
        composition.r#type,
        "the Composition the document names is the displayed one"
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_lookup_carries_the_status() -> TestResult {
    let terminology = Terminology::server(Arc::new(Scripted(Script::Refuse(
        StatusCode::SERVICE_UNAVAILABLE,
    ))));
    let refused = ReceivedDocument::read(DOCUMENT)?
        .displayed(&terminology)
        .await;
    assert!(
        matches!(
            refused,
            Err(DisplayError::Lookup {
                source: TerminologyError::Status {
                    status: StatusCode::SERVICE_UNAVAILABLE,
                    ..
                },
                ..
            })
        ),
        "{refused:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_coding_with_no_code_cannot_be_looked_up() -> TestResult {
    let text = document_with(
        r#"{ "system": "http://loinc.org", "code": "60591-5" }"#,
        r#"{ "system": "http://loinc.org" }"#,
    )?;
    let terminology = Terminology::server(Arc::new(Scripted(Script::Display("Synthetic term"))));
    let refused = ReceivedDocument::read(&text)?.displayed(&terminology).await;
    assert!(
        matches!(refused, Err(DisplayError::Uncoded { .. })),
        "{refused:?}"
    );
    Ok(())
}
