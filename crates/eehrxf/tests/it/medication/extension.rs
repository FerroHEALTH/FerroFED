// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `StructureDefinition` of the remaining-dispensations extension, held
//! to the FHIR R4 4.0.1 rules for an extension definition: 2.5.1 Defining
//! Extensions and the invariants of the `StructureDefinition` resource, each
//! quoted from the vendored pages under `docs/specs/fhir-r4/`.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;
use std::path::PathBuf;

use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
use eehrxf::medication::prescription::REMAINING_DISPENSATIONS;
use eehrxf::medication::prescription::REMAINING_DISPENSATIONS_DEFINITION;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::structure_definition::StructureDefinition;

use crate::support::mpd;

/// The vendored FHIR R4 page `name`.
fn page(name: &str) -> Result<String, Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/specs/fhir-r4")
        .join(name);
    Ok(std::fs::read_to_string(path)?)
}

/// The definition, as a JSON object.
fn definition() -> Result<Object, Box<dyn Error>> {
    match serde_json::from_str(REMAINING_DISPENSATIONS_DEFINITION)? {
        Value::Object(object) => Ok(object),
        _ => Err("the definition is not a JSON object".into()),
    }
}

/// The differential elements of the definition.
fn elements(definition: &Object) -> Result<Vec<&Object>, Box<dyn Error>> {
    definition
        .get("differential")
        .and_then(|differential| differential.get("element"))
        .and_then(Value::as_array)
        .ok_or("the definition has no differential elements")?
        .iter()
        .map(|element| {
            element
                .as_object()
                .ok_or_else(|| "an element is not an object".into())
        })
        .collect()
}

/// The differential element whose id is `id`.
fn element<'a>(definition: &'a Object, id: &str) -> Result<&'a Object, Box<dyn Error>> {
    elements(definition)?
        .into_iter()
        .find(|element| element.get("id").and_then(Value::as_str) == Some(id))
        .ok_or_else(|| format!("no element {id}").into())
}

/// The boolean `value` holds, when it is one.
fn boolean(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(flag) => Some(*flag),
        _ => None,
    }
}

/// The non-negative integer `value` holds, when it is one.
fn count(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        _ => None,
    }
}

/// The string member `name` of `object`.
fn text<'a>(object: &'a Object, name: &str) -> Option<&'a str> {
    object.get(name).and_then(Value::as_str)
}

#[test]
fn the_rules_the_tests_hold_are_the_vendored_r4_text() -> Result<(), Box<dyn Error>> {
    let defining = page("defining-extensions.html")?;
    for rule in [
        "Extensions SHALL only be used on a target that appears in their context list",
        "Element Ids are unique within the base specification",
        "Cardinality = 1...1 (fixed)",
        "value = canonical URL (fixed)",
        "Cardinality for Simple extensions (not nested): 1..1",
        "Use 0..0 if a simple (non-nested extension)",
    ] {
        assert!(defining.contains(rule), "2.5.1 does not say: {rule}");
    }
    let structure = page("structuredefinition.html")?;
    for rule in [
        "If the structure defines an extension then the structure must have context information",
        "If the structure is not abstract, then there SHALL be a baseDefinition",
        "All element definitions must have an id",
        "All element definitions must have unique ids (diff)",
        "In any differential, all the elements must start with the StructureDefinition's specified type",
        "No slice name on root",
    ] {
        assert!(
            structure.contains(rule),
            "StructureDefinition does not say: {rule}"
        );
    }
    let extensibility = page("extensibility.html")?;
    assert!(extensibility.contains("Must have either extensions or value[x], not both"));
    Ok(())
}

#[test]
fn the_definition_is_an_r4_structure_definition_read_back_unchanged() -> Result<(), Box<dyn Error>>
{
    let object = definition()?;
    let read = StructureDefinition::from_json(&object, &mut Path::root("StructureDefinition"))?;
    assert_eq!(read.to_json()?, object);
    assert_eq!(text(&object, "resourceType"), Some("StructureDefinition"));
    assert_eq!(text(&object, "fhirVersion"), Some("4.0.1"));
    Ok(())
}

