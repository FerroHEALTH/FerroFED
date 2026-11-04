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
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::mappings::{LABORATORY, UnknownContext};
use ferrofed_registry::definition::{QueryName, QueryNameError, StoredDefinition};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::provenance::{Provenance, ProvenanceAgent};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;
use openehr_mapping_core::template::{PathError, TemplateSource};
use openehr_query::ast::SelectQuery;
use openehr_query::printer;

use crate::reserved::{NAMESPACE, SAVED, VERSION};
use crate::section_query::{self, Archetype};
use crate::stack::{Deep, deep};
use crate::summary::Organisation;
use crate::summary::resources::{organization, reference};

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
/// The [`QueryNameError`] of [`QueryName::new`], which the name does not
/// meet.
pub fn name() -> Result<QueryName, QueryNameError> {
    QueryName::new(&format!("{NAMESPACE}::{SLUG}"))
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

/// Why a laboratory report is not served; no variant's text names a value
/// a composition carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServedError {
    /// The shipped laboratory project names no [`RESULT_CONTEXT`].
    #[error("the shipped laboratory project names no result context")]
    Context(#[source] UnknownContext),
    /// The OPT 1.4 template does not read.
    #[error("the laboratory report template does not read as an OPT 1.4")]
    Template(#[source] PathError),
    /// The shipped laboratory mapping does not compile against the template.
    #[error("the shipped laboratory mapping does not compile against the template")]
    Compile(#[source] MappingError),
    /// The mapping did not run on its thread.
    #[error(transparent)]
    Deep(#[from] Deep),
    /// The composition does not map.
    #[error("the laboratory composition does not map")]
    Run(#[source] MappingError),
    /// The document cannot be assembled from what the composition maps to.
    #[error("the laboratory report document cannot be assembled")]
    Document(#[source] DocumentError),
}

/// What one served report names beside its composition.
///
/// It is the gateway's absolute FHIR base, the document's identifier, a
/// `urn:uuid:` URN, when it is written, a FHIR `instant`, the patient by an
/// identifier, the operator and the member it was read at.
#[derive(Debug, Clone, Copy)]
pub struct Report<'a> {
    /// The absolute FHIR base the entries are named under.
    pub base: &'a str,
    /// The document's `Bundle.identifier`.
    pub identifier: &'a str,
    /// When it is written.
    pub timestamp: &'a str,
    /// The patient, as a system and a value.
    pub patient: (&'a str, &'a str),
    /// The operator, the document's author beside the gateway.
    pub operator: &'a Organisation,
    /// The member the composition was read at.
    pub member: &'a Organisation,
}

impl Served {
    /// Compiles the result context ([`RESULT_CONTEXT`]) of the shipped
    /// laboratory mapping (`eehrxf::mappings::LABORATORY`) against the OPT
    /// 1.4 template `opt`.
    ///
    /// # Errors
    ///
    /// [`ServedError::Template`] when `opt` does not read,
    /// [`ServedError::Context`] when the project names no result context, and
    /// [`ServedError::Compile`] when the mapping does not compile against it.
    pub fn compile(opt: &str) -> Result<Self, ServedError> {
        let source = TemplateSource::opt14(opt).map_err(ServedError::Template)?;
        // NOTE: no specification governs this: our own design; the project may list more than one
        // context over one template, so only the result context is compiled and run.
        let result = LABORATORY
            .with_context(RESULT_CONTEXT)
            .map_err(ServedError::Context)?;
        let mapping =
            deep(|| Mapping::compile_project(&source, &result))?.map_err(ServedError::Compile)?;
        Ok(Self { mapping })
    }

    /// Writes `composition`, a laboratory composition in canonical JSON, as
    /// the `Bundle-eu-lab` document `report` names.
    ///
    /// The document's `Composition` is authored by the gateway's `Device`
    /// and the operator's `Organization`, its subject the `Patient` the
    /// identifier names, its report the `DiagnosticReport-eu-lab` the
    /// mapping writes with its results listed in one section, and one
    /// `Provenance` names the member as the source of those results.
    ///
    /// # Errors
    ///
    /// [`ServedError::Run`] when the composition does not map, and
    /// [`ServedError::Document`] when the document cannot be assembled.
    pub fn document(&self, composition: &str, report: Report<'_>) -> Result<Bundle, ServedError> {
        let mut document = Document::of(
            &LABORATORY_REPORT_FORM,
            report.base,
            report.identifier.to_owned(),
            report.timestamp.to_owned(),
            String::from("Laboratory report"),
        );
        let (system, value) = report.patient;
        let subject = document
            .with_patient(Patient {
                id: Some(String::from("patient")),
                identifier: vec![Identifier {
                    system: Some(system.into()),
                    value: Some(value.into()),
                    ..Identifier::default()
                }],
                ..Patient::default()
            })
            .map_err(ServedError::Document)?;
        let operator = document.full_url("Organization", "operator");
        let device = document
            .with_author(&Resource::Device(Box::new(Device {
                id: Some(String::from("gateway")),
                device_name: vec![DeviceDeviceName {
                    name: "FerroFED".into(),
                    r#type: "manufacturer-name".into(),
                    ..DeviceDeviceName::default()
                }],
                r#type: Some(CodeableConcept {
                    text: Some("Software that writes the laboratory report from the member's record automatically".into()),
                    ..CodeableConcept::default()
                }),
                owner: Some(reference(&operator)),
                ..Device::default()
            })))
            .map_err(ServedError::Document)?;
        document
            .with_author(&Resource::Organization(Box::new(organization(
                "operator",
                report.operator,
            ))))
            .map_err(ServedError::Document)?;
        let context = CallContext::new()
            .with_patient(reference(&subject))
            .with_who(reference(&device));
        let settings = Settings::new(device.clone(), report.timestamp);
        let run = deep(|| self.mapping.to_fhir_with(composition, &settings, context))?
            .map_err(ServedError::Run)?;
        let results = document
            .with_mapped_report(run.bundle())
            .map_err(ServedError::Document)?;
        let member = document
            .add(&[Resource::Organization(Box::new(organization(
                "member",
                report.member,
            )))])
            .map_err(ServedError::Document)?;
        if let Some(member) = member.first() {
            document
                .add(&[Resource::Provenance(Box::new(Provenance {
                    target: results.iter().map(|url| reference(url)).collect(),
                    recorded: report.timestamp.into(),
                    agent: vec![ProvenanceAgent {
                        who: reference(member),
                        ..ProvenanceAgent::default()
                    }],
                    ..Provenance::default()
                }))])
                .map_err(ServedError::Document)?;
        }
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
