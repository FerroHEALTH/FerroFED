// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report fixtures (#1406).
//!
//! They are FerroBRIDGE's ADL 2 imaging report template and the CKM
//! archetypes it specialises or holds, vendored under
//! `docs/specs/ferrobridge-imaging/`, which the imaging project `eehrxf`
//! ships maps over, and a synthetic imaging composition as a member serves
//! it. The composition is written by the component's own mapping from a
//! synthetic `DiagnosticReportEuImaging` and the study it names, so its
//! shape is the one the template gives; its study and report are invented
//! for the tests.

use std::error::Error;
use std::path::PathBuf;

use eehrxf::mapping::Mapping;
use eehrxf::mappings::IMAGING;
use openehr_mapping_core::template::TemplateSource;

use crate::medication::{Written, composed};

/// The workspace root, two levels above this crate's manifest.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

/// The template the shipped imaging project maps.
pub const TEMPLATE: &str = "openEHR-EHR-COMPOSITION.ferrobridge_imaging_report.v1.0.0";

/// The archetypes the template specialises or holds.
const ARCHETYPES: [&str; 2] = [
    "openEHR-EHR-COMPOSITION.report-result.v1.adl",
    "openEHR-EHR-OBSERVATION.imaging_exam_result.v1.adl",
];

/// The conclusion the synthetic report states.
pub const CONCLUSION: &str = "A synthetic impression.";

/// The business identifier of the synthetic report.
pub const REPORT_IDENTIFIER: &str = "synthetic-imaging-report-0001";

/// The system of [`REPORT_IDENTIFIER`].
pub const REPORT_SYSTEM: &str = "http://example.org/fhir/sid/synthetic-imaging-reports";

/// The LOINC code of the synthetic examination, an MR of the knee.
pub const EXAMINATION: &str = "24802-1";

/// When the synthetic report's findings were observed.
pub const OBSERVED: &str = "2026-03-02T08:30:00+01:00";

/// When the synthetic study started.
pub const STARTED: &str = "2026-03-02T08:00:00+01:00";

/// The DICOM modality of the synthetic study.
pub const MODALITY: &str = "MR";

/// The description of the synthetic study.
pub const DESCRIPTION: &str = "Synthetic MR study";

/// The synthetic study's DICOM study instance UID.
pub const STUDY_UID: &str = "urn:oid:2.25.110239581749205781493301578934620193847";

/// Returns the path of the vendored imaging corpus.
#[must_use]
pub fn directory() -> PathBuf {
    PathBuf::from(ROOT).join("docs/specs/ferrobridge-imaging")
}

/// Returns the path of the vendored ADL 2 template.
#[must_use]
pub fn template_path() -> PathBuf {
    directory().join("ferrobridge_imaging_report.adlt")
}

/// Returns the paths of every archetype the template specialises or holds.
#[must_use]
pub fn archetype_paths() -> Vec<PathBuf> {
    ARCHETYPES
        .iter()
        .map(|file| directory().join(file))
        .collect()
}

/// Returns the template's text and each archetype as its file name and
/// text.
///
/// # Errors
///
/// When a vendored file cannot be read.
pub fn template() -> Result<Written, Box<dyn Error>> {
    let mut archetypes = Vec::new();
    for file in ARCHETYPES {
        archetypes.push((
            file.to_owned(),
            std::fs::read_to_string(directory().join(file))?,
        ));
    }
    Ok(Written {
        template: std::fs::read_to_string(template_path())?,
        archetypes,
    })
}

/// A synthetic `collection` of one `DiagnosticReportEuImaging` with each
/// element the shipped imaging mapping carries, the study it names and the
/// department that performed it.
fn collection() -> String {
    serde_json::json!({
        "resourceType": "Bundle",
        "type": "collection",
        "entry": [
            {
                "fullUrl": "http://example.org/fhir/DiagnosticReport/synthetic-report",
                "resource": {
                    "resourceType": "DiagnosticReport",
                    "id": "synthetic-report",
                    "meta": {"profile": ["http://hl7.eu/fhir/imaging/StructureDefinition/DiagnosticReportEuImaging"]},
                    "identifier": [{"system": REPORT_SYSTEM, "value": REPORT_IDENTIFIER}],
                    "status": "final",
                    "code": {"coding": [{"system": "http://loinc.org", "code": EXAMINATION, "display": "MR Knee"}]},
                    "subject": {"reference": "Patient/synthetic-patient"},
                    "effectiveDateTime": OBSERVED,
                    "performer": [{"reference": "Organization/synthetic-radiology"}],
                    "imagingStudy": [{"reference": "ImagingStudy/synthetic-study"}],
                    "conclusion": CONCLUSION
                }
            },
            {
                "fullUrl": "http://example.org/fhir/ImagingStudy/synthetic-study",
                "resource": {
                    "resourceType": "ImagingStudy",
                    "id": "synthetic-study",
                    "meta": {"profile": ["http://hl7.eu/fhir/imaging/StructureDefinition/ImagingStudyEuImaging"]},
                    "identifier": [{
                        "type": {"coding": [{"system": "http://dicom.nema.org/resources/ontology/DCM", "code": "110180", "display": "Study Instance UID"}]},
                        "system": "urn:dicom:uid",
                        "value": STUDY_UID
                    }],
                    "status": "available",
                    "modality": [{"system": "http://dicom.nema.org/resources/ontology/DCM", "code": MODALITY, "display": "Magnetic Resonance"}],
                    "subject": {"reference": "Patient/synthetic-patient"},
                    "started": STARTED,
                    "description": DESCRIPTION
                }
            },
            {
                "fullUrl": "http://example.org/fhir/Organization/synthetic-radiology",
                "resource": {"resourceType": "Organization", "id": "synthetic-radiology", "name": "Synthetic radiology"}
            }
        ]
    })
    .to_string()
}

/// Returns a synthetic imaging composition as a member serves it.
///
/// It is of `source`, the operational template of [`TEMPLATE`], under the
/// version id `uid`, holding the synthetic report and its study.
///
/// # Errors
///
/// When the shipped project does not compile against `source` or refuses
/// the synthetic report.
pub fn composition(source: &TemplateSource, uid: &str) -> Result<String, Box<dyn Error>> {
    composed(
        &Mapping::compile_project(source, &IMAGING)?,
        &collection(),
        uid,
    )
}
