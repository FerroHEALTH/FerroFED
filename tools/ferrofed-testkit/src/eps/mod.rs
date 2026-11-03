// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary fixtures and the structural check of a document
//! against the vendored HL7 Europe Patient Summary profiles (#689).
//!
//! The fixtures are a synthetic template, a synthetic FHIRconnect model
//! mapping and a context mapping to the HL7 Europe `allergyIntolerance-eu-core`
//! profile, so the face can feed its allergies section; the compositions are
//! synthetic too. [`profile::check`] reads the `bundle-eu-eps`, `composition-eu-eps`
//! and `patient-eu-eps` profiles from the vendored package and holds a
//! document to what they and the FHIR R4 document rules state: the required
//! elements and slices, the fixed codes, the section rules `cmp-1` and
//! `cmp-2`, the document rules `bdl-7`, `bdl-9`, `bdl-10` and `bdl-11`, and the
//! profile's reference invariants. It answers every finding, so a test
//! names what is wrong. HL7's FHIR Validator over the same documents is the
//! full check (#688).
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the vendored profiles and the document are read as JSON"
)]

pub mod profile;

use std::path::PathBuf;

use serde_json::Value;

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The context mapping the fixture files declare.
pub const CONTEXT: &str = "ferrofed_eu_allergy.context";

/// The synthetic template the fixture maps.
pub const TEMPLATE: &str = "ferrofed.eehrxf.allergy.v1";

/// Returns the path of the synthetic operational template.
#[must_use]
pub fn opt() -> PathBuf {
    PathBuf::from(ROOT).join("tools/ferrofed-testkit/fixtures/eehrxf/opt/allergy.opt")
}

/// Returns the model and context mapping files of the fixture mapping.
#[must_use]
pub fn mapping_files() -> [PathBuf; 2] {
    [
        PathBuf::from(ROOT)
            .join("tools/ferrofed-testkit/fixtures/eehrxf/mapping/ferrofed_allergy.yml"),
        PathBuf::from(ROOT)
            .join("tools/ferrofed-testkit/fixtures/eehrxf/mapping/eu_allergy.context.yml"),
    ]
}

/// Returns a canonical composition of the template `template`, its uid
/// `uid`, recording the synthetic substance `substance`.
///
/// Its clinical status is `active`, and [`TEMPLATE`] is the template the
/// fixture mapping maps.
///
/// Its entry is the synthetic allergy evaluation the fixture maps. A mock
/// member answers a section query with it as it stands: no CDR evaluates the
/// query in a test that uses it.
#[must_use]
pub fn allergy_composition(uid: &str, template: &str, substance: &str) -> String {
    serde_json::json!({
        "_type": "COMPOSITION",
        "name": { "_type": "DV_TEXT", "value": "Synthetic allergy summary" },
        "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1" },
            "template_id": { "_type": "TEMPLATE_ID", "value": template },
            "rm_version": "1.1.0"
        },
        "uid": { "_type": "OBJECT_VERSION_ID", "value": uid },
        "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
        "territory": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }, "code_string": "NL" },
        "category": {
            "_type": "DV_CODED_TEXT",
            "value": "persistent",
            "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "431" }
        },
        "composer": { "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" },
        "content": [{
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
                "items": [{
                    "_type": "ELEMENT",
                    "name": { "_type": "DV_TEXT", "value": "Substance" },
                    "archetype_node_id": "at0002",
                    "value": { "_type": "DV_TEXT", "value": substance }
                }, {
                    "_type": "ELEMENT",
                    "name": { "_type": "DV_TEXT", "value": "Clinical status" },
                    "archetype_node_id": "at0003",
                    "value": {
                        "_type": "DV_CODED_TEXT",
                        "value": "Active",
                        "defining_code": {
                            "_type": "CODE_PHRASE",
                            "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical" },
                            "code_string": "active",
                            "preferred_term": "Active"
                        }
                    }
                }]
            }
        }]
    })
    .to_string()
}

