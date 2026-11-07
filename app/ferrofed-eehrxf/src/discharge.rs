// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored query of the discharge report, and the report the gateway
//! writes from one member's discharge composition.
//!
//! A discharge report is one document per hospital stay (Regulation (EU)
//! 2025/327 Art 14(1)(f)): the HL7 Europe Hospital Discharge Report's
//! `composition-eu-hdr` heads it. The openEHR side is one composition per
//! report, so the category has one stored query, held read-only under the
//! reserved namespace at [`VERSION`] (§12.7, N44), in the shape of
//! [`section_query`]: every whole composition of [`COMPOSITION`], with its
//! uid and its template id. The archetype is the root FerroBRIDGE's
//! discharge report template specialises (vendored under
//! `docs/specs/ferrobridge-discharge/`); the query names the composition
//! archetype, since the template's entries, a problem or diagnosis, a
//! procedure and a medication order, are those every other record holds too.
//!
//! [`Served`] writes one such composition as an HL7 Europe Hospital
//! Discharge Report 1.0.0 `bundle-eu-hdr` document
//! (`DISCHARGE_REPORT_FORM`), through the shipped discharge mapping
//! (`eehrxf::mappings::HDR`) compiled against the deployment's template, for
//! the document list of the FHIR face (Regulation (EU) 2025/327 Annex II
//! 2.1).

use eehrxf::document::discharge::{DISCHARGE_REPORT_FORM, HDR_SLOTS};
use eehrxf::document::section::Section;
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::HDR;
use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use fhir_types::r4::bundle::Bundle;
use openehr_query::printer;
use serde::Deserialize;

use crate::prescriptions::mapped::Template;
use crate::report::{self, Report, ServedError};
use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query;
use crate::stack::deep;

/// The query's name in the reserved namespace.
pub const SLUG: &str = "discharge-report";

/// The Xt-EHR element path of the report body the query feeds, the row of
/// the discharge report crosswalk.
pub const ELEMENT: &str = "EHDSDischargeReport.body";

/// The composition archetype a discharge report composition is of.
pub const COMPOSITION: &str = "openEHR-EHR-COMPOSITION.transfer_summary.v1";

/// The section slice of `composition-eu-hdr` the gateway writes the
/// hospital course in.
const COURSE: &str = "sectionHospitalCourse";

/// What the hospital course section says, since the template records no
/// course the mapping carries (our own design).
const COURSE_TEXT: &str = "The member's record holds no hospital course in a form the gateway writes; the member's own record holds the stay.";

/// Returns the query's qualified name,
/// `eu.ferrofed.eehrxf::discharge-report`.
///
/// # Errors
///
/// The [`QueryNameError`] of [`QueryName::new`], which the name does not
/// meet.
pub fn name() -> Result<QueryName, QueryNameError> {
    QueryName::new(&format!("{NAMESPACE}::{SLUG}"))
}

/// Returns the query as AQL text, [`section_query::composed`] of
/// [`COMPOSITION`] printed from its syntax tree.
#[must_use]
pub fn aql() -> String {
    printer::to_aql(&section_query::composed(COMPOSITION), None)
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

/// The shipped discharge mapping compiled against a deployment's discharge
/// report template, which writes a member's composition as an HL7 Europe
/// Hospital Discharge Report 1.0.0 `bundle-eu-hdr` document.
#[derive(Debug)]
pub struct Served {
    mapping: Mapping,
}

/// The language of a canonical JSON composition (RM 1.1.0
/// `COMPOSITION.language`).
#[derive(Deserialize)]
struct Written {
    language: Language,
}

/// A `CODE_PHRASE` of ISO 639-1.
#[derive(Deserialize)]
struct Language {
    code_string: String,
}

impl Served {
    /// Compiles the shipped discharge mapping (`eehrxf::mappings::HDR`)
    /// against `template`.
    ///
    /// # Errors
    ///
    /// [`ServedError::Template`] when the template does not read and
    /// [`ServedError::Compile`] when the mapping does not compile against
    /// it.
    pub fn compile(template: Template<'_>) -> Result<Self, ServedError> {
        let source = template.source().map_err(ServedError::Template)?;
        let mapping =
            deep(|| Mapping::compile_project(&source, &HDR))?.map_err(ServedError::Compile)?;
        Ok(Self { mapping })
    }

    /// Writes `composition`, a discharge composition in canonical JSON, as
    /// the `bundle-eu-hdr` document `report` names, in the composition's
    /// language.
    ///
    /// The document's `Composition` is authored by the gateway's `Device`
    /// and the operator's `Organization`, its subject the `Patient` the
    /// identifier names and its encounter the stay the mapping writes. It
    /// holds each section the mapping writes, with its entries, and a
    /// hospital course that says the record holds none the gateway writes,
    /// which `composition-eu-hdr` `cmp-hdr-2` requires. One `Provenance`
    /// names the member as the source of every mapped entry.
    ///
    /// # Errors
    ///
    /// [`ServedError::Language`] when the composition names no language,
    /// [`ServedError::Run`] when it does not map, and
    /// [`ServedError::Document`] when the document cannot be assembled.
    pub fn document(&self, composition: &str, report: Report<'_>) -> Result<Bundle, ServedError> {
        let written: Written = serde_json::from_str(composition).map_err(ServedError::Language)?;
        let mut document = Document::of(
            &DISCHARGE_REPORT_FORM,
            report.base,
            report.identifier.to_owned(),
            report.timestamp.to_owned(),
            String::from("Discharge report"),
        );
        // NOTE: HL7 Europe HDR 1.0.0 bundle-eu-hdr: Bundle.language is 1..1, the language the
        // member's composition is written in (RM 1.1.0 COMPOSITION.language).
        document.with_language(written.language.code_string);
        let mapped = report::authored(
            &mut document,
            report,
            "Software that writes the discharge report from the member's record automatically",
            (&self.mapping, composition),
        )?;
        let sections = document
            .with_mapped_composition(mapped.bundle())
            .map_err(ServedError::Document)?;
        let entries: Vec<String> = sections
            .iter()
            .flat_map(|section| section.entries.iter().cloned())
            .collect();
        report::sourced(&mut document, report, &entries).map_err(ServedError::Document)?;
        // NOTE: HL7 Europe HDR 1.0.0 composition-eu-hdr cmp-hdr-2: a discharge summary or a
        // hospital course section is required, and the template records neither.
        let course = HDR_SLOTS
            .iter()
            .find(|slot| slot.slice == COURSE)
            .ok_or(ServedError::Document(DocumentError::Unmapped { found: 0 }))?;
        document
            .with_section(Section::new(course, vec![String::from(COURSE_TEXT)]))
            .map_err(ServedError::Document)?;
        for section in sections {
            let paragraph = format!(
                "The {} as the member records them, written by the gateway.",
                section.slot.title.to_lowercase()
            );
            document
                .with_section(section.into_section(vec![paragraph]))
                .map_err(ServedError::Document)?;
        }
        document.into_bundle().map_err(ServedError::Document)
    }
}
