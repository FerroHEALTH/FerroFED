// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What one semantic transformation did, counted by path and never by value
//! (eHealth Network General Guidelines Release 3.1 Art 6(3); eHealth
//! Network Guidelines on Patient Summary Art 10(7)).

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use eehrxf::category::Category;
use eehrxf::mapping::Mapping;
use eehrxf::terminology::ehn;
use eehrxf::transformation::{Direction, Pin, Report};
use fhir_types::codec::{Json as _, Path, Value};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::outcome::{Location, Loss, SkipReason, Warning};
use fhirconnect::operations::issues;

/// A code value no path may carry.
const CODE: &str = "Qz7-code-91";

/// A text value no path may carry.
const TEXT: &str = "Qz7-text-92";

/// The archetype path of the openEHR node a loss is located at, which is no
/// R4 element path and so is never counted as one.
const OPENEHR_PATH: &str = "/content[openEHR-EHR-EVALUATION.adverse_reaction_risk.v1]/data[at0001]";

/// A reference a declared loss names, which no path may carry.
const REFERENCE: &str = "Patient?identifier=urn:oid:2.999.1|Qz7-patient-93";

/// The resource the FHIR JSON `json` holds.
fn resource(json: &str) -> Result<Resource, Box<dyn Error>> {
    let value: Value = serde_json::from_str(json)?;
    let object = value.as_object().ok_or("an object")?;
    Ok(Resource::from_json(object, &mut Path::root("Resource"))?)
}

/// A report of a received patient summary through an empty pin.
fn report() -> Report {
    Report::new(
        Direction::Received,
        Category::PatientSummary,
        Pin {
            template: "example.template.v1".to_owned(),
            ..Pin::default()
        },
    )
}

/// Whether `system` is one of the preferred code systems of §4.
fn preferred(system: &str) -> bool {
    ehn::PATIENT_SUMMARY.contains(&system)
}

/// An allergy with a preferred code, a text-only reaction substance, a
/// non-preferred clinical status, a `code` primitive and a profile tag.
fn allergy() -> Result<Resource, Box<dyn Error>> {
    resource(&format!(
        r#"{{
            "resourceType": "AllergyIntolerance",
            "meta": {{ "tag": [{{ "system": "https://example.org/tags", "code": "{CODE}" }}] }},
            "clinicalStatus": {{ "coding": [{{ "system": "https://example.org/status", "code": "{CODE}" }}] }},
            "code": {{ "coding": [
                {{ "system": "http://snomed.info/sct", "version": "http://snomed.info/sct/11000146104", "code": "{CODE}" }},
                {{ "system": "https://example.org/local", "code": "{CODE}" }}
            ], "text": "{TEXT}" }},
            "criticality": "high",
            "patient": {{ "reference": "Patient/example" }},
            "reaction": [{{ "substance": {{ "text": "{TEXT}" }}, "manifestation": [{{ "text": "{TEXT}" }}] }}]
        }}"#
    ))
}

#[test]
fn a_coded_element_is_transcoded_only_in_a_preferred_system() -> Result<(), Box<dyn Error>> {
    let mut report = report();
    report.count_coded(&allergy()?, preferred)?;
    assert_eq!(report.transcoded().count(), 1);
    assert_eq!(
        report.transcoded().paths(),
        &BTreeMap::from([("AllergyIntolerance.code".to_owned(), 1)]),
        "one concept, counted once whatever codings it holds"
    );
    assert_eq!(report.untranscoded().count(), 3, "Art 10(7)");
    assert_eq!(
        report.untranscoded().paths(),
        &BTreeMap::from([
            ("AllergyIntolerance.clinicalStatus".to_owned(), 1),
            ("AllergyIntolerance.reaction.manifestation".to_owned(), 1),
            ("AllergyIntolerance.reaction.substance".to_owned(), 1),
        ])
    );
    assert_eq!(report.dropped().count(), 0, "nothing dropped is zero");
    assert!(
        report
            .terminology()
            .contains("http://snomed.info/sct|http://snomed.info/sct/11000146104"),
        "the SNOMED CT edition in force: {:?}",
        report.terminology()
    );
    assert!(
        report
            .terminology()
            .contains("ehn-patient-summary-guidelines|3.4"),
        "the release the preferred systems are read from"
    );
    Ok(())
}

