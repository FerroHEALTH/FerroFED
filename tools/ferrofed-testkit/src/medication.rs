// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The medication fixtures of the prescription face's map mode (#1560).
//!
//! They are FerroBRIDGE's ADL 2 medication template and the CKM archetypes
//! it specialises, vendored under `docs/specs/ferrobridge-medication/` and
//! `docs/specs/openehr-ckm-medication/`, which the MPD project `eehrxf`
//! ships maps over, and a synthetic medication order composition as a
//! member serves it. The composition is written by the component's own
//! mapping from a synthetic `MedicationRequest-eu-mpd` and the product it
//! references, so its shape is the one the template gives; its patient,
//! product and prescriber are invented for the tests.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the synthetic resources are written as JSON"
)]

use std::error::Error;
use std::path::PathBuf;

use eehrxf::mapping::Mapping;
use eehrxf::mappings::MPD;
use eehrxf::receive::ReceivedResources;
use fhirconnect::engine::traverse::Defaults;
use fhirconnect::operations::run::Settings;
use openehr_mapping_core::template::TemplateSource;
use serde_json::Value;

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The template the shipped MPD project maps.
pub const TEMPLATE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_medication.v1.0.0";

/// The archetypes of the template held beside it.
const BESIDE: [&str; 5] = [
    "openEHR-EHR-COMPOSITION.encounter.v1.adl",
    "openEHR-EHR-CLUSTER.medication.v2.adl",
    "openEHR-EHR-CLUSTER.therapeutic_direction.v1.adl",
    "openEHR-EHR-CLUSTER.dosage.v2.adl",
    "openEHR-EHR-CLUSTER.timing_daily.v1.adl",
];

/// The archetypes of the template the CKM medication corpus holds.
const CKM: [&str; 5] = [
    "openEHR-EHR-INSTRUCTION.medication_order.v3.adl",
    "openEHR-EHR-ACTION.medication.v1.adl",
    "openEHR-EHR-CLUSTER.medication_authorisation.v0.adl",
    "openEHR-EHR-CLUSTER.medication_order_summary.v0.adl",
    "openEHR-EHR-CLUSTER.medication_supply_amount.v0.adl",
];

/// The business identifier system of the synthetic prescriptions, in the
/// `example.org` domain RFC 6761 §6.5 reserves.
pub const PRESCRIPTION_SYSTEM: &str = "http://example.org/fhir/sid/ferrofed-prescriptions";

/// Returns the path of the vendored ADL 2 template.
#[must_use]
pub fn template_path() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/ferrobridge-medication/ferrobridge_medication.adlt")
}

/// Returns the paths of the vendored archetypes the template specialises,
/// in the two directories that hold them.
#[must_use]
pub fn archetype_paths() -> Vec<PathBuf> {
    let root = PathBuf::from(ROOT).join("docs/specs");
    BESIDE
        .iter()
        .map(|file| root.join("ferrobridge-medication").join(file))
        .chain(
            CKM.iter()
                .map(|file| root.join("openehr-ckm-medication").join(file)),
        )
        .collect()
}

/// The template's text, and each archetype as its file name and text.
#[derive(Debug, Clone)]
pub struct Written {
    /// The template's text.
    pub template: String,
    /// Each archetype, its file name and its text.
    pub archetypes: Vec<(String, String)>,
}

/// Returns the template's text and each archetype as its file name and
/// text.
///
/// # Errors
///
/// When a vendored file cannot be read.
pub fn template() -> Result<Written, Box<dyn Error>> {
    let template = std::fs::read_to_string(template_path())?;
    let mut archetypes = Vec::new();
    for path in archetype_paths() {
        let name = path
            .file_name()
            .ok_or("an archetype path names no file")?
            .to_string_lossy()
            .into_owned();
        archetypes.push((name, std::fs::read_to_string(&path)?));
    }
    Ok(Written {
        template,
        archetypes,
    })
}

/// The run settings of the composition a synthetic order is written into:
/// a synthetic engine device, a fixed instant and the composition defaults
/// a collection does not state.
fn settings() -> Settings {
    let now = "2026-03-02T09:00:00Z";
    Settings::new("Device/ferrofed-testkit", now)
        .with_defaults(Defaults::at(now).with_language("en").with_territory("NL"))
}

