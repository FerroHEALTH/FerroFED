// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report fixtures (#1405).
//!
//! They are FerroBRIDGE's ADL 2 discharge report template and the CKM
//! archetypes it specialises or holds, vendored under
//! `docs/specs/ferrobridge-discharge/` beside the medication archetypes of
//! `docs/specs/ferrobridge-medication/` and
//! `docs/specs/openehr-ckm-medication/`, which the HDR project `eehrxf`
//! ships maps over, and a synthetic discharge composition as a member serves
//! it. Its stay, diagnosis, procedure and medicine are invented for the
//! tests.

use std::error::Error;
use std::path::PathBuf;

use crate::medication::{
    ENTRY, OTHER_CARE, Written, details, held, patient_summary_template, summary_order,
};

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The template the shipped HDR project maps.
pub const TEMPLATE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_discharge_report.v1.0.0";

/// The archetypes the template adds to the medication ones.
const BESIDE: [&str; 3] = [
    "openEHR-EHR-COMPOSITION.transfer_summary.v1.adl",
    "openEHR-EHR-EVALUATION.problem_diagnosis.v1.adl",
    "openEHR-EHR-ACTION.procedure.v1.adl",
];

/// The synthetic diagnosis's code, in a code system under `example.org`.
pub const DIAGNOSIS: &str = "SYN-DIAGNOSIS-0001";

/// The synthetic procedure's code, in a code system under `example.org`.
pub const PROCEDURE: &str = "SYN-PROCEDURE-0001";

/// The synthetic medicine's code, in a code system under `example.org`.
pub const MEDICINE: &str = "SYN-MEDICINE-0001";

/// The synthetic stay's admission.
pub const ADMITTED: &str = "2026-10-12T09:30:00Z";

/// The synthetic stay's discharge.
pub const DISCHARGED: &str = "2026-10-20T11:00:00Z";

/// Returns the path of the vendored discharge corpus.
#[must_use]
pub fn directory() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/ferrobridge-discharge")
}

/// Returns the path of the vendored ADL 2 template.
#[must_use]
pub fn template_path() -> PathBuf {
    directory().join("ferrobridge_discharge_report.adlt")
}

/// Returns the paths of every archetype the template specialises or holds,
/// in the three directories that hold them.
#[must_use]
pub fn archetype_paths() -> Vec<PathBuf> {
    let mut paths = crate::medication::archetype_paths();
    let medication = PathBuf::from(ROOT).join("docs/specs/ferrobridge-medication");
    paths.extend(
        [
            "openEHR-EHR-COMPOSITION.health_summary.v1.adl",
            "openEHR-EHR-SECTION.adhoc.v1.adl",
            "openEHR-EHR-SECTION.immunisation_list.v0.adl",
        ]
        .iter()
        .map(|file| medication.join(file)),
    );
    paths.extend(BESIDE.iter().map(|file| directory().join(file)));
    paths
}

/// Returns the template's text and each archetype as its file name and
/// text.
///
/// # Errors
///
/// When a vendored file cannot be read.
pub fn template() -> Result<Written, Box<dyn Error>> {
    let mut written = patient_summary_template()?;
    written.template = std::fs::read_to_string(template_path())?;
    for file in BESIDE {
        written.archetypes.push((
            file.to_owned(),
            std::fs::read_to_string(directory().join(file))?,
        ));
    }
    Ok(written)
}

/// An element `at` named `name` holding `value`.
fn element(at: &str, name: &str, value: &str) -> String {
    format!(
        r#"{{ "_type": "ELEMENT", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, "archetype_node_id": "{at}", "value": {value} }}"#
    )
}

/// A coded text of the synthetic code system `system` under `example.org`.
fn synthetic(system: &str, value: &str, code: &str) -> String {
    format!(
        r#"{{ "_type": "DV_CODED_TEXT", "value": "{value}", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "http://example.org/fhir/CodeSystem/{system}" }}, "code_string": "{code}" }} }}"#
    )
}

