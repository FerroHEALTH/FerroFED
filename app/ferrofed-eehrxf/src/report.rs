// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every report the gateway writes from one member's composition
//! shares.
//!
//! The reports are the laboratory report ([`crate::laboratory`]), the
//! discharge report ([`crate::discharge`]), the imaging report
//! ([`crate::imaging`]) and the vaccination history
//! ([`crate::vaccination`]).
//!
//! Each is a document the gateway authors with its `Device` and the
//! operator's `Organization`, about the `Patient` an identifier names and
//! the identity binding describes, whose mapped content one `Provenance`
//! traces to the member it was read at, which the `Composition` names its
//! custodian. The authorship and the provenance are our own design: no
//! specification governs them.

use std::collections::BTreeSet;

use async_trait::async_trait;
use eehrxf::category::Category;
use eehrxf::document::authored::recorded_author;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::document::imaging::IMAGING_BUNDLE;
use eehrxf::document::laboratory::LAB_BUNDLE;
use eehrxf::document::language::LanguageError;
use eehrxf::document::vaccination::EUVAC_BUNDLE;
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::mappings::{Project, UnknownContext};
use eehrxf::terminology::Terminology;
use ferrofed_identity::role::header::{PatientHeader, PersonName};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::provenance::{Provenance, ProvenanceAgent};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::contract::ToFhirResponse;
use fhirconnect::operations::issues::{OPENEHR_PATH_SYSTEM, WARNING};
use fhirconnect::operations::run::Settings;
use openehr_its::rest::stored_query::{QualifiedQueryName, QueryNameError};

use crate::prescriptions::mapped::{MapModeError, Template};
use crate::summary::Organisation;
use crate::summary::resources::{absent, header_patient, human_name, organization, reference};
use crate::vaccination::{Attester, Unattested, Unnamed};

/// What one served report names beside its composition.
///
/// It is the gateway's absolute FHIR base, the document's identifier, a
/// `urn:uuid:` URN, when it is written, a FHIR `instant`, the patient by an
/// identifier with what the identity binding holds of them, the operator
/// and the member it was read at.
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
    /// What the identity binding holds of the patient; an element it does
    /// not hold is written with a `data-absent-reason` of `unknown`.
    pub header: &'a PatientHeader,
    /// The operator, the document's author beside the gateway.
    pub operator: &'a Organisation,
    /// The member the composition was read at.
    pub member: &'a Organisation,
    /// The persons who attested the composition's version, its legal
    /// authenticators; none for a document that names none.
    pub attesters: &'a [Attester],
}