/// A synthetic `collection` of one `MedicationRequest-eu-mpd` of the
/// business identifier `prescription`, and the product it references.
fn collection(prescription: &str) -> String {
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            {
                "fullUrl": "http://example.org/fhir/MedicationRequest/synthetic",
                "resource": {
                    "resourceType": "MedicationRequest",
                    "meta": {"profile": ["http://hl7.eu/fhir/mpd/StructureDefinition/MedicationRequest-eu-mpd"]},
                    "identifier": [{"system": PRESCRIPTION_SYSTEM, "value": prescription}],
                    "status": "unknown",
                    "intent": "order",
                    "medicationReference": {"reference": "Medication/synthetic-product"},
                    "subject": {"reference": "Patient/synthetic-patient"},
                    "authoredOn": "2026-03-02T09:00:00+01:00",
                    "requester": {"display": "Synthetic Prescriber"},
                    "dosageInstruction": [{"sequence": 1, "text": "One tablet twice a day"}]
                }
            },
            {
                "fullUrl": "http://example.org/fhir/Medication/synthetic-product",
                "resource": {
                    "resourceType": "Medication",
                    "id": "synthetic-product",
                    "meta": {"profile": ["http://hl7.eu/fhir/mpd/StructureDefinition/Medication-eu-mpd"]},
                    "code": {"coding": [{
                        "system": "http://example.org/fhir/CodeSystem/ferrofed-products",
                        "code": "SYN-PRODUCT-0001",
                        "display": "Synthetic tablet 500 mg"
                    }]}
                }
            }
        ]
    })
    .to_string()
}

/// Returns a synthetic medication order composition as a member serves it.
///
/// It is of `source`, the operational template of [`TEMPLATE`], under the
/// version id `uid`, holding one order of the business identifier
/// `prescription`, with no `feeder_audit`, as a member's own order has none.
///
/// # Errors
///
/// When the shipped project does not compile against `source` or refuses
/// the synthetic order.
pub fn order_composition(
    source: &TemplateSource,
    uid: &str,
    prescription: &str,
) -> Result<String, Box<dyn Error>> {
    composed(
        &Mapping::compile_project(source, &MPD)?,
        &collection(prescription),
        uid,
    )
}

/// Returns the composition `mapping` writes from the `collection` of
/// resources, under the version id `uid`, with no `feeder_audit`, as a
/// member's own composition has none.
///
/// # Errors
///
/// When the mapping refuses the collection.
pub(crate) fn composed(
    mapping: &Mapping,
    collection: &str,
    uid: &str,
) -> Result<String, Box<dyn Error>> {
    let received = ReceivedResources::read(collection)?;
    let composed = mapping.resources_to_openehr(&received, &settings())?;
    let mut canonical: Value = serde_json::from_str(composed.composition())?;
    let object = canonical
        .as_object_mut()
        .ok_or("the composition is no object")?;
    object.remove("feeder_audit");
    object.insert(
        String::from("uid"),
        serde_json::json!({"_type": "OBJECT_VERSION_ID", "value": uid}),
    );
    Ok(canonical.to_string())
}

/// The patient summary template the shipped EPS project maps over.
pub const PATIENT_SUMMARY_TEMPLATE: &str =
    "openEHR-EHR-COMPOSITION.ferrobridge_patient_summary.v1.0.0";

/// The archetypes FerroBRIDGE's patient summary template specialises beyond
/// the medication template's.
const SUMMARY_BESIDE: [&str; 3] = [
    "openEHR-EHR-COMPOSITION.health_summary.v1.adl",
    "openEHR-EHR-SECTION.adhoc.v1.adl",
    "openEHR-EHR-SECTION.immunisation_list.v0.adl",
];

/// Returns FerroBRIDGE's ADL 2 patient summary template and every archetype
/// it specialises, each as its file name and text.
///
/// # Errors
///
/// When a vendored file cannot be read.
pub fn patient_summary_template() -> Result<Written, Box<dyn Error>> {
    let root = PathBuf::from(ROOT).join("docs/specs/ferrobridge-medication");
    let mut written = template()?;
    written.template = std::fs::read_to_string(root.join("ferrobridge_patient_summary.adlt"))?;
    for file in SUMMARY_BESIDE {
        written
            .archetypes
            .push((file.to_owned(), std::fs::read_to_string(root.join(file))?));
    }
    Ok(written)
}

/// The language, encoding and subject of a synthetic entry.
pub(crate) const ENTRY: &str = r#""language": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }, "code_string": "en" },
  "encoding": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "IANA_character-sets" }, "code_string": "UTF-8" },
  "subject": { "_type": "PARTY_SELF" }"#;

/// The archetype details of `id`.
pub(crate) fn details(id: &str) -> String {
    format!(
        r#""archetype_node_id": "{id}", "archetype_details": {{ "_type": "ARCHETYPED", "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "{id}" }}, "rm_version": "1.1.0" }}"#
    )
}

/// A synthetic coded text of the local synthetic code system.
fn coded(value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/ferrofed-products" }}, "code_string": "{code}" }} }}"#
    )
}

