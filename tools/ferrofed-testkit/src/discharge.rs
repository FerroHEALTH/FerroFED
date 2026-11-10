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

use eehrxf::document::Document;
use eehrxf::document::discharge::{DISCHARGE_REPORT_FORM, HDR_SLOTS};
use eehrxf::document::section::Section;
use eehrxf::mapping::Mapping;
use eehrxf::mappings::HDR;
use fhir_types::r4::reference::Reference;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use openehr_mapping_core::template::TemplateSource;

use crate::medication::{
    ENTRY, OTHER_CARE, Written, details, held, order, patient_summary_template, summary_order,
};
use crate::sender::{BASE, authored, displayed};

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

/// A problem or diagnosis holding `items`.
fn evaluation(items: &[String]) -> String {
    format!(
        r#"{{ "_type": "EVALUATION", "name": {{ "_type": "DV_TEXT", "value": "Problem/Diagnosis" }}, {details}, {ENTRY},
  "data": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "structure" }}, "archetype_node_id": "at0001", "items": [{items}] }} }}"#,
        details = details("openEHR-EHR-EVALUATION.problem_diagnosis.v1"),
        items = items.join(","),
    )
}

/// A procedure completed at `time`, holding `items`.
fn completed(time: &str, items: &[String]) -> String {
    format!(
        r#"{{ "_type": "ACTION", "name": {{ "_type": "DV_TEXT", "value": "Procedure" }}, {details}, {ENTRY},
  "time": {time},
  "ism_transition": {{
    "_type": "ISM_TRANSITION",
    "current_state": {{ "_type": "DV_CODED_TEXT", "value": "completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "openehr" }}, "code_string": "532" }} }},
    "careflow_step": {{ "_type": "DV_CODED_TEXT", "value": "Procedure completed", "defining_code": {{ "_type": "CODE_PHRASE", "terminology_id": {{ "_type": "TERMINOLOGY_ID", "value": "local" }}, "code_string": "at0043" }} }}
  }},
  "description": {{ "_type": "ITEM_TREE", "name": {{ "_type": "DV_TEXT", "value": "Tree" }}, "archetype_node_id": "at0001", "items": [{items}] }} }}"#,
        details = details("openEHR-EHR-ACTION.procedure.v1"),
        time = instant(time),
        items = items.join(","),
    )
}

/// The synthetic diagnosis, recognised during the stay.
fn diagnosis() -> String {
    evaluation(&[element(
        "at0002",
        "Problem/Diagnosis name",
        &synthetic("ferrofed-diagnoses", "Synthetic diagnosis", DIAGNOSIS),
    )])
}

/// The synthetic procedure, completed during the stay.
fn procedure() -> String {
    completed(
        "2026-10-14T13:00:00Z",
        &[element(
            "at0002",
            "Procedure name",
            &synthetic("ferrofed-procedures", "Synthetic procedure", PROCEDURE),
        )],
    )
}

/// Returns a synthetic discharge composition as a member serves it.
///
/// It is of [`TEMPLATE`], under the version id `uid`: a stay from
/// [`ADMITTED`] to [`DISCHARGED`] whose diagnostic summary, significant
/// procedures and discharge medications sections each hold one entry.
#[must_use]
pub fn composition(uid: &str) -> String {
    composed(
        uid,
        &[
            ("Diagnostic summary", vec![diagnosis()]),
            ("Significant procedures", vec![procedure()]),
            ("Discharge medications", vec![summary_order(MEDICINE)]),
        ],
    )
}

/// A problem or diagnosis of the full synthetic discharge composition, each
/// element the shipped HDR mapping reads.
#[derive(Debug, Clone, Copy)]
pub struct Diagnosis {
    /// The code of the diagnosis, in a code system under `example.org`.
    pub code: &'static str,
    /// When it began.
    pub onset: &'static str,
    /// When it was clinically recognised.
    pub recognised: &'static str,
    /// The body site, as text.
    pub body_site: &'static str,
    /// The comment.
    pub comment: &'static str,
}

/// A procedure of the full synthetic discharge composition, each element
/// the shipped HDR mapping reads.
#[derive(Debug, Clone, Copy)]
pub struct Procedure {
    /// The code of the procedure, in a code system under `example.org`.
    pub code: &'static str,
    /// When it was completed.
    pub performed: &'static str,
    /// The body site, as text.
    pub body_site: &'static str,
    /// The comment.
    pub comment: &'static str,
}