/// Why a report the gateway writes is not served; no variant's text names
/// a value a composition carries.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServedError {
    /// The shipped project names no context of the name selected.
    #[error("the shipped project names no context to compile")]
    Context(#[source] UnknownContext),
    /// The template does not read, or its operational template cannot be
    /// built.
    #[error("the report template does not read")]
    Template(#[source] MapModeError),
    /// The shipped mapping does not compile against the template.
    #[error("the shipped mapping does not compile against the template")]
    Compile(#[source] MappingError),
    /// The composition is no canonical JSON.
    #[error("the composition is no canonical JSON")]
    Composition(#[source] serde_json::Error),
    /// The composition's language and territory give no language the
    /// document's form takes.
    #[error("the composition names no language the report's form takes")]
    Language(#[source] LanguageError),
    /// The composition names no author the report's form takes.
    #[error("the composition names no author the report's form takes")]
    Author(#[source] DocumentError),
    /// No person attested the composition's version, and the document's
    /// form requires a legal authenticator.
    #[error("the composition names no legal authenticator the document's form requires")]
    Unattested(#[source] Unattested),
    /// The patient lacks what the document's form requires of its subject.
    #[error("the patient lacks what the report's form requires of its subject")]
    Unnamed(#[source] Unnamed),
    /// The composition does not map.
    #[error("the composition does not map")]
    Run(#[source] MappingError),
    /// The document cannot be assembled from what the composition maps to.
    #[error("the report document cannot be assembled")]
    Document(#[source] DocumentError),
}

/// Compiles the shipped `project` against `template`.
///
/// # Errors
///
/// [`ServedError::Template`] when the template does not read and
/// [`ServedError::Compile`] when the project does not compile against it.
pub(crate) fn compiled(template: Template<'_>, project: &Project) -> Result<Mapping, ServedError> {
    let source = template.source().map_err(ServedError::Template)?;
    Mapping::compile_project(&source, project).map_err(ServedError::Compile)
}

/// Who a written report names as its authors.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Author<'a> {
    /// The gateway's `Device`, described as `software`, and the operator's
    /// `Organization`, for a form whose profile allows a `Device` author.
    Gateway(&'a str),
    /// The author the member's composition, in canonical JSON, records
    /// (`eehrxf::document::authored::recorded_author`).
    Record(&'a serde_json::Value),
}

/// Names in `document` the patient `report` names and the authors `author`
/// names, then runs `mapping` over `composition` for them.
///
/// # Errors
///
/// [`ServedError::Document`] for a resource the document does not take,
/// [`ServedError::Author`] for a composition that records no author, and
/// [`ServedError::Run`] when the composition does not map.
pub(crate) fn authored(
    document: &mut Document,
    report: Report<'_>,
    author: Author<'_>,
    (mapping, composition): (&Mapping, &str),
) -> Result<ToFhirResponse, ServedError> {
    let subject = document
        .with_patient(patient(report))
        .map_err(ServedError::Document)?;
    let who = match author {
        Author::Gateway(software) => gateway(document, report, software)?,
        Author::Record(record) => {
            let recorded = recorded_author(record).map_err(ServedError::Author)?;
            document
                .with_author(&recorded)
                .map_err(ServedError::Document)?
        }
    };
    let settings = Settings::new(who.clone(), report.timestamp);
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&who));
    mapping
        .to_fhir_with(composition, &settings, context)
        .map_err(ServedError::Run)
}

/// Names in `document` the gateway's `Device`, described as `software`, and
/// the operator's `Organization` as its authors, returning the `Device`'s
/// `fullUrl`.
fn gateway(
    document: &mut Document,
    report: Report<'_>,
    software: &str,
) -> Result<String, ServedError> {
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
                text: Some(software.into()),
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
    Ok(device)
}

/// The `Patient` of `report`: the identifier it is named by and every
/// element the identity binding holds, as the patient summary writes them.
///
/// A name or a birth date the binding does not hold is written with a
/// `data-absent-reason` of `unknown` in place of a value, never filled in.
fn patient(report: Report<'_>) -> Patient {
    let mut patient = header_patient("patient", report.patient, report.header);
    // NOTE: HL7 Europe Base 2.0.1 patient-eu-core Patient.name 1..*, eu-pat-1: a name with no
    // family, given or text SHALL carry a data-absent-reason extension instead.
    patient.name = report
        .header
        .names
        .iter()
        .filter(|name| name.names())
        .map(human_name)
        .collect();
    if patient.name.is_empty() {
        patient.name.push(HumanName {
            extension: vec![absent()],
            ..human_name(&PersonName::default())
        });
    }
    patient
}

/// Adds to `document` the member `report` names as the `Organization` the
/// `Composition` names its custodian, and one `Provenance` naming it as the
/// source of `targets`, the `fullUrl`s of what the run mapped.
///
/// # Errors
///
/// The [`DocumentError`] of a resource the document does not take.
pub(crate) fn sourced(
    document: &mut Document,
    report: Report<'_>,
    targets: &[String],
) -> Result<(), DocumentError> {
    // NOTE: FHIR R4 documents.html §3.3.1 admits only what the Composition references; FHIR R4
    // Composition.custodian is who maintains and gives access to it, the member holding the source.
    let member = document.with_custodian(organization("member", report.member))?;
    document.add(&[Resource::Provenance(Box::new(Provenance {
        target: targets.iter().map(|url| reference(url)).collect(),
        recorded: report.timestamp.into(),
        agent: vec![ProvenanceAgent {
            who: reference(&member),
            ..ProvenanceAgent::default()
        }],
        ..Provenance::default()
    }))])?;
    Ok(())
}

/// The narrative line naming, by path and never by value, each element of
/// the member's composition the mapping run `run` declared it did not carry, so a
/// loss is never silent in the report; `None` when it declared none.
///
/// The run declares each loss as a `warning` issue of its `OperationOutcome`,
/// which the document leaves out because it is no document content (FHIR R4
/// documents.html). A loss is named by its openEHR path, which says where
/// the member's record holds it, or by its FHIR path when it has no openEHR
/// one; one with neither is counted alone.
pub(crate) fn withheld(run: &Bundle) -> Option<String> {
    let mut count = 0_usize;
    let mut paths = BTreeSet::new();
    let issues = run
        .entry
        .iter()
        .filter_map(|entry| match entry.resource.as_ref() {
            Some(Resource::OperationOutcome(outcome)) => Some(outcome.issue.iter()),
            _ => None,
        })
        .flatten()
        .filter(|issue| issue.severity.value.as_deref() == Some(WARNING));
    for issue in issues {
        count = count.saturating_add(1);
        let openehr = issue
            .details
            .iter()
            .flat_map(|details| details.coding.iter())
            .find(|coding| {
                coding
                    .system
                    .as_ref()
                    .and_then(|system| system.value.as_deref())
                    == Some(OPENEHR_PATH_SYSTEM)
            })
            .and_then(|coding| coding.code.as_ref())
            .and_then(|code| code.value.as_deref());
        let fhir = issue
            .expression
            .first()
            .and_then(|expression| expression.value.as_deref());
        paths.extend(openehr.or(fhir).map(str::to_owned));
    }
    if count == 0 {
        return None;
    }
    let named = if paths.is_empty() {
        String::new()
    } else {
        format!(" ({})", paths.into_iter().collect::<Vec<_>>().join(", "))
    };
    Some(format!(
        "{count} element(s) of the member's record could not be carried into this report{named}, so the record holds more than the report shows."
    ))
}

/// A shipped mapping compiled against a deployment's template, which writes
/// a member's composition as the document of its category.
#[async_trait]
pub trait Writes: std::fmt::Debug + Send + Sync {
    /// Writes `composition`, a composition in canonical JSON, as the
    /// document `report` names, asking `terminology` whether a value set
    /// the form binds holds what the document writes.
    ///
    /// # Errors
    ///
    /// The [`ServedError`] that says why no document is written.
    async fn document(
        &self,
        composition: &str,
        report: Report<'_>,
        terminology: &Terminology,
    ) -> Result<Bundle, ServedError>;
}

/// A category of report the gateway writes from one member's composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Written {
    /// The laboratory report, Art 14(1)(e) ([`crate::laboratory`]).
    Laboratory,
    /// The discharge report, Art 14(1)(f) ([`crate::discharge`]).
    Discharge,
    /// The imaging report, Art 14(1)(d) ([`crate::imaging`]).
    Imaging,
    /// The vaccination history, MyHealth@EU Requirements Catalogue v10.0.0
    /// §1.24 ([`crate::vaccination`]).
    Vaccination,
}

impl Written {
    /// Every written category.
    pub const ALL: [Self; 4] = [
        Self::Laboratory,
        Self::Discharge,
        Self::Imaging,
        Self::Vaccination,
    ];

    /// Returns the written category of the priority category code `code`,
    /// such as `Discharge-Reports`; none for the vaccination history, which
    /// is of no priority category.
    #[must_use]
    pub fn of_category(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|written| {
            written
                .priority()
                .is_some_and(|priority| priority.code() == code)
        })
    }

    /// Returns the written category of the code [`Written::code`] gives.
    #[must_use]
    pub fn of_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|written| written.code() == code)
    }

    /// Returns the qualified name of the category's stored query.
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of the name, which no query meets.
    pub fn name(self) -> Result<QualifiedQueryName, QueryNameError> {
        match self {
            Self::Laboratory => crate::laboratory::name(),
            Self::Discharge => crate::discharge::name(),
            Self::Imaging => crate::imaging::name(),
            Self::Vaccination => crate::vaccination::name(),
        }
    }

    /// Returns the category's stored query as AQL text.
    #[must_use]
    pub fn aql(self) -> String {
        match self {
            Self::Laboratory => crate::laboratory::aql(),
            Self::Discharge => crate::discharge::aql(),
            Self::Imaging => crate::imaging::aql(),
            Self::Vaccination => crate::vaccination::aql(),
        }
    }

    /// Returns the Art 14(1) priority category of the report; none for the
    /// vaccination history, which MyHealth@EU asks for beside them.
    #[must_use]
    pub const fn priority(self) -> Option<Category> {
        match self {
            Self::Laboratory => Some(Category::Laboratory),
            Self::Discharge => Some(Category::Discharge),
            Self::Imaging => Some(Category::Imaging),
            Self::Vaccination => None,
        }
    }

    /// Returns the code a document URL names the category by: the priority
    /// category code, such as `Discharge-Reports`, or `Vaccination-History`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self.priority() {
            Some(priority) => priority.code(),
            None => VACCINATION_HISTORY,
        }
    }

    /// Returns the category's document `Bundle` profile, the format a
    /// listing names; the discharge report's MyHealth@EU `Bundle` profile is
    /// derived from it.
    #[must_use]
    pub const fn bundle(self) -> &'static str {
        match self {
            Self::Laboratory => LAB_BUNDLE,
            Self::Discharge => HDR_BUNDLE,
            Self::Imaging => IMAGING_BUNDLE,
            Self::Vaccination => EUVAC_BUNDLE,
        }
    }

    /// Returns the report's name, in the words a log or an outcome names it
    /// by.
    #[must_use]
    pub const fn named(self) -> &'static str {
        match self {
            Self::Laboratory => "laboratory report",
            Self::Discharge => "discharge report",
            Self::Imaging => "imaging report",
            Self::Vaccination => "vaccination history",
        }
    }
}