/// The synthetic medication template's id
/// (`tools/ferrofed-testkit/fixtures/eehrxf/opt/medication.opt`).
pub const MEDICATION_TEMPLATE: &str = "ferrofed.eehrxf.medication.v1";

/// The context that maps the synthetic medication use to
/// `MedicationStatement-eu-eps`, for every medicine that is no vaccine.
pub const MEDICATION_CONTEXT: &str = "ferrofed_eu_medication.context";

/// The context that maps the synthetic medication order to
/// `MedicationStatement-eu-eps`.
pub const MEDICATION_ORDER_CONTEXT: &str = "ferrofed_eu_medication_order.context";

/// The context that maps the synthetic medication use whose medicine is the
/// synthetic vaccine code to `immunization-eu-core`.
pub const IMMUNISATION_CONTEXT: &str = "ferrofed_eu_immunisation.context";

/// The synthetic vaccine code.
///
/// A synthetic medication use of this medicine carries the kind "vaccine"
/// (`at0006`), which the immunisations' context maps, and every other
/// medicine the kind "medicine" (`at0005`), which the medication summary's
/// maps.
pub const VACCINE: &str = "J07BB02";

/// Returns the path of the synthetic medication operational template.
#[must_use]
pub fn medication_opt() -> PathBuf {
    PathBuf::from(ROOT).join("tools/ferrofed-testkit/fixtures/eehrxf/opt/medication.opt")
}

/// Returns the model and context mapping files of the synthetic medication
/// fixtures: the medication use, the medication order and the vaccination,
/// with a context each.
#[must_use]
pub fn medication_mapping_files() -> Vec<PathBuf> {
    [
        "ferrofed_medication.yml",
        "ferrofed_medication_order.yml",
        "ferrofed_immunisation.yml",
        "eu_medication.context.yml",
        "eu_medication_order.context.yml",
        "eu_immunisation.context.yml",
    ]
    .into_iter()
    .map(|name| {
        PathBuf::from(ROOT)
            .join("tools/ferrofed-testkit/fixtures/eehrxf/mapping")
            .join(name)
    })
    .collect()
}

/// One synthetic medicine as a composition records it: its code in the ATC
/// system, its display, and its route's code in the EDQM standard terms.
#[derive(Debug, Clone, Copy)]
pub struct Medicine<'a> {
    /// The ATC code.
    pub code: &'a str,
    /// The display of the code.
    pub display: &'a str,
    /// The route's EDQM standard term code.
    pub route: &'a str,
}