/// A discharge medicine of the full synthetic discharge composition, each
/// element the shipped HDR mapping reads.
#[derive(Debug, Clone, Copy)]
pub struct Medicine {
    /// The code of the medicine, in a code system under `example.org`.
    pub code: &'static str,
    /// The overall directions.
    pub directions: &'static str,
    /// The comment.
    pub comment: &'static str,
}

/// The two problems or diagnoses of the full synthetic discharge
/// composition.
pub const DIAGNOSES: [Diagnosis; 2] = [
    Diagnosis {
        code: DIAGNOSIS,
        onset: "2026-10-11T18:00:00Z",
        recognised: "2026-10-12T10:15:00Z",
        body_site: "Synthetic body site one",
        comment: "Synthetic diagnosis comment one",
    },
    Diagnosis {
        code: "SYN-DIAGNOSIS-0002",
        onset: "2026-10-13T07:00:00Z",
        recognised: "2026-10-13T09:40:00Z",
        body_site: "Synthetic body site two",
        comment: "Synthetic diagnosis comment two",
    },
];

/// The two procedures of the full synthetic discharge composition.
pub const PROCEDURES: [Procedure; 2] = [
    Procedure {
        code: PROCEDURE,
        performed: "2026-10-14T13:00:00Z",
        body_site: "Synthetic procedure site one",
        comment: "Synthetic procedure comment one",
    },
    Procedure {
        code: "SYN-PROCEDURE-0002",
        performed: "2026-10-16T08:30:00Z",
        body_site: "Synthetic procedure site two",
        comment: "Synthetic procedure comment two",
    },
];

/// The two discharge medicines of the full synthetic discharge composition.
pub const MEDICINES: [Medicine; 2] = [
    Medicine {
        code: MEDICINE,
        directions: "One synthetic tablet each morning",
        comment: "Synthetic medicine comment one",
    },
    Medicine {
        code: "SYN-MEDICINE-0002",
        directions: "Two synthetic tablets each evening",
        comment: "Synthetic medicine comment two",
    },
];

