// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every report the gateway writes from one member's composition
//! shares.
//!
//! The reports are the laboratory report ([`crate::laboratory`]), the
//! discharge report ([`crate::discharge`]) and the imaging report
//! ([`crate::imaging`]).
//!
//! Each is a document the gateway authors with its `Device` and the
//! operator's `Organization`, about the `Patient` an identifier names, whose
//! mapped content one `Provenance` traces to the member it was read at.
//! The authorship and the provenance are our own design: no specification
//! governs them.

use eehrxf::category::Category;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::document::imaging::IMAGING_BUNDLE;
use eehrxf::document::laboratory::LAB_BUNDLE;
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::mappings::{Project, UnknownContext};
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::provenance::{Provenance, ProvenanceAgent};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::contract::ToFhirResponse;
use fhirconnect::operations::run::Settings;
use openehr_its::rest::stored_query::{QualifiedQueryName, QueryNameError};

use crate::prescriptions::mapped::{MapModeError, Template};
use crate::summary::Organisation;
use crate::summary::resources::{organization, reference};

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
    /// The composition is no canonical JSON composition with a language.
    #[error("the composition names no language")]
    Language(#[source] serde_json::Error),
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

/// Names in `document` the patient `report` names and the authors, the
/// gateway's `Device`, described as `software`, and the operator's
/// `Organization`, then runs `mapping` over `composition` for them.
///
/// # Errors
///
/// [`ServedError::Document`] for a resource the document does not take,
/// and [`ServedError::Run`] when the composition does not map.
pub(crate) fn authored(
    document: &mut Document,
    report: Report<'_>,
    software: &str,
    (mapping, composition): (&Mapping, &str),
) -> Result<ToFhirResponse, ServedError> {
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
    let settings = Settings::new(device.clone(), report.timestamp);
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&device));
    mapping
        .to_fhir_with(composition, &settings, context)
        .map_err(ServedError::Run)
}

/// Adds to `document` the member `report` names as an `Organization`, and
/// one `Provenance` naming it as the source of `targets`, the `fullUrl`s of
/// what the run mapped.
///
/// # Errors
///
/// The [`DocumentError`] of a resource the document does not take.
pub(crate) fn sourced(
    document: &mut Document,
    report: Report<'_>,
    targets: &[String],
) -> Result<(), DocumentError> {
    let member = document.add(&[Resource::Organization(Box::new(organization(
        "member",
        report.member,
    )))])?;
    if let Some(member) = member.first() {
        document.add(&[Resource::Provenance(Box::new(Provenance {
            target: targets.iter().map(|url| reference(url)).collect(),
            recorded: report.timestamp.into(),
            agent: vec![ProvenanceAgent {
                who: reference(member),
                ..ProvenanceAgent::default()
            }],
            ..Provenance::default()
        }))])?;
    }
    Ok(())
}

/// A shipped mapping compiled against a deployment's template, which writes
/// a member's composition as the document of its category.
pub trait Writes: std::fmt::Debug + Send + Sync {
    /// Writes `composition`, a composition in canonical JSON, as the
    /// document `report` names.
    ///
    /// # Errors
    ///
    /// The [`ServedError`] that says why no document is written.
    fn document(&self, composition: &str, report: Report<'_>) -> Result<Bundle, ServedError>;
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
}

impl Written {
    /// Returns the written category of the code `code`, such as
    /// `Discharge-Reports`.
    #[must_use]
    pub fn of_category(code: &str) -> Option<Self> {
        [Self::Laboratory, Self::Discharge, Self::Imaging]
            .into_iter()
            .find(|written| written.category() == code)
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
        }
    }

    /// Returns the category's stored query as AQL text.
    #[must_use]
    pub fn aql(self) -> String {
        match self {
            Self::Laboratory => crate::laboratory::aql(),
            Self::Discharge => crate::discharge::aql(),
            Self::Imaging => crate::imaging::aql(),
        }
    }

    /// Returns the code of the priority category, such as
    /// `Discharge-Reports`.
    #[must_use]
    pub const fn category(self) -> &'static str {
        match self {
            Self::Laboratory => Category::Laboratory.code(),
            Self::Discharge => Category::Discharge.code(),
            Self::Imaging => Category::Imaging.code(),
        }
    }

    /// Returns the document `Bundle` profile the report is written in.
    #[must_use]
    pub const fn bundle(self) -> &'static str {
        match self {
            Self::Laboratory => LAB_BUNDLE,
            Self::Discharge => HDR_BUNDLE,
            Self::Imaging => IMAGING_BUNDLE,
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
        }
    }
}
