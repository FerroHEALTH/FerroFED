// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored query of the imaging report, and the report the gateway writes
//! from one member's imaging composition.
//!
//! An imaging report is one document per report (Regulation (EU) 2025/327
//! Art 14(1)(d)): the HL7 Europe Imaging Report's `CompositionEuImaging`
//! heads it and its `DiagnosticReportEuImaging` carries the body. The
//! openEHR side is one composition per report, so the category has one
//! stored query, held read-only under the reserved namespace at [`VERSION`]
//! (§12.7, N44), in the shape of [`section_query`]: every whole composition
//! that contains [`ARCHETYPE`], with its uid and its template id. The
//! archetype is the entry FerroBRIDGE's imaging report template holds
//! (vendored under `docs/specs/ferrobridge-imaging/`); its root,
//! `openEHR-EHR-COMPOSITION.report-result.v1`, roots laboratory reports too,
//! so the query names the entry alone.
//!
//! [`Served`] writes one such composition as an HL7 Europe Imaging Report
//! 1.0.0-ballot `BundleReportEuImaging` document (`IMAGING_REPORT_FORM`),
//! through the shipped imaging mapping (`eehrxf::mappings::IMAGING`)
//! compiled against the deployment's template, for the document list of the
//! FHIR face (Regulation (EU) 2025/327 Annex II 2.1).

use eehrxf::document::Document;
use eehrxf::document::imaging::{IMAGING_REPORT_FORM, IMAGING_SLOTS};
use eehrxf::document::section::{EmptyReason, Section};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::IMAGING;
use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::resource::Resource;
use openehr_query::printer;

use crate::prescriptions::mapped::Template;
use crate::report::{self, Report, ServedError, Writes};
use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};

/// The query's name in the reserved namespace.
pub const SLUG: &str = "imaging-report";

/// The Xt-EHR element path of the report body the query feeds, the row of
/// the imaging report crosswalk.
pub const ELEMENT: &str = "EHDSImagingReport.body";

/// The entry archetype an imaging report composition contains.
pub const ARCHETYPE: Archetype = Archetype {
    class: "OBSERVATION",
    id: "openEHR-EHR-OBSERVATION.imaging_exam_result.v1",
};

/// The section slice of `CompositionEuImaging` that lists the study.
const STUDY: &str = "imagingstudy";

/// How many of `CompositionEuImaging`'s sections the profile requires, the
/// first of the form's slots.
const REQUIRED: usize = 4;

/// Returns the query's qualified name, `eu.ferrofed.eehrxf::imaging-report`.
///
/// # Errors
///
/// The [`QueryNameError`] of [`QueryName::new`], which the name does not
/// meet.
pub fn name() -> Result<QueryName, QueryNameError> {
    QueryName::new(&format!("{NAMESPACE}::{SLUG}"))
}

/// Returns the query as AQL text, [`section_query::containing`] of
/// [`ARCHETYPE`] printed from its syntax tree.
#[must_use]
pub fn aql() -> String {
    printer::to_aql(&section_query::containing(&ARCHETYPE), None)
}

/// Returns the query as the registry holds it: under its reserved name, at
/// [`VERSION`], saved at [`SAVED`].
///
/// # Errors
///
/// The [`QueryNameError`] of [`name`].
pub fn definition() -> Result<StoredDefinition, QueryNameError> {
    Ok(StoredDefinition::new(name()?, VERSION, aql(), SAVED))
}

/// The shipped imaging mapping compiled against a deployment's imaging
/// report template, which writes a member's composition as an HL7 Europe
/// Imaging Report 1.0.0-ballot `BundleReportEuImaging` document.
#[derive(Debug)]
pub struct Served {
    mapping: Mapping,
}

impl Served {
    /// Compiles the shipped imaging mapping (`eehrxf::mappings::IMAGING`)
    /// against `template`.
    ///
    /// # Errors
    ///
    /// [`ServedError::Template`] when the template does not read and
    /// [`ServedError::Compile`] when the mapping does not compile against
    /// it.
    pub fn compile(template: Template<'_>) -> Result<Self, ServedError> {
        Ok(Self {
            mapping: report::compiled(template, &IMAGING)?,
        })
    }
}

impl Writes for Served {
    /// Writes `composition`, an imaging composition in canonical JSON, as
    /// the `BundleReportEuImaging` document `report` names.
    ///
    /// The document's `Composition` is authored by the gateway's `Device`
    /// and the operator's `Organization`, its subject the `Patient` the
    /// identifier names, and its report the `DiagnosticReportEuImaging` the
    /// mapping writes. The imaging study section lists the study the
    /// mapping writes, and each other section `CompositionEuImaging`
    /// requires is empty as `unavailable`, which `eu-imaging-composition-1`
    /// asks of an empty section. One `Provenance` names the member as the
    /// source of the study.
    ///
    /// # Errors
    ///
    /// [`ServedError::Run`] when the composition does not map, and
    /// [`ServedError::Document`] when the document cannot be assembled.
    fn document(&self, composition: &str, report: Report<'_>) -> Result<Bundle, ServedError> {
        let mut document = Document::of(
            &IMAGING_REPORT_FORM,
            report.base,
            report.identifier.to_owned(),
            report.timestamp.to_owned(),
            String::from("Imaging report"),
        );
        let mapped = report::authored(
            &mut document,
            report,
            "Software that writes the imaging report from the member's record automatically",
            (&self.mapping, composition),
        )?;
        let study = mapped
            .bundle()
            .entry
            .iter()
            .find_map(|entry| match entry.resource.as_ref() {
                Some(Resource::ImagingStudy(study)) => study.id.clone(),
                _ => None,
            })
            .map(|id| document.full_url("ImagingStudy", &id));
        let mut targets = document
            .with_mapped_report(mapped.bundle())
            .map_err(ServedError::Document)?;
        targets.extend(study.iter().cloned());
        report::sourced(&mut document, report, &targets).map_err(ServedError::Document)?;
        for slot in IMAGING_SLOTS.iter().take(REQUIRED) {
            let section = Section::new(
                slot,
                vec![format!(
                    "The {} as the member records it, written by the gateway.",
                    slot.title.to_lowercase()
                )],
            );
            // NOTE: HL7 Europe Imaging Report 1.0.0-ballot eu-imaging-composition-1: a section holds an
            // entry or an emptyReason, and the member's record holds no order, history or procedure.
            let section = match (&study, slot.slice == STUDY) {
                (Some(study), true) => section.with_entry(study.clone()),
                _ => section.empty_because(EmptyReason::Unavailable),
            };
            document
                .with_section(section)
                .map_err(ServedError::Document)?;
        }
        document.into_bundle().map_err(ServedError::Document)
    }
}
