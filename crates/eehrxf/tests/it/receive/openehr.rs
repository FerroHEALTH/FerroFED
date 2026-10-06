// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document mapped into openEHR by the synthetic FHIRconnect
//! context, with the original document kept in the composition's
//! `FEEDER_AUDIT.original_content`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::mapping::Mapping;
use eehrxf::receive::ReceivedDocument;
use eehrxf::receive::openehr::FHIR_JSON;
use eehrxf::receive::openehr::ReceiveMappingError;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run::Settings;
use openehr_rm::v1_2::composition::composition::Composition;
use openehr_rm::v1_2::data_types::encapsulated::dv_encapsulated::DvEncapsulated;

use super::DOCUMENT;
use super::document_with;
use super::eps_example;

/// The fixtures beside this suite.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The synthetic mapping of the allergy template.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    let opt = std::fs::read_to_string(fixture("opt/allergy.opt"))?;
    Ok(Mapping::compile(
        &opt,
        &[
            fixture("mapping/ferrofed_allergy.yml"),
            fixture("mapping/ferrofed_allergy.context.yml"),
        ],
        &["ferrofed_allergy.context"],
    )?)
}

/// The run settings: a synthetic engine device, a fixed instant, and the
/// composition language and territory the caller defaults.
fn settings() -> Settings {
    let now = "2026-10-06T10:00:00Z";
    Settings::new("Device/ferrofed-eehrxf-test", now)
        .with_defaults(Defaults::at(now).with_language("en").with_territory("NL"))
}

#[test]
fn a_document_maps_to_one_composition_that_keeps_the_original() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let received = mapping()?.to_openehr(&document, &settings())?;
    assert_eq!(received.template_id(), "ferrofed.eehrxf.allergy.v1");
    let composition: Composition = serde_json::from_str(received.composition())?;
    let audit = composition
        .feeder_audit
        .as_ref()
        .ok_or("the composition carries a feeder audit")?;
    assert_eq!(
        audit.originating_system_audit.system_id, "Device/ferrofed-eehrxf-test",
        "the engine's own audit is kept beside the original"
    );
    let original = audit
        .original_content
        .as_ref()
        .ok_or("the composition keeps the original content")?;
    let DvEncapsulated::DvParsable(parsable) = original else {
        return Err(format!("the original is no DV_PARSABLE: {original:?}").into());
    };
    assert_eq!(parsable.formalism, FHIR_JSON);
    assert_eq!(
        parsable.value, DOCUMENT,
        "the original is kept byte for byte"
    );
    assert!(
        received.composition().contains("Synthetic substance one"),
        "the allergy's substance is mapped into the composition"
    );
    Ok(())
}

#[test]
fn the_same_document_maps_to_the_same_composition() -> Result<(), Box<dyn Error>> {
    let document = ReceivedDocument::read(DOCUMENT)?;
    let mapping = mapping()?;
    let first = mapping.to_openehr(&document, &settings())?;
    let second = mapping.to_openehr(&document, &settings())?;
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn a_document_no_context_maps_is_refused() -> Result<(), Box<dyn Error>> {
    let text = document_with(
        r#""meta": { "profile": ["http://example.org/fhir/StructureDefinition/ferrofed-allergy"] },"#,
        "",
    )?;
    let document = ReceivedDocument::read(&text)?;
    let refused = mapping()?.to_openehr(&document, &settings());
    let Err(ReceiveMappingError::Run { source }) = refused else {
        return Err(format!("{refused:?}").into());
    };
    assert!(
        matches!(*source, OperationError::Select { .. }),
        "{source:?}"
    );
    Ok(())
}

#[test]
fn a_document_with_two_resources_of_the_mapped_type_is_refused() -> Result<(), Box<dyn Error>> {
    let second = r#",
    {
      "fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy-two",
      "resource": {
        "resourceType": "AllergyIntolerance",
        "id": "synthetic-allergy-two",
        "meta": { "profile": ["http://example.org/fhir/StructureDefinition/ferrofed-allergy"] },
        "code": { "text": "Synthetic substance two" },
        "patient": { "reference": "Patient/synthetic-patient" }
      }
    }
  ]
}"#;
    let end = DOCUMENT.rfind("\n  ]\n}").ok_or("the end of the entries")?;
    let text = format!("{}{second}", DOCUMENT.get(..end).ok_or("the entries")?);
    let document = ReceivedDocument::read(&text)?;
    let refused = mapping()?.to_openehr(&document, &settings());
    let Err(ReceiveMappingError::Run { source }) = refused else {
        return Err(format!("{refused:?}").into());
    };
    assert!(
        matches!(
            *source,
            OperationError::SeveralSubjectResources { count: 2, .. }
        ),
        "{source:?}"
    );
    Ok(())
}

// R4 resolves a reference inside a Bundle against the entries' fullUrls
// (<https://hl7.org/fhir/R4/bundle.html#references>), so a Patient entry whose
// fullUrl is the urn:uuid every reference names is one subject, not two.
#[test]
#[ignore = "fhirconnect refuses urn:uuid subjects; FerroBRIDGE issue to follow"]
fn a_published_document_with_urn_full_urls_is_not_refused_as_several_subjects()
-> Result<(), Box<dyn Error>> {
    let text = eps_example("Bundle-EPSExampleBundle01NoProblemsMedicationAllergies.json")?;
    let document = ReceivedDocument::read(&text)?;
    let mapped = mapping()?.to_openehr(&document, &settings());
    if let Err(ReceiveMappingError::Run { ref source }) = mapped {
        assert!(
            !matches!(**source, OperationError::SeveralSubjects { .. }),
            "{source:?}"
        );
    }
    Ok(())
}