#[test]
fn a_report_carries_no_code_value_or_text() -> Result<(), Box<dyn Error>> {
    let mut report = report();
    report.count_coded(&allergy()?, preferred)?;
    report.count_losses(&losses().ok_or("an outcome")?);
    let shown = format!("{report:?}");
    for withheld in [CODE, TEXT, REFERENCE, "Qz7-patient-93"] {
        assert!(!shown.contains(withheld), "{withheld} in {shown}");
    }
    Ok(())
}

/// The outcome of a run that declared each kind of loss, each located as the
/// engine locates it, a skipped mapping with no FHIR element, and a skipped
/// reference with no literal reference.
fn losses() -> Option<fhir_types::r4::operation_outcome::OperationOutcome> {
    issues::of_warnings(&[
        Warning::new(
            Loss::LastOfMany {
                path: "AllergyIntolerance.note".to_owned(),
                dropped: 2,
            },
            Location::default().with_fhir("AllergyIntolerance.note"),
        ),
        Warning::new(
            Loss::Skipped {
                mapping: "allergy.reaction".to_owned(),
                reason: SkipReason::UnresolvedReference {
                    reference: REFERENCE.to_owned(),
                },
            },
            Location::default()
                .with_fhir("AllergyIntolerance.reaction[0]")
                .with_openehr(OPENEHR_PATH),
        ),
        Warning::new(
            Loss::OneWayFallback {
                cell: "DV_QUANTITY".to_owned(),
                field: "precision".to_owned(),
            },
            Location::default()
                .with_fhir("AllergyIntolerance.reaction[1].manifestation[0].coding[2].code"),
        ),
        Warning::new(
            Loss::Skipped {
                mapping: "allergy.recorder".to_owned(),
                reason: SkipReason::EmptyReference,
            },
            Location::default().with_fhir("AllergyIntolerance.recorder"),
        ),
        Warning::new(
            Loss::Skipped {
                mapping: "allergy.category".to_owned(),
                reason: SkipReason::Unidirectional,
            },
            Location::default().with_openehr(OPENEHR_PATH),
        ),
        Warning::new(
            Loss::Defaulted {
                field: "/language".to_owned(),
            },
            Location::default().with_openehr("/language"),
        ),
    ])
}

#[test]
fn each_declared_loss_is_dropped_at_the_path_it_names_without_a_value() -> Result<(), Box<dyn Error>>
{
    let mut report = report();
    report.count_losses(&losses().ok_or("an outcome")?);
    assert_eq!(
        report.dropped().count(),
        5,
        "every warning is a loss; a defaulted field is information"
    );
    assert_eq!(
        report.dropped().paths(),
        &BTreeMap::from([
            ("AllergyIntolerance.note".to_owned(), 1),
            ("AllergyIntolerance.reaction".to_owned(), 1),
            (
                "AllergyIntolerance.reaction.manifestation.coding.code".to_owned(),
                1
            ),
            ("AllergyIntolerance.recorder".to_owned(), 1),
        ]),
        "each loss at the element its expression locates, without the index; \
         a loss with no FHIR element is counted with no path"
    );
    Ok(())
}

#[test]
fn an_uncovered_composition_is_dropped_whole_and_reports_absorb() {
    let mut first = report();
    first.count_uncovered();
    let mut second = report();
    second.count_uncovered();
    first.absorb(&second);
    assert_eq!(first.dropped().count(), 2);
    assert_eq!(
        first.dropped().paths(),
        &BTreeMap::from([("/".to_owned(), 2)])
    );
}

/// The fixtures beside this suite.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn a_mapping_pins_its_context_template_profile_and_files() -> Result<(), Box<dyn Error>> {
    let opt = std::fs::read_to_string(fixture("opt/allergy.opt"))?;
    let files = [
        fixture("mapping/ferrofed_allergy.yml"),
        fixture("mapping/ferrofed_allergy.context.yml"),
    ];
    let mapping = Mapping::compile(&opt, &files, &["ferrofed_allergy.context"])?;
    assert_eq!(
        mapping.files(),
        ["ferrofed_allergy.yml", "ferrofed_allergy.context.yml"],
        "file names, never the deployment's directories"
    );
    let pin = mapping
        .pin("ferrofed.eehrxf.allergy.v1")
        .ok_or("the template's pin")?;
    assert_eq!(pin.context.as_deref(), Some("ferrofed_allergy.context"));
    assert_eq!(pin.template, "ferrofed.eehrxf.allergy.v1");
    assert!(pin.profile.is_some(), "the profile it maps to");
    assert!(pin.package.is_none(), "the mapping holds no package");
    assert!(mapping.pin("another.template.v1").is_none());
    Ok(())
}
