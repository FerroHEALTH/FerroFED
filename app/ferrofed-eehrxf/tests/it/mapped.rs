// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The map mode of the prescription face: the MPD project's request
//! context compiled against the vendored medication template, and a member's
//! medication order composition mapped into one page of prescriptions.

use std::error::Error;

use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use ferrofed_eehrxf::prescriptions::mapped::{self, MapMode, MapModeError, Template};
use ferrofed_testkit::medication;
use fhir_types::codec::Value;

type TestResult = Result<(), Box<dyn Error>>;

/// The version id of the synthetic composition, created at the member
/// `node-a.example.org`.
const UID: &str = "5f0c2a9e-0000-4000-8000-000000000001::node-a.example.org::1";

/// The patient the search names.
const PATIENT: (&str, &str) = ("urn:oid:2.999.1", "synthetic-patient-1");

/// The map mode of the vendored ADL 2 template, and a composition of it.
fn compiled() -> Result<(MapMode, String), Box<dyn Error>> {
    let medication::Written {
        template,
        archetypes,
    } = medication::template()?;
    let written = Template::Adl2 {
        template: &template,
        archetypes: &archetypes,
    };
    let composition =
        medication::order_composition(&written.source()?, UID, "synthetic-prescription-1")?;
    Ok((MapMode::compile(written)?, composition))
}

#[test]
fn an_order_composition_maps_to_one_mpd_prescription_with_its_product() -> TestResult {
    let (mode, composition) = compiled()?;
    let page = mode.page(&composition, PATIENT, "2026-11-04T10:00:00Z")?;
    let [prescription] = page.prescriptions.as_slice() else {
        return Err(format!("{} prescriptions, not one", page.prescriptions.len()).into());
    };
    assert_eq!(
        vec![(
            medication::PRESCRIPTION_SYSTEM.to_owned(),
            "synthetic-prescription-1".to_owned()
        )],
        prescription.identifiers
    );
    let resource = &prescription.kept.resource;
    let profiles = resource
        .get("meta")
        .and_then(|meta| meta.get("profile"))
        .ok_or("no meta.profile")?;
    assert_eq!(
        Some(MPD_MEDICATION_REQUEST),
        match profiles {
            Value::Array(held) => held.first().and_then(Value::as_str),
            _ => None,
        },
        "the mapped resource claims MedicationRequest-eu-mpd"
    );
    let subject = resource
        .get("subject")
        .and_then(|subject| subject.get("identifier"))
        .ok_or("no subject identifier")?;
    assert_eq!(
        Some(PATIENT.1),
        subject.get("value").and_then(Value::as_str),
        "FHIR R4 Reference.identifier names the patient the search names"
    );
    let included: Vec<&str> = page
        .included
        .iter()
        .map(|kept| kept.resource_type.as_str())
        .collect();
    assert_eq!(
        vec!["Medication"],
        included,
        "the product, and no Provenance"
    );
    Ok(())
}

#[test]
fn a_template_the_mpd_project_does_not_map_is_refused() -> TestResult {
    let opt = std::fs::read_to_string(format!(
        "{}/../../tools/ferrofed-testkit/fixtures/eehrxf/opt/medication.opt",
        env!("CARGO_MANIFEST_DIR")
    ))?;
    let refused = MapMode::compile(Template::Opt14(&opt));
    assert!(
        matches!(refused, Err(MapModeError::Compile(_))),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_template_whose_archetypes_are_missing_is_refused() -> TestResult {
    let template = medication::template()?.template;
    let refused = MapMode::compile(Template::Adl2 {
        template: &template,
        archetypes: &[],
    });
    assert!(
        matches!(refused, Err(MapModeError::Operational(_))),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_template_that_does_not_parse_is_refused() {
    let refused = MapMode::compile(Template::Adl2 {
        template: "not ADL",
        archetypes: &[],
    });
    assert!(
        matches!(refused, Err(MapModeError::Syntax { .. })),
        "{refused:?}"
    );
}

/// A received dispensation with its product contained.
const DISPENSE: &str = r##"{"resourceType":"MedicationDispense","meta":{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd"]},
  "extension":[{"url":"http://hl7.org/fhir/5.0/StructureDefinition/extension-MedicationDispense.recorded","valueDateTime":"2026-03-03T10:00:00+01:00"}],
  "contained":[{"resourceType":"Medication","id":"med","meta":{"profile":["http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd"]},
    "code":{"coding":[{"system":"http://example.org/fhir/CodeSystem/ferrofed-products","code":"SYN-PRODUCT-0001","display":"Synthetic tablet 500 mg"}]}}],
  "identifier":[{"system":"urn:oid:2.999.6.1","value":"SYNTHETIC-MD-1"}],
  "status":"completed","medicationReference":{"reference":"#med"},
  "subject":{"identifier":{"system":"urn:oid:2.999.1","value":"synthetic-patient-1"}},
  "performer":[{"actor":{"display":"Synthetic Pharmacist"}}],
  "quantity":{"value":20,"unit":"tablet","system":"http://unitsofmeasure.org","code":"{tbl}"},
  "whenHandedOver":"2026-03-03T10:05:00+01:00"}"##;

#[test]
fn a_dispensation_maps_into_one_composition_naming_the_order() -> TestResult {
    let (mode, composition) = compiled()?;
    let order = mapped::order_in(&composition, UID, 0).ok_or("the composition names its order")?;
    let mode = mode.with_defaults("en", "NL");
    let composed = mode.dispensation(
        DISPENSE,
        (&order, mapped::ORDER_ACTIVITY),
        "2026-11-04T10:00:00Z",
    )?;
    let canonical: serde_json::Value = serde_json::from_str(composed.composition())?;
    let actions: Vec<&serde_json::Value> = canonical["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["_type"] == "ACTION")
        .collect();
    let [action] = actions.as_slice() else {
        return Err(format!("{} actions, not one", actions.len()).into());
    };
    assert_eq!(
        action.pointer("/instruction_details/instruction_id/id/value"),
        Some(&serde_json::Value::from(UID)),
        "RM INSTRUCTION_DETAILS.instruction_id"
    );
    assert_eq!(
        action.pointer("/instruction_details/activity_id"),
        Some(&serde_json::Value::from("activities[at0001]"))
    );
    Ok(())
}

#[test]
fn a_dispensation_with_no_language_and_territory_is_refused() -> TestResult {
    let (mode, composition) = compiled()?;
    let order = mapped::order_in(&composition, UID, 0).ok_or("the composition names its order")?;
    let refused = mode.dispensation(
        DISPENSE,
        (&order, mapped::ORDER_ACTIVITY),
        "2026-11-04T10:00:00Z",
    );
    assert!(
        matches!(refused, Err(MapModeError::Undefaulted)),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn an_order_the_composition_does_not_hold_names_nothing() -> TestResult {
    let (_, composition) = compiled()?;
    assert!(
        mapped::order_in(&composition, UID, 1).is_none(),
        "one order only"
    );
    assert!(mapped::order_in(&composition, "no version", 0).is_none());
    Ok(())
}
