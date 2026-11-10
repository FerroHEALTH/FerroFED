// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report fixtures (#1555).
//!
//! They are the vendored HL7 Europe Laboratory Report 2.0.0 package, the
//! `KDS_Laborbericht` operational template the mapping `eehrxf` ships
//! compiles against, that mapping written where a deployment keeps its
//! files, and a synthetic laboratory report document as a sender writes it.
//!
//! The document is assembled with `eehrxf`'s own document builder in the
//! `Bundle-eu-lab` form, so its composition, its report and the links
//! between them are the ones the component writes. Its patient, specimen,
//! result and report are invented for the tests.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the synthetic resources are written as JSON"
)]

use std::error::Error;
use std::path::{Path, PathBuf};

use eehrxf::document::laboratory::{LAB_SLOTS, LABORATORY_REPORT_FORM};
use eehrxf::document::section::Section;
use eehrxf::mappings::LABORATORY;
use fhir_types::r4::resource::Resource;
use serde_json::Value;

use crate::sender::{authored, displayed, resource, sent};

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The template the shipped laboratory mapping maps.
pub const TEMPLATE: &str = "KDS_Laborbericht";

/// The LOINC code of the analyte the synthetic result records.
pub const ANALYTE: &str = "2093-3";

/// The conclusion the synthetic report states.
pub const CONCLUSION: &str = "A synthetic conclusion.";

/// The business identifier of the synthetic report.
pub const REPORT_IDENTIFIER: &str = "synthetic-report-0001";

/// The order filler's identifier of the order the synthetic report answers.
pub const ORDER_IDENTIFIER: &str = "synthetic-order-0001";

/// The system of [`ORDER_IDENTIFIER`].
pub const ORDER_SYSTEM: &str = "http://example.org/fhir/sid/synthetic-lab-orders";

/// The lower and upper bound, in mg/dL, of the synthetic result's normal
/// range.
pub const NORMAL_RANGE: (f64, f64) = (120.0, 200.0);

/// The R4 code system of `Observation.referenceRange.type`.
pub const REFERENCE_RANGE_MEANING: &str =
    "http://terminology.hl7.org/CodeSystem/referencerange-meaning";

/// A reference range of the synthetic result other than its normal range,
/// typed by its code in [`REFERENCE_RANGE_MEANING`].
#[derive(Debug, Clone, Copy)]
pub struct TypedRange<'a> {
    /// The `referencerange-meaning` code, such as `treatment`.
    pub meaning: &'a str,
    /// The lower bound in mg/dL, when the range has one.
    pub low: Option<f64>,
    /// The upper bound in mg/dL, when the range has one.
    pub high: Option<f64>,
}

/// The canonical URL of the laboratory result profile.
const RESULT_PROFILE: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab";

/// The canonical URL of the laboratory report profile.
const REPORT_PROFILE: &str =
    "http://hl7.eu/fhir/laboratory/StructureDefinition/DiagnosticReport-eu-lab";

/// The canonical URL of the laboratory specimen profile.
const SPECIMEN_PROFILE: &str = "http://hl7.eu/fhir/laboratory/StructureDefinition/Specimen-eu-lab";

/// Returns the path of the vendored HL7 Europe Laboratory Report 2.0.0
/// package.
#[must_use]
pub fn package() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/eu-hl7-laboratory/hl7.fhir.eu.laboratory-2.0.0.tgz")
}

/// Returns the path of the vendored `KDS_Laborbericht` operational
/// template.
#[must_use]
pub fn opt() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/kds-laborbericht/KDS_Laborbericht.opt")
}

/// Writes the laboratory mapping `eehrxf` ships into `directory`, which
/// must exist, and returns its files in loading order.
///
/// # Errors
///
/// The I/O error of the first file that cannot be written.
pub fn mapping_files(directory: &Path) -> std::io::Result<Vec<PathBuf>> {
    LABORATORY.write_to(directory)
}

/// The context of the shipped laboratory project a laboratory report is
/// mapped by, as the gateway names it (`ferrofed_eehrxf::laboratory`).
pub const RESULT_CONTEXT: &str = "ferrobridge_eu_laboratory.context";