/// Returns a synthetic composition of the medication template with the uid
/// `uid`: one synthetic medication use per entry of `used`, and one
/// synthetic medication order per entry of `ordered`.
#[must_use]
pub fn medication_composition(
    uid: &str,
    used: &[Medicine<'_>],
    ordered: &[Medicine<'_>],
) -> String {
    let items = |medicine: &Medicine<'_>, first: &str, second: &str| {
        serde_json::json!([{
            "_type": "ELEMENT",
            "name": { "_type": "DV_TEXT", "value": "Medicine" },
            "archetype_node_id": first,
            "value": coded_text(medicine.display, "http://www.whocc.no/atc", medicine.code)
        }, {
            "_type": "ELEMENT",
            "name": { "_type": "DV_TEXT", "value": "Route" },
            "archetype_node_id": second,
            "value": coded_text("Synthetic route", "urn:oid:0.4.0.127.0.16.1.1.2.1", medicine.route)
        }])
    };
    let used_items = |medicine: &Medicine<'_>| {
        let (kind, code) = if medicine.code == VACCINE {
            ("Vaccine", "at0006")
        } else {
            ("Medicine", "at0005")
        };
        let mut listed = items(medicine, "at0002", "at0003");
        if let Value::Array(listed) = &mut listed {
            listed.push(serde_json::json!({
                "_type": "ELEMENT",
                "name": { "_type": "DV_TEXT", "value": "Kind" },
                "archetype_node_id": "at0004",
                "value": coded_text(kind, "local", code)
            }));
        }
        listed
    };
    let mut content: Vec<Value> = used
        .iter()
        .map(|medicine| {
            entry(
                "EVALUATION",
                "Synthetic medication use",
                "openEHR-EHR-EVALUATION.ferrofed_medication.v1",
                serde_json::json!({
                    "data": {
                        "_type": "ITEM_TREE",
                        "name": { "_type": "DV_TEXT", "value": "Tree" },
                        "archetype_node_id": "at0001",
                        "items": used_items(medicine)
                    }
                }),
            )
        })
        .collect();
    content.extend(ordered.iter().map(|medicine| {
        entry(
            "INSTRUCTION",
            "Synthetic medication order",
            "openEHR-EHR-INSTRUCTION.ferrofed_medication_order.v1",
            serde_json::json!({
                "narrative": { "_type": "DV_TEXT", "value": "Synthetic order" },
                "activities": [{
                    "_type": "ACTIVITY",
                    "name": { "_type": "DV_TEXT", "value": "Order" },
                    "archetype_node_id": "at0001",
                    "action_archetype_id": "openEHR-EHR-ACTION\\.medication\\.v1",
                    "description": {
                        "_type": "ITEM_TREE",
                        "name": { "_type": "DV_TEXT", "value": "Tree" },
                        "archetype_node_id": "at0002",
                        "items": items(medicine, "at0003", "at0004")
                    }
                }]
            }),
        )
    }));
    serde_json::json!({
        "_type": "COMPOSITION",
        "name": { "_type": "DV_TEXT", "value": "Synthetic medication summary" },
        "archetype_node_id": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1",
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.ferrofed_summary.v1" },
            "template_id": { "_type": "TEMPLATE_ID", "value": MEDICATION_TEMPLATE },
            "rm_version": "1.1.0"
        },
        "uid": { "_type": "OBJECT_VERSION_ID", "value": uid },
        "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
        "territory": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }, "code_string": "NL" },
        "category": {
            "_type": "DV_CODED_TEXT",
            "value": "persistent",
            "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "431" }
        },
        "composer": { "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" },
        "content": content
    })
    .to_string()
}

/// A synthetic `DV_CODED_TEXT` of `code` in `system`, displayed `display`.
fn coded_text(display: &str, system: &str, code: &str) -> Value {
    serde_json::json!({
        "_type": "DV_CODED_TEXT",
        "value": display,
        "defining_code": {
            "_type": "CODE_PHRASE",
            "terminology_id": { "_type": "TERMINOLOGY_ID", "value": system },
            "code_string": code,
            "preferred_term": display
        }
    })
}

/// A synthetic entry of the RM class `class`, named `name`, of the
/// archetype `archetype`, with the attributes `body` adds.
fn entry(class: &str, name: &str, archetype: &str, body: Value) -> Value {
    let mut entry = serde_json::json!({
        "_type": class,
        "name": { "_type": "DV_TEXT", "value": name },
        "archetype_node_id": archetype,
        "archetype_details": {
            "_type": "ARCHETYPED",
            "archetype_id": { "_type": "ARCHETYPE_ID", "value": archetype },
            "rm_version": "1.1.0"
        },
        "language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
        "encoding": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }, "code_string": "UTF-8" },
        "subject": { "_type": "PARTY_SELF" }
    });
    if let (Value::Object(entry), Value::Object(body)) = (&mut entry, body) {
        entry.extend(body);
    }
    entry
}

