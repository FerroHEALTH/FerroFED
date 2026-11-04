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
    let mapping = Mapping::compile_project(source, &MPD)?;
    let received = ReceivedResources::read(&collection(prescription))?;
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