/// A date and time.
fn instant(value: &str) -> String {
    format!(r#"{{ "_type": "DV_DATE_TIME", "value": "{value}" }}"#)
}

/// The synthetic diagnosis, recognised during the stay.
fn diagnosis() -> String {
    format!(
        r#"{{ "_type": "EVALUATION", "name": {{ "_type": "DV_TEXT", "value": "Problem/Diagnosis" }}, {details}, {ENTRY},
  "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "structure" }}, "archetype_node_id": "at0001", "items": [{name}] }} }}"#,
        details = details("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        name = element(
            "at0002",
            "Problem/Diagnosis name",
            &synthetic("ferrofed-diagnoses", "Synthetic diagnosis", DIAGNOSIS),
        ),
    )
}

/// The synthetic procedure, completed during the stay.
fn procedure() -> String {
    format!(
        r#"{{ "_type": "ACTION", "name": {{ "_type": "DV_TEXT", "value": "Procedure" }}, {details}, {ENTRY},
  "time": {time},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "532" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Procedure completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0043" }} }}
  }},
  "description": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0001", "items": [{name}] }} }}"#,
        details = details("openEHR-EHR-ACTION.procedure.v1"),
        time = instant("2026-10-14T13:00:00Z"),
        name = element(
            "at0002",
            "Procedure name",
            &synthetic("ferrofed-procedures", "Synthetic procedure", PROCEDURE),
        ),
    )
}

/// Returns a synthetic discharge composition as a member serves it.
///
/// It is of [`TEMPLATE`], under the version id `uid`: a stay from
/// [`ADMITTED`] to [`DISCHARGED`] whose diagnostic summary, significant
/// procedures and discharge medications sections each hold one entry.
#[must_use]
pub fn composition(uid: &str) -> String {
    let sections = [
        ("Diagnostic summary", diagnosis()),
        ("Significant procedures", procedure()),
        ("Discharge medications", summary_order(MEDICINE)),
    ];
    let content: Vec<String> = sections
        .iter()
        .map(|(name, item)| {
            format!(
                r#"{{ "_type": "SECTION", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, {section}, "items": [{item}] }}"#,
                section = details("openEHR-EHR-SECTION.adhoc.v1"),
            )
        })
        .collect();
    format!(
        r#"{{
  "_type": "COMPOSITION",
  "name": {{ "_type": "DV_TEXT", "value": "Discharge report" }},
  "archetype_node_id": "openEHR-EHR-COMPOSITION.transfer_summary.v1",
  "archetype_details": {{
    "_type": "ARCHETYPED",
    "archetype_id": {{ "_type": "ARCHETYPE_ID", "value": "openEHR-EHR-COMPOSITION.transfer_summary.v1" }},
    "template_id": {{ "_type": "TEMPLATE_ID", "value": "{TEMPLATE}" }},
    "rm_version": "1.1.0"
  }},
  {held}
  "context": {{
    "_type": "EVENT_CONTEXT",
    "start_time": {start},
    "end_time": {end},
    "setting": {OTHER_CARE}
  }},
  "content": [{content}]
}}"#,
        held = held(uid),
        start = instant(ADMITTED),
        end = instant(DISCHARGED),
        content = content.join(","),
    )
}

/// The context of the shipped HDR project a discharge report is mapped by.
pub const CONTEXT: &str = "ferrobridge_eu_hdr.context";

/// Returns the path of the vendored HL7 Europe Hospital Discharge Report
/// 1.0.0 package.
#[must_use]
pub fn package() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/eu-hl7-hdr/hl7.fhir.eu.hdr-1.0.0.tgz")
}

/// Writes the HDR mapping `eehrxf` ships into `directory`, which must
/// exist, and returns its files in loading order.
///
/// # Errors
///
/// When a file cannot be written.
pub fn mapping_files(directory: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    eehrxf::mappings::HDR.write_to(directory)
}