#[test]
fn the_definition_is_an_extension_constraint_with_the_canonical_url() -> Result<(), Box<dyn Error>>
{
    let object = definition()?;
    assert_eq!(text(&object, "url"), Some(REMAINING_DISPENSATIONS));
    assert_eq!(
        REMAINING_DISPENSATIONS.rsplit('/').next(),
        text(&object, "id")
    );
    assert_eq!(text(&object, "type"), Some("Extension"));
    assert_eq!(text(&object, "kind"), Some("complex-type"));
    assert_eq!(object.get("abstract").and_then(boolean), Some(false));
    assert_eq!(
        text(&object, "baseDefinition"),
        Some("http://hl7.org/fhir/StructureDefinition/Extension")
    );
    assert_eq!(text(&object, "derivation"), Some("constraint"));
    assert_eq!(object.get("contextInvariant"), None);
    Ok(())
}

#[test]
fn the_context_is_the_dispense_request_of_a_medication_request() -> Result<(), Box<dyn Error>> {
    let object = definition()?;
    let contexts = object
        .get("context")
        .and_then(Value::as_array)
        .ok_or("the definition has no context")?;
    let [context] = contexts else {
        return Err(format!("one context: {contexts:?}").into());
    };
    let context = context.as_object().ok_or("the context is not an object")?;
    assert_eq!(text(context, "type"), Some("element"));
    let expression = text(context, "expression").ok_or("the context has no expression")?;
    assert_eq!(expression, "MedicationRequest.dispenseRequest");
    let profile = mpd(MPD_MEDICATION_REQUEST)?;
    assert!(
        profile.element(expression).is_some(),
        "the MPD MedicationRequest has no element {expression}"
    );
    Ok(())
}

#[test]
fn every_element_has_a_unique_id_equal_to_its_path_under_the_type() -> Result<(), Box<dyn Error>> {
    let object = definition()?;
    let elements = elements(&object)?;
    let mut ids = BTreeSet::new();
    for element in &elements {
        let id = text(element, "id").ok_or("an element has no id")?;
        assert_eq!(text(element, "path"), Some(id));
        assert!(id == "Extension" || id.starts_with("Extension."), "{id}");
        assert!(ids.insert(id), "the id {id} repeats");
        assert_eq!(element.get("sliceName"), None, "{id}");
    }
    assert_eq!(
        ids,
        BTreeSet::from([
            "Extension",
            "Extension.extension",
            "Extension.url",
            "Extension.value[x]",
        ])
    );
    let first = elements.first().ok_or("no element")?;
    assert_eq!(text(first, "id"), Some("Extension"));
    assert_eq!(first.get("type"), None);
    Ok(())
}

#[test]
fn the_extension_is_simple_at_most_once_with_an_unsigned_int() -> Result<(), Box<dyn Error>> {
    let object = definition()?;
    let root = element(&object, "Extension")?;
    assert_eq!(root.get("min").and_then(count), Some(0));
    assert_eq!(text(root, "max"), Some("1"));
    assert_eq!(
        text(element(&object, "Extension.extension")?, "max"),
        Some("0")
    );
    assert_eq!(
        text(element(&object, "Extension.url")?, "fixedUri"),
        Some(REMAINING_DISPENSATIONS)
    );
    let value = element(&object, "Extension.value[x]")?;
    assert_eq!(value.get("min").and_then(count), Some(1));
    let types = value
        .get("type")
        .and_then(Value::as_array)
        .ok_or("the value has no type")?;
    let codes: Vec<_> = types
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|kind| text(kind, "code"))
        .collect();
    assert_eq!(codes, ["unsignedInt"]);
    assert_eq!(types.len(), 1);
    Ok(())
}
