// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect entry point, over a synthetic template, a synthetic
//! mapping of our own and a synthetic composition.
//!
//! The HL7 Europe mapping contexts are FerroBRIDGE's to author; until they
//! are published, the engine is wired and run against this minimal context.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::mapping::Mapping;
use eehrxf::mapping::MappingError;
use fhir_types::r4::resource::Resource;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run::Settings;

/// The fixtures beside this suite.
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The synthetic operational template.
fn opt() -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(fixture("opt/allergy.opt"))?)
}

/// The synthetic model and context mapping files.
fn files() -> [PathBuf; 2] {
    [
        fixture("mapping/ferrofed_allergy.yml"),
        fixture("mapping/ferrofed_allergy.context.yml"),
    ]
}

/// The context mapping the files declare.
const CONTEXT: &str = "ferrofed_allergy.context";

/// A canonical composition of the synthetic template, as a CDR would answer
/// it, with a synthetic `OBJECT_VERSION_ID`.
pub(crate) const COMPOSITION: &str = r#"{
  "_type": "COMPOSITION",
  "name": { "_type": "DV_TEXT", "value": "Synthetic allergy summary" },
  "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1",
  "archetype_details": {
    "_type": "ARCHETYPED",
    "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1" },
    "template_id": { "_type": "TEMPLATE_ID", "value": "ferrofed.eehrxf.allergy.v1" },
    "rm_version": "1.1.0"
  },
  "uid": { "_type": "OBJECT_VERSION_ID", "value": "7c4e1d20-0000-4000-8000-000000000001::ferrofed.example::1" },
  "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
  "territory": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }, "code_string": "NL" },
  "category": {
    "_type": "DV_CODED_TEXT",
    "value": "persistent",
    "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "431" }
  },
  "composer": { "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" },
  "content": [
    {
      "_type": "EVALUATION",
      "name": { "_type": "DV_TEXT", "value": "Synthetic allergy" },
      "archetype_node_id": "openEHR-EHR-EVALUATION.ferrofed_allergy.v1",
      "archetype_details": {
        "_type": "ARCHETYPED",
        "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-EVALUATION.ferrofed_allergy.v1" },
        "rm_version": "1.1.0"
      },
      "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
      "encoding": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }, "code_string": "UTF-8" },
      "subject": { "_type": "PARTY_SELF" },
      "data": {
        "_type": "ITEM_TREE",
        "name": { "_type": "DV_TEXT", "value": "Tree" },
        "archetype_node_id": "at0001",
        "items": [
          {
            "_type": "ELEMENT",
            "name": { "_type": "DV_TEXT", "value": "Substance" },
            "archetype_node_id": "at0002",
            "value": { "_type": "DV_TEXT", "value": "Synthetic substance one" }
          }
        ]
      }
    }
  ]
}"#;

/// The run settings: a synthetic engine device and a fixed instant, so
/// nothing reads a clock.
fn settings() -> Settings {
    Settings::new("Device/ferrofed-eehrxf-test", "2026-10-06T10:00:00Z")
}

/// The compiled synthetic mapping.
fn mapping() -> Result<Mapping, Box<dyn Error>> {
    Ok(Mapping::compile(&opt()?, &files(), &[CONTEXT])?)
}