/// The code [`Written::code`] names the vaccination history by (no
/// specification governs this: our own design).
const VACCINATION_HISTORY: &str = "Vaccination-History";

#[cfg(test)]
mod tests {
    use fhir_types::r4::bundle::{Bundle, BundleEntry};
    use fhir_types::r4::codeable_concept::CodeableConcept;
    use fhir_types::r4::coding::Coding;
    use fhir_types::r4::operation_outcome::{OperationOutcome, OperationOutcomeIssue};
    use fhir_types::r4::resource::Resource;
    use fhirconnect::operations::issues::OPENEHR_PATH_SYSTEM;

    use super::withheld;

    /// An issue of `severity`, naming `openehr` by its openEHR path and
    /// `fhir` by its FHIR path when either is given.
    fn issue(severity: &str, openehr: Option<&str>, fhir: Option<&str>) -> OperationOutcomeIssue {
        OperationOutcomeIssue {
            severity: severity.into(),
            code: "incomplete".into(),
            details: openehr.map(|path| CodeableConcept {
                coding: vec![Coding {
                    system: Some(OPENEHR_PATH_SYSTEM.into()),
                    code: Some(path.into()),
                    ..Coding::default()
                }],
                ..CodeableConcept::default()
            }),
            expression: fhir.into_iter().map(Into::into).collect(),
            ..OperationOutcomeIssue::default()
        }
    }