/// A synthetic medication order of the medicine `code`.
pub(crate) fn summary_order(code: &str) -> String {
    format!(
        r#"{{
  "_type": "INSTRUCTION",
  "name": {{ "_type": "DV_TEXT", "value": "Medication order" }},
  {details},
  {ENTRY},
  "narrative": {{ "_type": "DV_TEXT", "value": "Synthetic medication order" }},
  "activities": [{{
    "_type": "ACTIVITY",
    "name": {{ "_type": "DV_TEXT", "value": "Order" }},
    "archetype_node_id": "at0001",
    "description": {{
      "_type": "ITEM_TREE",
      "name": {{ "_type": "DV_TEXT", "value": "Tree" }},
      "archetype_node_id": "at0002",
      "items": [
        {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0070", "value": {medicine} }},
        {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Overall directions description" }}, "archetype_node_id": "at0009", "value": {{ "_type": "DV_TEXT", "value": "One synthetic tablet each morning" }} }},
        {{ "_type": "CLUSTER", "name": {{ "_type": "DV_TEXT", "value": "Order details" }}, "archetype_node_id": "at0113", "items": [
          {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Order start date/time" }}, "archetype_node_id": "at0012", "value": {{ "_type": "DV_DATE_TIME", "value": "2026-10-01T08:00:00Z" }} }}
        ] }}
      ]
    }},
    "timing": {{ "_type": "DV_PARSABLE", "value": "R/2026-10-01T08:00:00Z/P1D", "formalism": "ISO8601" }},
    "action_archetype_id": "openEHR-EHR-ACTION\\.medication\\.v1"
  }}],
  "protocol": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0005", "items": [] }}
}}"#,
        details = details("openEHR-EHR-INSTRUCTION.medication_order.v3"),
        medicine = coded("Synthetic tablet 500 mg", code),
    )
}

/// A synthetic medication action recording a dose of the vaccine `code`
/// administered.
fn administered(code: &str) -> String {
    format!(
        r#"{{
  "_type": "ACTION",
  "name": {{ "_type": "DV_TEXT", "value": "Vaccination administered" }},
  {details},
  {ENTRY},
  "time": {{ "_type": "DV_DATE_TIME", "value": "2026-09-15T09:30:00Z" }},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "active", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "245" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Dose administered", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0006" }} }}
  }},
  "description": {{
    "_type": "ITEM_TREE",
    "name": {{ "_type": "DV_TEXT", "value": "Tree" }},
    "archetype_node_id": "at0017",
    "items": [
      {{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "Medication item" }}, "archetype_node_id": "at0020", "value": {vaccine} }}
    ]
  }}
}}"#,
        details = details("openEHR-EHR-ACTION.medication.v1"),
        vaccine = coded("Synthetic vaccine", code),
    )
}

/// Returns a synthetic composition of [`PATIENT_SUMMARY_TEMPLATE`].
///
/// It is under the version id `uid`, the medicine `medicine` ordered in its
/// medication summary section and the vaccine `vaccine` administered as a
/// medication action in its vaccination list section.
#[must_use]
pub fn summary_composition(uid: &str, (medicine, vaccine): (&str, &str)) -> String {
    let section = |name: &str, archetype: &str, item: String| {
        format!(
            r#"{{ "_type": "SECTION", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, {}, "items": [{item}] }}"#,
            details(archetype)
        )
    };
    let content = [
        section(
            "Medication summary",
            "openEHR-EHR-SECTION.adhoc.v1",
            summary_order(medicine),
        ),
        section(
            "Immunisations",
            "openEHR-EHR-SECTION.immunisation_list.v0",
            administered(vaccine),
        ),
    ]
    .join(",");
    format!(
        r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Patient summary" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.health_summary.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.health_summary.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "{PATIENT_SUMMARY_TEMPLATE}" }},
    "rm_version": "1.1.0"
  }},
  {held}
  "context": {{
    "_type": "EVENT_CONTEXT",
    "start_time": {{ "_type": "DV_DATE_TIME", "value": "2026-10-01T08:00:00Z" }},
    "setting": {OTHER_CARE}
  }},
  "content": [{content}]
}}"#,
        held = held(uid),
    )
}

/// The uid, language, territory, category and composer of a synthetic
/// event composition in English of the Netherlands, under the version id
/// `uid`, as members of a canonical JSON object.
pub(crate) fn held(uid: &str) -> String {
    format!(
        r#""uid": {{ "_type": "OBJECT_VERSION_ID", "value": "{uid}" }},
  "language": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_639-1" }}, "code_string": "en" }},
  "territory": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "ISO_3166-1" }}, "code_string": "NL" }},
  "category": {{ "_type": "DV_CODED_TEXT", "value": "event", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "433" }} }},
  "composer": {{ "_type": "PARTY_IDENTIFIED", "name": "Synthetic composer" }},"#
    )
}

/// The `other care` setting of the openEHR terminology, a canonical JSON
/// `DV_CODED_TEXT`.
pub(crate) const OTHER_CARE: &str = r#"{ "_type": "DV_CODED_TEXT", "value": "other care", "defining_code": { "_type": "CODE_PHRASE", "terminology_id": { "_type": "TERMINOLOGY_ID", "value": "openehr" }, "code_string": "238" } }"#;
