// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stored query of the laboratory report.
//!
//! A laboratory report is one document per report (Regulation (EU)
//! 2025/327 Art 14(1)(e)): the HL7 Europe Laboratory Report's
//! `Composition-eu-lab` heads it and its `DiagnosticReport-eu-lab` carries
//! the body. The openEHR side is one composition per report, so the
//! category has one stored query, held read-only under the reserved
//! namespace at [`VERSION`] (§12.7, N44), in the shape of
//! [`section_query`]: every whole composition that contains one of
//! [`ARCHETYPES`], with its uid and its template id.
//!
//! The archetypes are the entries the openEHR international Clinical
//! Knowledge Manager's laboratory test result template names (CKM cid
//! `1013.26.408` "Generic lab test result example simple", asset version 3,
//! DRAFT, vendored under `docs/specs/openehr-ckm-laboratory/`). The
//! template is rooted at `openEHR-EHR-COMPOSITION.report-result.v1`; the
//! query names its entry alone and no composition archetype, so a CDR that
//! records the result under another report composition still answers.
//!
//! [`Served`] writes one such composition as an HL7 Europe Laboratory
//! Report 2.0.0 `Bundle-eu-lab` document (`LABORATORY_REPORT_FORM`), through
//! the shipped laboratory mapping compiled against the deployment's
//! template, for the document list of the FHIR face (Regulation (EU)
//! 2025/327 Art 14(1)(e), Annex II 2.1).

use eehrxf::document::laboratory::{LAB_SLOTS, LABORATORY_REPORT_FORM};
use eehrxf::document::section::Section;
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::Mapping;
use eehrxf::mappings::LABORATORY;
use ferrofed_registry::definition::{StoredDefinition, query_name};
use fhir_types::r4::bundle::Bundle;
use openehr_its::rest::stored_query::{QualifiedQueryName, QueryNameError};
use openehr_query::ast::SelectQuery;
use openehr_query::printer;

use crate::prescriptions::mapped::Template;
use crate::report::{self, Report, ServedError, Writes};
use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};

/// The query's name in the reserved namespace.
pub const SLUG: &str = "laboratory-report";

/// The Xt-EHR element path of the report body the query feeds, the row of
/// the laboratory report crosswalk.
pub const ELEMENT: &str = "EHDSLaboratoryReport.body";

/// The entry archetypes a laboratory report composition contains, at least
/// one of them, as the template names them.
pub const ARCHETYPES: &[Archetype] = &[Archetype {
    class: "OBSERVATION",
    id: "openEHR-EHR-OBSERVATION.laboratory_test_result.v1",
}];

/// Returns the query's qualified name,
/// `eu.ferrofed.eehrxf::laboratory-report`.
///
/// # Errors
///
/// The [`QueryNameError`] of [`QualifiedQueryName::new`], which the name does not
/// meet.
pub fn name() -> Result<QualifiedQueryName, QueryNameError> {
    query_name(&format!("{NAMESPACE}::{SLUG}"))
}

/// Returns the query as an AQL syntax tree.
///
/// # Panics
///
/// Never: [`ARCHETYPES`] names an archetype.
#[must_use]
#[expect(
    clippy::expect_used,
    reason = "ARCHETYPES names one archetype, which a test holds"
)]
pub fn query() -> SelectQuery {
    section_query::query(ARCHETYPES, &[]).expect("the laboratory report should name an archetype")
}

/// Returns the query as AQL text, printed from its syntax tree.
#[must_use]
pub fn aql() -> String {
    printer::to_aql(&query(), None)
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

/// The section a mapped laboratory report lists its results in: the
/// `lab-studyType-eu-lab` code of laboratory studies in general, since the
/// composition names no study type of its own (our own design).
const SECTION: &str = "26436-6";

/// The shipped laboratory mapping compiled against a deployment's
/// laboratory report template, which writes a member's composition as an
/// HL7 Europe Laboratory Report 2.0.0 `Bundle-eu-lab` document.
#[derive(Debug)]
pub struct Served {
    mapping: Mapping,
}

/// The context of the shipped laboratory project a laboratory composition
/// is mapped by, in both directions.
pub const RESULT_CONTEXT: &str = "ferrobridge_eu_laboratory.context";

impl Served {
    /// Compiles the result context ([`RESULT_CONTEXT`]) of the shipped
    /// laboratory mapping (`eehrxf::mappings::LABORATORY`) against
    /// `template`.
    ///
    /// # Errors
    ///
    /// [`ServedError::Template`] when the template does not read,
    /// [`ServedError::Context`] when the project names no result context, and
    /// [`ServedError::Compile`] when the mapping does not compile against it.
    pub fn compile(template: Template<'_>) -> Result<Self, ServedError> {
        let source = template.source().map_err(ServedError::Template)?;
        // NOTE: no specification governs this: our own design; the project may list more than one
        // context over one template, so only the result context is compiled and run.
        let result = LABORATORY
            .with_context(RESULT_CONTEXT)
            .map_err(ServedError::Context)?;
        let mapping = Mapping::compile_project(&source, &result).map_err(ServedError::Compile)?;
        Ok(Self { mapping })
    }
}

impl Writes for Served {
    /// Writes `composition`, a laboratory composition in canonical JSON, as
    /// the `Bundle-eu-lab` document `report` names.
    ///
    /// The document's `Composition` is authored by the gateway's `Device`
    /// and the operator's `Organization`, its custodian the member's
    /// `Organization`, its subject the `Patient` the identifier names, its report the `DiagnosticReport-eu-lab` the
    /// mapping writes with its results listed in one section, and one
    /// `Provenance` names the member as the source of those results.
    ///
    /// # Errors
    ///
    /// [`ServedError::Run`] when the composition does not map, and
    /// [`ServedError::Document`] when the document cannot be assembled.
    fn document(&self, composition: &str, report: Report<'_>) -> Result<Bundle, ServedError> {
        let mut document = Document::of(
            &LABORATORY_REPORT_FORM,
            report.base,
            report.identifier.to_owned(),
            report.timestamp.to_owned(),
            String::from("Laboratory report"),
        );
        let mapped = report::authored(
            &mut document,
            report,
            "Software that writes the laboratory report from the member's record automatically",
            (&self.mapping, composition),
        )?;
        let results = document
            .with_mapped_report(mapped.bundle())
            .map_err(ServedError::Document)?;
        report::sourced(&mut document, report, &results).map_err(ServedError::Document)?;
        let slot = LAB_SLOTS
            .iter()
            .find(|slot| slot.code == SECTION)
            .ok_or(ServedError::Document(DocumentError::Unmapped { found: 0 }))?;
        let mut section = Section::new(
            slot,
            vec![String::from(
                "The results as the member records them, written by the gateway.",
            )],
        );
        for result in results {
            section = section.with_entry(result);
        }
        document
            .with_section(section)
            .map_err(ServedError::Document)?;
        document.into_bundle().map_err(ServedError::Document)
    }
}
