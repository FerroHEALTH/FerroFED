// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every report the gateway writes from one member's composition
//! shares: the laboratory report ([`crate::laboratory`]) and the discharge
//! report ([`crate::discharge`]).
//!
//! Each is a document the gateway authors with its `Device` and the
//! operator's `Organization`, about the `Patient` an identifier names, whose
//! mapped content one `Provenance` traces to the member it was read at.
//! The authorship and the provenance are our own design: no specification
//! governs them.

use eehrxf::category::Category;
use eehrxf::document::discharge::HDR_BUNDLE;
use eehrxf::document::laboratory::LAB_BUNDLE;
use eehrxf::document::{Document, DocumentError};
use eehrxf::mapping::{Mapping, MappingError};
use ferrofed_registry::definition::{QueryName, QueryNameError};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::device::{Device, DeviceDeviceName};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::provenance::{Provenance, ProvenanceAgent};
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::contract::ToFhirResponse;
use fhirconnect::operations::run::Settings;

use crate::stack::{Deep, deep};
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

/// Why a served report's mapping run wrote nothing.
#[derive(Debug)]
pub(crate) enum Unwritten {
    /// The document does not take a resource.
    Document(DocumentError),
    /// The run did not run on its thread.
    Deep(Deep),
    /// The composition does not map.
    Run(MappingError),
}

/// Names in `document` the patient `report` names and the authors, the
/// gateway's `Device`, described as `software`, and the operator's
/// `Organization`, then runs `mapping` over `composition` for them.
///
/// # Errors
///
/// The [`Unwritten`] that says why the run wrote nothing.
pub(crate) fn authored(
    document: &mut Document,
    report: Report<'_>,
    software: &str,
    (mapping, composition): (&Mapping, &str),
) -> Result<ToFhirResponse, Unwritten> {
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
        .map_err(Unwritten::Document)?;
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
        .map_err(Unwritten::Document)?;
    document
        .with_author(&Resource::Organization(Box::new(organization(
            "operator",
            report.operator,
        ))))
        .map_err(Unwritten::Document)?;
    let settings = Settings::new(device.clone(), report.timestamp);
    let context = CallContext::new()
        .with_patient(reference(&subject))
        .with_who(reference(&device));
    deep(|| mapping.to_fhir_with(composition, &settings, context))
        .map_err(Unwritten::Deep)?
        .map_err(Unwritten::Run)
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

/// A category of report the gateway writes from one member's composition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Written {
    /// The laboratory report, Art 14(1)(e) ([`crate::laboratory`]).
    Laboratory,
    /// The discharge report, Art 14(1)(f) ([`crate::discharge`]).
    Discharge,
}

impl Written {
    /// Returns the written category of the code `code`, such as
    /// `Discharge-Reports`.
    #[must_use]
    pub fn of_category(code: &str) -> Option<Self> {
        [Self::Laboratory, Self::Discharge]
            .into_iter()
            .find(|written| written.category() == code)
    }

    /// Returns the qualified name of the category's stored query.
    ///
    /// # Errors
    ///
    /// The [`QueryNameError`] of the name, which no query meets.
    pub fn name(self) -> Result<QueryName, QueryNameError> {
        match self {
            Self::Laboratory => crate::laboratory::name(),
            Self::Discharge => crate::discharge::name(),
        }
    }

    /// Returns the category's stored query as AQL text.
    #[must_use]
    pub fn aql(self) -> String {
        match self {
            Self::Laboratory => crate::laboratory::aql(),
            Self::Discharge => crate::discharge::aql(),
        }
    }

    /// Returns the code of the priority category, such as
    /// `Discharge-Reports`.
    #[must_use]
    pub const fn category(self) -> &'static str {
        match self {
            Self::Laboratory => Category::Laboratory.code(),
            Self::Discharge => Category::Discharge.code(),
        }
    }

    /// Returns the document `Bundle` profile the report is written in.
    #[must_use]
    pub const fn bundle(self) -> &'static str {
        match self {
            Self::Laboratory => LAB_BUNDLE,
            Self::Discharge => HDR_BUNDLE,
        }
    }

    /// Returns the report's name, in the words a log or an outcome names it
    /// by.
    #[must_use]
    pub const fn named(self) -> &'static str {
        match self {
            Self::Laboratory => "laboratory report",
            Self::Discharge => "discharge report",
        }
    }
}