/// A plain text.
fn text(value: &str) -> String {
    format!(r#"{{ "_type": "DV_TEXT", "value": "{value}" }}"#)
}

/// A problem or diagnosis with each element the HDR mapping reads.
fn full_diagnosis(diagnosis: &Diagnosis) -> String {
    let items = [
        element(
            "at0002",
            "Problem/Diagnosis name",
            &synthetic("ferrofed-diagnoses", "Synthetic diagnosis", diagnosis.code),
        ),
        element("at0012", "Body site", &text(diagnosis.body_site)),
        element("at0077", "Date/time of onset", &instant(diagnosis.onset)),
        element(
            "at0003",
            "Date/time clinically recognised",
            &instant(diagnosis.recognised),
        ),
        element("at0069", "Comment", &text(diagnosis.comment)),
    ];
    evaluation(&items)
}

/// A completed procedure with each element the HDR mapping reads.
fn full_procedure(procedure: &Procedure) -> String {
    let items = [
        element(
            "at0002",
            "Procedure name",
            &synthetic("ferrofed-procedures", "Synthetic procedure", procedure.code),
        ),
        element("at0063", "Body site", &text(procedure.body_site)),
        element("at0005", "Comment", &text(procedure.comment)),
    ];
    completed(procedure.performed, &items)
}

/// A medication order with each element the HDR mapping reads.
fn full_order(medicine: &Medicine) -> String {
    let items = [
        element(
            "at0070",
            "Medication item",
            &synthetic("ferrofed-products", "Synthetic tablet", medicine.code),
        ),
        element(
            "at0009",
            "Overall directions description",
            &text(medicine.directions),
        ),
        element("at0167", "Comment", &text(medicine.comment)),
    ];
    order(&items)
}

/// A discharge composition of the sections `sections`, each a name and its
/// entries.
fn composed(uid: &str, sections: &[(&str, Vec<String>)]) -> String {
    let content: Vec<String> = sections
        .iter()
        .map(|(name, items)| {
            format!(
                r#"{{ "_type": "SECTION", "name": {{ "_type": "DV_TEXT", "value": "{name}" }}, {section}, "items": [{items}] }}"#,
                section = details("openEHR-EHR-SECTION.adhoc.v1"),
                items = items.join(","),
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

/// Returns the full synthetic discharge composition as a member serves it.
///
/// It is of [`TEMPLATE`], under the version id `uid`: a stay from
/// [`ADMITTED`] to [`DISCHARGED`] whose three sections, the only ones the
/// template holds, hold two entries each, [`DIAGNOSES`], [`PROCEDURES`] and
/// [`MEDICINES`], with every element the shipped HDR mapping reads.
#[must_use]
pub fn full_composition(uid: &str) -> String {
    composed(
        uid,
        &[
            (
                "Diagnostic summary",
                DIAGNOSES.iter().map(full_diagnosis).collect(),
            ),
            (
                "Significant procedures",
                PROCEDURES.iter().map(full_procedure).collect(),
            ),
            (
                "Discharge medications",
                MEDICINES.iter().map(full_order).collect(),
            ),
        ],
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
    HDR.write_to(directory)
}

/// The hospital course the synthetic sender writes as its section's one
/// paragraph.
pub const HOSPITAL_COURSE: &str = "A synthetic hospital course.";

/// The instant the synthetic sender writes its report at.
const SENT_AT: &str = "2026-10-21T08:00:00Z";

/// The synthetic diagnoses' code system.
const DIAGNOSES_SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrofed-diagnoses";

/// The synthetic procedures' code system.
const PROCEDURES_SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrofed-procedures";

/// The synthetic products' code system.
const PRODUCTS_SYSTEM: &str = "http://example.org/fhir/CodeSystem/ferrofed-products";

/// The displays a sender that names its codes writes for the codes the
/// component writes with none, each `(system, code, display)`.
const DISPLAYS: [(&str, &str, &str); 11] = [
    (DIAGNOSES_SYSTEM, DIAGNOSIS, "Synthetic diagnosis"),
    (
        DIAGNOSES_SYSTEM,
        "SYN-DIAGNOSIS-0002",
        "Synthetic diagnosis",
    ),
    (PROCEDURES_SYSTEM, PROCEDURE, "Synthetic procedure"),
    (
        PROCEDURES_SYSTEM,
        "SYN-PROCEDURE-0002",
        "Synthetic procedure",
    ),
    (PRODUCTS_SYSTEM, MEDICINE, "Synthetic tablet"),
    (PRODUCTS_SYSTEM, "SYN-MEDICINE-0002", "Synthetic tablet"),
    ("http://loinc.org", "34105-7", "Hospital Discharge summary"),
    ("http://loinc.org", "8648-8", "Hospital course Narrative"),
    (
        "http://loinc.org",
        "11535-2",
        "Hospital discharge Dx Narrative",
    ),
    (
        "http://loinc.org",
        "10185-7",
        "Hospital discharge procedures Narrative",
    ),
    (
        "http://loinc.org",
        "75311-1",
        "Discharge medications Narrative",
    ),
];

/// A reference to `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// A synthetic discharge report of the patient `identifiers`, as another
/// system sends it.
///
/// Each identifier is a `(system, value)`. The document is in the HL7
/// Europe Hospital Discharge Report 1.0.0 form, written by the shipped HDR
/// project from [`full_composition`] over `source`, the operational
/// template of [`TEMPLATE`], with a named `Patient`, the sender's `Device`
/// and `Organization` as its authors, and a hospital course.
///
/// # Errors
///
/// When the shipped project does not compile against `source`, refuses the
/// synthetic composition, or the component's builder refuses a resource.
pub fn received_document(
    source: &TemplateSource,
    identifiers: &[(&str, &str)],
) -> Result<String, Box<dyn Error>> {
    let mut document = Document::of(
        &DISCHARGE_REPORT_FORM,
        BASE,
        String::from("urn:uuid:5b0c3f7e-0000-4000-8000-00000000d1c1"),
        String::from(SENT_AT),
        String::from("Discharge report"),
    );
    let (subject, device) = authored(
        &mut document,
        identifiers,
        (
            "Synthetic hospital system",
            ("hospital", "Synthetic hospital"),
        ),
    )?;
    document.with_language(String::from("en"));
    let run = Mapping::compile_project(source, &HDR)?.to_fhir_with(
        &full_composition("5b0c3f7e-0000-4000-8000-00000000d1c2::hospital.example.org::1"),
        &Settings::new(device.clone(), SENT_AT),
        CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device)),
    )?;
    // NOTE: HL7 Europe HDR 1.0.0 composition-eu-hdr cmp-hdr-2 requires the discharge summary or
    // the hospital course, which the sending system writes.
    let course = HDR_SLOTS
        .iter()
        .find(|slot| slot.slice == "sectionHospitalCourse")
        .ok_or("the hospital course slot")?;
    document.with_section(Section::new(course, vec![String::from(HOSPITAL_COURSE)]))?;
    for section in document.with_mapped_composition(run.bundle())? {
        let paragraph = format!("{}.", section.slot.title);
        document.with_section(section.into_section(vec![paragraph]))?;
    }
    let mut bundle = serde_json::to_value(document.into_bundle()?)?;
    displayed(&mut bundle, &DISPLAYS);
    Ok(bundle.to_string())
}