/// Returns a synthetic patient summary document in the exchange format.
///
/// It carries the five sections the EPS `Composition` profile requires,
/// each one with no entry stating its `emptyReason` (`ips-comp-1`), every
/// coding displayed with a synthetic text (the receive path refuses a coding
/// with no display when no terminology server is configured), one active
/// allergy that claims the `allergyIntolerance-eu-core` profile the fixture
/// mapping maps, recording `substance` (`ait-1` asks its `clinicalStatus`),
/// and the patient, with a synthetic name and birth date (`patient-eu-eps`
/// requires both) and identified by each `(system, value)` of `identifiers`. FerroBRIDGE's
/// validator holds it conformant to the HL7 Europe Patient Summary.
///
/// Every reference is a relative `Type/id` under `example.org` full URLs,
/// the spelling the FHIRconnect engine resolves.
#[must_use]
pub fn received_document(identifiers: &[(&str, &str)], substance: &str) -> String {
    let identifier: Vec<Value> = identifiers
        .iter()
        .map(|(system, value)| serde_json::json!({ "system": system, "value": value }))
        .collect();
    let section = |title: &str, code: &str, text: &str| {
        serde_json::json!({
            "title": title,
            "code": { "coding": [{ "system": "http://loinc.org", "code": code, "display": title }] },
            "text": {
                "status": "generated",
                "div": format!("<div xmlns=\"http://www.w3.org/1999/xhtml\">{text}</div>")
            },
            "emptyReason": { "coding": [{
                "system": "http://terminology.hl7.org/CodeSystem/list-empty-reason",
                "code": "unavailable",
                "display": "Unavailable"
            }] }
        })
    };
    let mut allergies = section("Allergies", "48765-2", substance);
    if let Some(allergies) = allergies.as_object_mut() {
        allergies.remove("emptyReason");
        allergies.insert(
            String::from("entry"),
            serde_json::json!([{ "reference": "AllergyIntolerance/synthetic-allergy" }]),
        );
    }
    serde_json::json!({
        "resourceType": "Bundle",
        "identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000802" },
        "type": "document",
        "timestamp": "2026-10-06T10:00:00Z",
        "entry": [
            {
                "fullUrl": "http://example.org/fhir/Composition/synthetic-composition",
                "resource": {
                    "resourceType": "Composition",
                    "id": "synthetic-composition",
                    "meta": { "profile": ["http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps"] },
                    "identifier": { "system": "urn:ietf:rfc:3986", "value": "urn:uuid:0c3e1d20-0000-4000-8000-000000000803" },
                    "status": "final",
                    "type": { "coding": [{ "system": "http://loinc.org", "code": "60591-5", "display": "Synthetic patient summary" }] },
                    "subject": { "reference": "Patient/synthetic-patient" },
                    "date": "2026-10-06T10:00:00Z",
                    "author": [{ "display": "Synthetic author" }],
                    "title": "Synthetic patient summary",
                    "section": [
                        section("Problems", "11450-4", "No problems recorded"),
                        allergies,
                        section("Medication", "10160-0", "No medication recorded"),
                        section("Procedures", "47519-4", "No procedures recorded"),
                        section("Medical devices", "46264-8", "No devices recorded"),
                    ]
                }
            },
            {
                "fullUrl": "http://example.org/fhir/Patient/synthetic-patient",
                "resource": {
                    "resourceType": "Patient",
                    "id": "synthetic-patient",
                    "identifier": identifier,
                    "name": [{ "family": "Synthetic", "given": ["Example"] }],
                    "birthDate": "1970-01-01"
                }
            },
            {
                "fullUrl": "http://example.org/fhir/AllergyIntolerance/synthetic-allergy",
                "resource": {
                    "resourceType": "AllergyIntolerance",
                    "id": "synthetic-allergy",
                    "meta": { "profile": ["http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core"] },
                    "clinicalStatus": { "coding": [{
                        "system": "http://terminology.hl7.org/CodeSystem/allergyintolerance-clinical",
                        "code": "active",
                        "display": "Active"
                    }] },
                    "code": { "text": substance },
                    "patient": { "reference": "Patient/synthetic-patient" }
                }
            }
        ]
    })
    .to_string()
}