#[test]
fn a_composition_maps_to_its_resource_with_one_provenance() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    assert_eq!(mapping.len(), 1, "one context compiled");
    let contexts: Vec<_> = mapping
        .contexts()
        .map(|context| (context.name, context.template, context.profile))
        .collect();
    assert_eq!(
        contexts,
        [(
            CONTEXT,
            "ferrofed.eehrxf.allergy.v1",
            "http://example.org/fhir/StructureDefinition/ferrofed-allergy"
        )]
    );
    let answer = mapping.to_fhir(COMPOSITION, &settings())?;
    let bundle = answer.bundle();
    assert_eq!(bundle.r#type.value.as_deref(), Some("collection"));
    let allergy = bundle
        .entry
        .iter()
        .find_map(|entry| match entry.resource {
            Some(Resource::AllergyIntolerance(ref allergy)) => Some(allergy),
            _ => None,
        })
        .ok_or("the Bundle carries the AllergyIntolerance")?;
    assert_eq!(
        allergy
            .code
            .as_ref()
            .and_then(|code| code.text.as_ref())
            .and_then(|text| text.value.as_deref()),
        Some("Synthetic substance one"),
        "the substance became the code"
    );
    assert_eq!(
        allergy.id.as_deref(),
        Some("7c4e1d20-0000-4000-8000-000000000001"),
        "the id is the composition's versioned object uid"
    );
    let provenances: Vec<_> = bundle
        .entry
        .iter()
        .filter_map(|entry| match entry.resource {
            Some(Resource::Provenance(ref provenance)) => Some(provenance),
            _ => None,
        })
        .collect();
    let [provenance] = provenances.as_slice() else {
        return Err(format!("one Provenance, found {}", provenances.len()).into());
    };
    let targets: Vec<Option<&str>> = provenance
        .target
        .iter()
        .map(|target| {
            target
                .reference
                .as_ref()
                .and_then(|reference| reference.value.as_deref())
        })
        .collect();
    assert_eq!(
        targets,
        [Some(
            "AllergyIntolerance/7c4e1d20-0000-4000-8000-000000000001"
        )]
    );
    Ok(())
}

#[test]
fn the_same_composition_maps_to_the_same_bundle() -> Result<(), Box<dyn Error>> {
    let mapping = mapping()?;
    let first = mapping.to_fhir(COMPOSITION, &settings())?;
    let second = mapping.to_fhir(COMPOSITION, &settings())?;
    assert_eq!(first.bundle(), second.bundle());
    Ok(())
}

#[test]
fn a_flat_composition_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = mapping()?.to_fhir(
        r#"{"ferrofed.eehrxf.allergy.v1/synthetic_allergy/substance": "Synthetic substance one"}"#,
        &settings(),
    );
    assert!(
        matches!(refused, Err(MappingError::NotCanonical)),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_composition_of_another_template_is_refused() -> Result<(), Box<dyn Error>> {
    let other = COMPOSITION.replace("ferrofed.eehrxf.allergy.v1", "ferrofed.eehrxf.other.v1");
    let refused = mapping()?.to_fhir(&other, &settings());
    let Err(MappingError::Run { source }) = refused else {
        return Err(format!("{refused:?}").into());
    };
    assert!(
        matches!(*source, OperationError::UnknownTemplate { .. }),
        "{source:?}"
    );
    Ok(())
}

#[test]
fn text_that_is_no_json_object_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = mapping()?.to_fhir("[]", &settings());
    assert!(
        matches!(refused, Err(MappingError::Run { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_template_that_is_no_opt_is_refused() {
    let refused = Mapping::compile("<template/>", &files(), &[CONTEXT]);
    assert!(
        matches!(refused, Err(MappingError::Template { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_context_the_files_do_not_declare_is_refused_with_diagnostics() -> Result<(), Box<dyn Error>> {
    let refused = Mapping::compile(&opt()?, &files(), &["ferrofed_absent.context"]);
    let Err(MappingError::Refused { diagnostics }) = refused else {
        return Err(format!("{refused:?}").into());
    };
    assert!(!diagnostics.is_empty(), "the refusal names its diagnostics");
    Ok(())
}

#[test]
fn an_empty_context_name_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = Mapping::compile(&opt()?, &files(), &[""]);
    assert!(
        matches!(refused, Err(MappingError::ContextName { .. })),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_mapping_file_that_does_not_load_is_refused() -> Result<(), Box<dyn Error>> {
    let refused = Mapping::compile(&opt()?, &[fixture("mapping/absent.yml")], &[CONTEXT]);
    assert!(
        matches!(refused, Err(MappingError::Refused { .. })),
        "{refused:?}"
    );
    Ok(())
}