    /// A run whose `OperationOutcome` holds `issues`.
    fn run(issues: Vec<OperationOutcomeIssue>) -> Bundle {
        Bundle {
            r#type: "collection".into(),
            entry: vec![BundleEntry {
                resource: Some(Resource::OperationOutcome(Box::new(OperationOutcome {
                    issue: issues,
                    ..OperationOutcome::default()
                }))),
                ..BundleEntry::default()
            }],
            ..Bundle::default()
        }
    }

    #[test]
    fn a_run_that_declares_nothing_adds_no_line() {
        assert_eq!(withheld(&run(vec![issue("information", None, None)])), None);
    }

    #[test]
    fn a_loss_is_named_by_its_openehr_path_before_its_fhir_path() {
        let line = withheld(&run(vec![issue(
            "warning",
            Some("/content[openEHR-EHR-OBSERVATION.imaging_exam_result.v1]/data/events/data/items[at0012]"),
            Some("DiagnosticReport.conclusion"),
        )]))
        .unwrap();
        assert!(line.starts_with("1 element(s)"), "{line}");
        assert!(line.contains("items[at0012]"), "{line}");
        assert!(!line.contains("DiagnosticReport.conclusion"), "{line}");
    }

    #[test]
    fn a_loss_with_no_openehr_path_is_named_by_its_fhir_path_and_one_with_neither_is_counted() {
        let line = withheld(&run(vec![
            issue("warning", None, Some("Composition.section[3]")),
            issue("warning", None, None),
        ]))
        .unwrap();
        assert!(line.starts_with("2 element(s)"), "{line}");
        assert!(line.contains("(Composition.section[3])"), "{line}");
    }

    #[test]
    fn a_loss_named_by_no_path_is_counted_alone() {
        assert_eq!(
            withheld(&run(vec![issue("warning", None, None)])).as_deref(),
            Some(
                "1 element(s) of the member's record could not be carried into this report, so the record holds more than the report shows."
            )
        );
    }
}