/// Returns the context mapping a received laboratory report is mapped by,
/// the project narrowed to [`RESULT_CONTEXT`].
///
/// # Errors
///
/// When the shipped project names no such context.
pub fn context() -> Result<&'static str, Box<dyn Error>> {
    let result = LABORATORY.with_context(RESULT_CONTEXT)?;
    result
        .contexts()
        .first()
        .copied()
        .ok_or_else(|| "the laboratory project names no result context".into())
}

/// A synthetic laboratory report of the patient `identifiers`.
///
/// Each identifier is a `(system, value)`. The document is in the HL7
/// Europe Laboratory Report 2.0.0 form: one serum cholesterol result of
/// 182 mg/dL with its normal range, each of `typed` after it, and its
/// specimen, and the report that names both and the order it answers.
///
/// # Errors
///
/// When the component's builder refuses a synthetic resource.
pub fn received_document(
    identifiers: &[(&str, &str)],
    typed: &[TypedRange<'_>],
) -> Result<String, Box<dyn Error>> {
    let mut document = sent(
        &LABORATORY_REPORT_FORM,
        "urn:uuid:5b0c3f7e-0000-4000-8000-00000000f001",
        "2026-03-02T10:00:00Z",
        "Laboratory report",
    );
    authored(
        &mut document,
        identifiers,
        (
            "Synthetic laboratory system",
            ("laboratory", "Synthetic laboratory"),
        ),
    )?;
    let urls = document.add(&[
        resource("Observation", &result(typed))?,
        resource("Specimen", &specimen())?,
    ])?;
    let slot = LAB_SLOTS
        .iter()
        .find(|slot| slot.code == "18719-5")
        .ok_or("the chemistry slot")?;
    let mut section = Section::new(slot, vec![String::from("Lipid panel.")]);
    if let Some(result) = urls.first() {
        section = section.with_entry(result.clone());
    }
    document.with_section(section)?;
    let Resource::DiagnosticReport(report) = resource("DiagnosticReport", &report())? else {
        return Err("the synthetic report is no DiagnosticReport".into());
    };
    document.with_report(*report)?;
    let mut bundle = serde_json::to_value(document.into_bundle()?)?;
    displayed(&mut bundle, &DISPLAYS);
    Ok(bundle.to_string())
}

/// The displays a sender that names its codes writes for the codes the
/// component writes with none, each `(system, code, display)`.
const DISPLAYS: [(&str, &str, &str); 4] = [
    ("http://loinc.org", "11502-2", "Laboratory report"),
    ("http://loinc.org", "18719-5", "Chemistry studies"),
    (
        "http://terminology.hl7.org/CodeSystem/observation-category",
        "laboratory",
        "Laboratory",
    ),
    (REFERENCE_RANGE_MEANING, "treatment", "Treatment Range"),
];

/// A synthetic serum specimen in the shape `Specimen-eu-lab` constrains.
fn specimen() -> Value {
    serde_json::json!({
        "resourceType": "Specimen",
        "id": "synthetic-serum",
        "meta": { "profile": [SPECIMEN_PROFILE] },
        "accessionIdentifier": {
            "system": "http://example.org/fhir/sid/synthetic-specimens",
            "value": "synthetic-specimen-0001"
        },
        "type": { "coding": [{
            "system": "http://snomed.info/sct",
            "code": "119364003",
            "display": "Serum specimen"
        }] },
        "subject": { "reference": "Patient/synthetic-patient" },
        "receivedTime": "2026-03-02T07:40:00+01:00",
        "collection": { "collectedDateTime": "2026-03-02T07:05:00+01:00" }
    })
}

/// A synthetic quantitative result in the shape
/// `Observation-resultslab-eu-lab` constrains, with its normal range and
/// each of `typed`.
fn result(typed: &[TypedRange<'_>]) -> Value {
    let mut ranges = vec![serde_json::json!({
        "low": milligrams(NORMAL_RANGE.0),
        "high": milligrams(NORMAL_RANGE.1)
    })];
    for range in typed {
        let mut held = serde_json::Map::new();
        held.insert(
            String::from("type"),
            serde_json::json!({ "coding": [{
                "system": REFERENCE_RANGE_MEANING,
                "code": range.meaning
            }] }),
        );
        for (bound, value) in [("low", range.low), ("high", range.high)] {
            if let Some(value) = value {
                held.insert(String::from(bound), milligrams(value));
            }
        }
        ranges.push(Value::Object(held));
    }
    serde_json::json!({
        "resourceType": "Observation",
        "id": "synthetic-cholesterol",
        "meta": { "profile": [RESULT_PROFILE] },
        "status": "final",
        "category": [{ "coding": [{
            "system": "http://terminology.hl7.org/CodeSystem/observation-category",
            "code": "laboratory"
        }] }],
        "code": { "coding": [{
            "system": "http://loinc.org",
            "code": ANALYTE,
            "display": "Cholesterol [Mass/volume] in Serum or Plasma"
        }] },
        "subject": { "reference": "Patient/synthetic-patient" },
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "performer": [{ "reference": "Organization/laboratory" }],
        "valueQuantity": {
            "value": 182,
            "unit": "mg/dL",
            "system": "http://unitsofmeasure.org",
            "code": "mg/dL"
        },
        "referenceRange": ranges
    })
}

/// A UCUM quantity of `value` mg/dL.
fn milligrams(value: f64) -> Value {
    serde_json::json!({
        "value": value,
        "unit": "mg/dL",
        "system": "http://unitsofmeasure.org",
        "code": "mg/dL"
    })
}

/// A synthetic report in the shape `DiagnosticReport-eu-lab` constrains,
/// naming the result, the specimen and, by its filler's identifier, the
/// order it answers.
fn report() -> Value {
    serde_json::json!({
        "resourceType": "DiagnosticReport",
        "id": "synthetic-report",
        "meta": { "profile": [REPORT_PROFILE] },
        "identifier": [{
            "system": "http://example.org/fhir/sid/synthetic-lab-reports",
            "value": REPORT_IDENTIFIER
        }],
        "basedOn": [{
            "type": "ServiceRequest",
            "identifier": { "system": ORDER_SYSTEM, "value": ORDER_IDENTIFIER }
        }],
        "status": "final",
        "code": { "coding": [{
            "system": "http://loinc.org",
            "code": "24331-1",
            "display": "Lipid panel - Serum or Plasma"
        }] },
        "subject": { "reference": "Patient/synthetic-patient" },
        "effectiveDateTime": "2026-03-02T08:15:00+01:00",
        "specimen": [{ "reference": "Specimen/synthetic-serum" }],
        "result": [{ "reference": "Observation/synthetic-cholesterol" }],
        "conclusion": CONCLUSION
    })
}

/// Returns the synthetic report of the patient `identifiers`, with the
/// further ranges `typed`, as a member serves it.
///
/// The report is written into one `KDS_Laborbericht` composition by the
/// shipped mapping, its files written under `directory`, which must exist,
/// and committed under the version id `uid`.
///
/// # Errors
///
/// When the mapping does not compile or refuses the synthetic report.
pub fn composition(
    directory: &Path,
    identifiers: &[(&str, &str)],
    (uid, typed): (&str, &[TypedRange<'_>]),
) -> Result<String, Box<dyn Error>> {
    let files = directory.join("composition-mapping");
    std::fs::create_dir_all(&files)?;
    let mapping = eehrxf::mapping::Mapping::compile(
        &std::fs::read_to_string(opt())?,
        &mapping_files(&files)?,
        &[context()?],
    )?;
    let document =
        eehrxf::receive::ReceivedDocument::read(&received_document(identifiers, typed)?)?;
    let now = "2026-03-02T10:00:00Z";
    let settings = fhirconnect::operations::run::Settings::new("Device/ferrofed-testkit", now)
        .with_defaults(
            fhirconnect::engine::traverse::defaults::Defaults::at(now)
                .with_language("en")
                .with_territory("NL"),
        );
    let composed = mapping.to_openehr(&document, &settings)?;
    let mut canonical: Value = serde_json::from_str(composed.composition())?;
    canonical
        .as_object_mut()
        .ok_or("the composition is no object")?
        .insert(
            String::from("uid"),
            serde_json::json!({"_type": "OBJECT_VERSION_ID", "value": uid}),
        );
    Ok(canonical.to_string())
}
