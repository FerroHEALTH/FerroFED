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
//! for the tests. [`received_document`] is the same report as another
//! system sends it, an HL7 Europe Imaging Report 1.0.0-ballot
//! `BundleReportEuImaging` (#1716).

use std::error::Error;
use std::path::PathBuf;

use eehrxf::document::Document;
use eehrxf::document::imaging::{IMAGING_REPORT_FORM, IMAGING_SLOTS};
use eehrxf::document::section::{EmptyReason, Section};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::IMAGING;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use openehr_mapping_core::template::TemplateSource;

use crate::medication::{Written, composed};
use crate::sender::{BASE, device, displayed, named, organization};

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

/// The context of the shipped imaging project an imaging report is mapped
/// by.
pub const CONTEXT: &str = "ferrobridge_eu_imaging.context";

/// Writes the imaging mapping `eehrxf` ships into `directory`, which must
/// exist, and returns its files in loading order.
///
/// # Errors
///
/// When a file cannot be written.
pub fn mapping_files(directory: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    IMAGING.write_to(directory)
}

/// The instant the synthetic sender writes its report at.
const SENT_AT: &str = "2026-10-21T08:00:00Z";

/// The section slice of `CompositionEuImaging` that lists the study.
const STUDY: &str = "imagingstudy";

/// How many of `CompositionEuImaging`'s sections the profile requires, the
/// first of the form's slots.
const REQUIRED: usize = 4;

/// The displays the sender writes for the codes the component writes with
/// none, each `(system, code, display)`.
const DISPLAYS: [(&str, &str, &str); 1] = [("http://loinc.org", EXAMINATION, "MR Knee")];

/// A synthetic imaging report of the patient `identifiers`, as another
/// system sends it.
///
/// Each identifier is a `(system, value)`. The document is in the HL7
/// Europe Imaging Report 1.0.0-ballot form, written by the shipped imaging
/// project from the synthetic composition over `source`, the operational
/// template of [`TEMPLATE`], with a named `Patient`, the sender's `Device`
/// as its author, owned by the sender's `Organization`, which is its
/// custodian, the study listed in its section and each other section the
/// profile requires empty as `unavailable`.
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
        &IMAGING_REPORT_FORM,
        BASE,
        String::from("urn:uuid:5b0c3f7e-0000-4000-8000-00000000e1c1"),
        String::from(SENT_AT),
        String::from("Imaging report"),
    );
    let subject = named(&mut document, identifiers)?;
    let hospital = document.full_url("Organization", "hospital");
    let device = document.with_author(&Resource::Device(Box::new(device(
        "Synthetic radiology information system",
        Some(&hospital),
    ))))?;
    // NOTE: HL7 Europe Imaging Report 1.0.0-ballot dr-comp-author-org drops an Organization author for
    // the report's performers, so the sender names its organization the custodian (FHIR R4 Composition).
    document.with_custodian(organization("hospital", "Synthetic hospital"))?;
    let reference = |url: &str| Reference {
        reference: Some(url.into()),
        ..Reference::default()
    };
    let run = Mapping::compile_project(source, &IMAGING)?.to_fhir_with(
        &composition(
            source,
            "5b0c3f7e-0000-4000-8000-00000000e1c2::hospital.example.org::1",
        )?,
        &Settings::new(device.clone(), SENT_AT),
        CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device)),
    )?;
    let study = run
        .bundle()
        .entry
        .iter()
        .find_map(|entry| match entry.resource.as_ref() {
            Some(Resource::ImagingStudy(study)) => study.id.clone(),
            _ => None,
        })
        .map(|id| document.full_url("ImagingStudy", &id))
        .ok_or("the run writes the study")?;
    document.with_mapped_report(run.bundle())?;
    for slot in IMAGING_SLOTS.iter().take(REQUIRED) {
        let section = Section::new(slot, vec![format!("{}.", slot.title)]);
        // NOTE: HL7 Europe Imaging Report 1.0.0-ballot eu-imaging-composition-1: a section holds an
        // entry or an emptyReason, and the sender records no order, history or procedure.
        let section = if slot.slice == STUDY {
            section.with_entry(study.clone())
        } else {
            section.empty_because(EmptyReason::Unavailable)
        };
        document.with_section(section)?;
    }
    let mut bundle = serde_json::to_value(document.into_bundle()?)?;
    displayed(&mut bundle, &DISPLAYS);
    Ok(bundle.to_string())
}
