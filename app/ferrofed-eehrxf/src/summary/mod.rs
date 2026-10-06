// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document a federation assembles from what its members
//! answered to the section queries.
//!
//! Each [`Section`] query selects whole compositions from every member that
//! holds the patient ([`crate::patient_summary`]). [`assemble`] maps each
//! composition a section selected through every mapping the deployment
//! supplies for that section and the composition's template
//! ([`mappings`]), and writes the HL7 Europe Patient Summary document
//! ([`eehrxf::document`]) with these rules:
//!
//! - the composition is authored by the gateway's `Device`, which states that
//!   the summary is generated automatically (eHN PS A.1.4.3), and by the
//!   operator's `Organization` (A.1.5); no attester is named;
//! - each section is authored by the `Organization` of every member whose
//!   data it holds, and every mapped run carries one `Provenance` naming the
//!   source composition and that member;
//! - a member's data are listed as mapped, beside every other member's,
//!   never merged with them;
//! - each section's narrative is generated and says that it is not
//!   exhaustive, names every member that gave no answer, and counts every
//!   composition no mapping covers, by member and template, which is
//!   reported and never dropped;
//! - a section with no entry is `nilknown` only when every member answered
//!   and none holds content for it, and `unavailable` otherwise.
//!
//! No specification governs how a federation authors a summary: these are
//! our own design, against the eHealth Network guideline's header elements
//! and the EPS composition's author, section and empty-reason elements.

pub mod mappings;

use std::collections::{BTreeMap, BTreeSet};

use eehrxf::document::{Document, DocumentError, EmptyReason};
use eehrxf::mapping::{Mapping, MappingError};
use ferrofed_identity::role::header::{PatientHeader, PersonName, PostalAddress, Telecom};
use fhir_types::r4::address::Address;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::contact_point::ContactPoint;
use fhir_types::r4::device::{Device, DeviceDeviceName, DeviceVersion};
use fhir_types::r4::extension::{Extension, ExtensionValue};
use fhir_types::r4::human_name::HumanName;
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::organization::Organization;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::primitives::{Code, Date};
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;
use fhirconnect::engine::context::CallContext;
use fhirconnect::operations::run::Settings;

use crate::patient_summary::Section;
use crate::summary::mappings::{Mappings, MappingsError, slot_of};

/// The extension that says why a required element has no value (FHIR R4
/// `data-absent-reason`).
pub const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// The statement every section's narrative carries.
pub const NOT_EXHAUSTIVE: &str = "This section is not exhaustive: it holds what the members that answered record, mapped to the European exchange format.";

/// An organisation as the registry names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Organisation {
    /// The registry's organisation id.
    pub id: String,
    /// Its display name, when the registry gives one.
    pub name: Option<String>,
    /// Its identifiers, each a system and a value.
    pub identifiers: Vec<(String, String)>,
}

impl Organisation {
    /// Returns the name a narrative shows: the display name, or the id.
    fn shown(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
}

/// One member endpoint the section queries were sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The endpoint id.
    pub endpoint: String,
    /// The organisation that holds the member's data.
    pub organisation: Organisation,
}

/// One composition a member answered a section query with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// Its `OBJECT_VERSION_ID`.
    pub uid: String,
    /// Its template id.
    pub template: String,
    /// The composition, as canonical JSON.
    pub composition: String,
}

/// What one origin answered to one section query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// It answered, with these compositions.
    Answered(Vec<Held>),
    /// It gave no answer: the §11.1 status says why.
    Silent {
        /// The `meta.federation` status of the endpoint.
        status: String,
    },
}

/// What every origin answered to one section query, by origin index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionAnswers {
    /// The section.
    pub section: Section,
    /// The answer of each origin that was asked, by its index in the
    /// origins.
    pub answers: Vec<(usize, Answer)>,
}

/// Who writes the summary: the gateway and its operator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Author {
    /// The gateway's product name.
    pub product: String,
    /// The gateway's version.
    pub version: String,
    /// The operator.
    pub operator: Organisation,
}

/// The document one request asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The absolute FHIR base the entries are named under.
    pub base: String,
    /// The document's identifier, a `urn:uuid:` URN.
    pub identifier: String,
    /// The `id` the patient entry takes.
    pub patient_id: String,
    /// When it is written, a FHIR `instant`.
    pub timestamp: String,
    /// The patient's identifier system.
    pub system: String,
    /// The patient's identifier value.
    pub value: String,
    /// What the identity binding holds of the patient for the header.
    pub header: PatientHeader,
}

/// The document, and what assembling it showed.
#[derive(Debug, Clone)]
pub struct Assembled {
    /// The document `Bundle`.
    pub bundle: Bundle,
    /// The compositions some mapping covered, by origin index and uid.
    pub mapped: BTreeSet<(usize, String)>,
    /// The compositions no mapping covered in some section, by origin index
    /// and uid.
    pub unmapped: BTreeSet<(usize, String)>,
}

/// Why the document cannot be assembled.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AssemblyError {
    /// A section has no EPS slot.
    #[error(transparent)]
    Slot(#[from] MappingsError),
    /// A mapping refused a composition a member answered with.
    #[error("the mapping of a composition of the template {template} was refused")]
    Mapping {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: MappingError,
    },
    /// The document could not be written.
    #[error(transparent)]
    Document(#[from] DocumentError),
    /// A section answer names an origin that was not given.
    #[error("a section answer names origin {index}, which was not given")]
    Origin {
        /// The index.
        index: usize,
    },
    /// The header names the patient by no family name, given name or text,
    /// which the HL7 Europe Patient Summary `Patient` requires (`ips-pat-1`).
    #[error("the patient header holds no name, which the EPS Patient requires (ips-pat-1)")]
    Unnamed,
}
/// Assembles the patient summary `request` asks for from the `sections`
/// the `origins` answered, mapping each composition with `mappings`, as
/// `author` writes it.
///
/// # Errors
///
/// [`AssemblyError::Unnamed`] for a header with no name,
/// [`AssemblyError::Mapping`] when a mapping refuses a composition, which
/// fails the document rather than leaving the composition out, and the
/// other [`AssemblyError`] variants for a document that cannot be written.
pub fn assemble(
    request: &Request,
    author: &Author,
    (origins, sections): (&[Origin], &[SectionAnswers]),
    mappings: &Mappings,
) -> Result<Assembled, AssemblyError> {
    if !request.header.named() {
        return Err(AssemblyError::Unnamed);
    }
    let mut document = Document::new(
        &request.base,
        request.identifier.clone(),
        request.timestamp.clone(),
        String::from("Patient Summary"),
    );
    let subject = document.with_patient(patient(request))?;
    let operator_url = document.full_url("Organization", "operator");
    let device_url =
        document.with_author(&Resource::Device(Box::new(device(author, &operator_url))))?;
    document.with_author(&Resource::Organization(Box::new(organization(
        "operator",
        &author.operator,
    ))))?;
    let mut assembly = Assembly {
        settings: Settings::new(device_url.clone(), request.timestamp.clone()),
        document,
        subject,
        device: device_url,
        members: BTreeMap::new(),
        mapped: BTreeSet::new(),
        unmapped: BTreeSet::new(),
    };
    for answers in sections {
        let section = assembly.section(answers, origins, mappings)?;
        assembly.document.with_section(section)?;
    }
    Ok(Assembled {
        bundle: assembly.document.into_bundle()?,
        mapped: assembly.mapped,
        unmapped: assembly.unmapped,
    })
}

/// The document as it is assembled, and what it took from whom.
struct Assembly {
    document: Document,
    settings: Settings,
    subject: String,
    device: String,
    members: BTreeMap<usize, String>,
    mapped: BTreeSet<(usize, String)>,
    unmapped: BTreeSet<(usize, String)>,
}

impl Assembly {
    /// The section `answers` fill, its entries mapped with `mappings` and
    /// added to the document.
    fn section(
        &mut self,
        answers: &SectionAnswers,
        origins: &[Origin],
        mappings: &Mappings,
    ) -> Result<eehrxf::document::Section, AssemblyError> {
        let slot = slot_of(answers.section)?;
        let mut entries = Vec::new();
        let mut authors = BTreeSet::new();
        let mut silent = Vec::new();
        let mut uncovered: BTreeMap<(usize, String), usize> = BTreeMap::new();
        for (index, answer) in &answers.answers {
            let origin = origins
                .get(*index)
                .ok_or(AssemblyError::Origin { index: *index })?;
            let held = match answer {
                Answer::Answered(held) => held,
                Answer::Silent { status } => {
                    silent.push(format!("{} ({status})", origin.organisation.shown()));
                    continue;
                }
            };
            for composition in held {
                let runs = mappings.of(answers.section, &composition.template);
                if runs.is_empty() {
                    *uncovered
                        .entry((*index, composition.template.clone()))
                        .or_default() += 1;
                    self.unmapped.insert((*index, composition.uid.clone()));
                    continue;
                }
                let member = self.member(*index, origin)?;
                for mapping in runs {
                    entries.extend(self.run(mapping, composition, &member)?);
                }
                authors.insert(member);
                self.mapped.insert((*index, composition.uid.clone()));
            }
        }
        let shown = authors_shown(&authors, &self.members, origins);
        let paragraphs = narrative(entries.len(), (&silent, &uncovered), origins, &shown);
        let mut section = eehrxf::document::Section::new(slot, paragraphs);
        for author in authors {
            section = section.with_author(author);
        }
        if entries.is_empty() {
            let reason = if silent.is_empty() && uncovered.is_empty() {
                EmptyReason::NilKnown
            } else {
                EmptyReason::Unavailable
            };
            section = section.empty_because(reason);
        }
        for entry in entries {
            section = section.with_entry(entry);
        }
        Ok(section)
    }

    /// Runs `mapping` over `composition` for the member at `member`, adds
    /// what it mapped, and returns the `fullUrl` of the mapped resource.
    fn run(
        &mut self,
        mapping: &Mapping,
        composition: &Held,
        member: &str,
    ) -> Result<Option<String>, AssemblyError> {
        let context = CallContext::new()
            .with_patient(reference(&self.subject))
            .with_who(reference(&self.device))
            .with_on_behalf_of(reference(member));
        let answer = mapping
            .to_fhir_with(&composition.composition, &self.settings, context)
            .map_err(|source| AssemblyError::Mapping {
                template: composition.template.clone(),
                source,
            })?;
        let resources: Vec<Resource> = answer
            .bundle()
            .entry
            .iter()
            .filter_map(|entry| entry.resource.clone())
            .collect();
        Ok(self.document.add(&resources)?.into_iter().next())
    }

    /// The `fullUrl` of the `Organization` of the origin at `index`, added
    /// the first time a section takes data from it.
    fn member(&mut self, index: usize, origin: &Origin) -> Result<String, AssemblyError> {
        if let Some(url) = self.members.get(&index) {
            return Ok(url.clone());
        }
        let id = format!("member-{index}");
        let added = self
            .document
            .add(&[Resource::Organization(Box::new(organization(
                &id,
                &origin.organisation,
            )))])?;
        let url = added
            .into_iter()
            .next()
            .ok_or(AssemblyError::Origin { index })?;
        self.members.insert(index, url.clone());
        Ok(url)
    }
}

/// The names of the organisations at `authors`.
fn authors_shown(
    authors: &BTreeSet<String>,
    members: &BTreeMap<usize, String>,
    origins: &[Origin],
) -> Vec<String> {
    members
        .iter()
        .filter(|(_, url)| authors.contains(*url))
        .filter_map(|(index, _)| origins.get(*index))
        .map(|origin| origin.organisation.shown().to_owned())
        .collect()
}

/// The paragraphs of a section's generated narrative.
fn narrative(
    entries: usize,
    (silent, uncovered): (&[String], &BTreeMap<(usize, String), usize>),
    origins: &[Origin],
    contributors: &[String],
) -> Vec<String> {
    let mut paragraphs = Vec::new();
    paragraphs.push(if entries == 0 {
        String::from("No entry.")
    } else {
        format!("{entries} entries, from {}.", contributors.join(", "))
    });
    paragraphs.push(NOT_EXHAUSTIVE.to_owned());
    if !silent.is_empty() {
        paragraphs.push(format!("No answer from: {}.", silent.join(", ")));
    }
    for ((index, template), count) in uncovered {
        let member = origins
            .get(*index)
            .map_or("a member", |origin| origin.organisation.shown());
        paragraphs.push(format!(
            "Not shown: {count} record(s) of the template {template} from {member}, which no mapping covers."
        ));
    }
    paragraphs
}

/// The patient of `request`: the identifier it was asked by, and the
/// header the identity binding holds, each element as the binding gives it.
///
/// An element the binding does not hold is left out, and the birth date,
/// which the EPS `Patient` requires, then carries a `data-absent-reason` of
/// `unknown` (eHN PS Art 10(5)).
fn patient(request: &Request) -> Patient {
    let header = &request.header;
    Patient {
        id: Some(request.patient_id.clone()),
        identifier: vec![Identifier {
            system: Some(request.system.as_str().into()),
            value: Some(request.value.as_str().into()),
            ..Identifier::default()
        }],
        name: header.names.iter().map(human_name).collect(),
        birth_date: Some(header.birth_date.as_deref().map_or_else(
            || Date {
                extension: vec![absent()],
                ..Date::default()
            },
            Date::from,
        )),
        gender: header.gender.as_deref().map(Code::from),
        address: header.addresses.iter().map(address).collect(),
        telecom: header.telecoms.iter().map(contact_point).collect(),
        ..Patient::default()
    }
}

/// The FHIR name `name` is written as.
fn human_name(name: &PersonName) -> HumanName {
    HumanName {
        r#use: name.purpose.as_deref().map(Code::from),
        text: name
            .text
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        family: name
            .family
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        given: strings(&name.given),
        prefix: strings(&name.prefix),
        suffix: strings(&name.suffix),
        ..HumanName::default()
    }
}

/// The FHIR address `postal` is written as.
fn address(postal: &PostalAddress) -> Address {
    Address {
        r#use: postal.purpose.as_deref().map(Code::from),
        text: postal
            .text
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        line: strings(&postal.lines),
        city: postal
            .city
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        district: postal
            .district
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        state: postal
            .state
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        postal_code: postal
            .postal_code
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        country: postal
            .country
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        ..Address::default()
    }
}

/// The FHIR contact point `telecom` is written as.
fn contact_point(telecom: &Telecom) -> ContactPoint {
    ContactPoint {
        system: telecom.system.as_deref().map(Code::from),
        value: telecom
            .value
            .as_deref()
            .map(fhir_types::r4::primitives::String::from),
        r#use: telecom.purpose.as_deref().map(Code::from),
        ..ContactPoint::default()
    }
}

/// The FHIR `string`s `values` are written as.
fn strings(values: &[String]) -> Vec<fhir_types::r4::primitives::String> {
    values.iter().map(|value| value.as_str().into()).collect()
}

/// The `data-absent-reason` extension stating `unknown`.
fn absent() -> Extension {
    Extension {
        url: DATA_ABSENT_REASON.to_owned(),
        value: Some(ExtensionValue::Code("unknown".into())),
        ..Extension::default()
    }
}

/// The gateway's `Device`, owned by the operator at `operator`.
fn device(author: &Author, operator: &str) -> Device {
    Device {
        id: Some(String::from("gateway")),
        device_name: vec![DeviceDeviceName {
            name: author.product.as_str().into(),
            r#type: "manufacturer-name".into(),
            ..DeviceDeviceName::default()
        }],
        version: vec![DeviceVersion {
            value: author.version.as_str().into(),
            ..DeviceVersion::default()
        }],
        // NOTE: eHN PS A.1.4.3: the nature of the summary is carried by its author device.
        r#type: Some(CodeableConcept {
            text: Some("Software that generates the patient summary automatically".into()),
            ..CodeableConcept::default()
        }),
        owner: Some(reference(operator)),
        ..Device::default()
    }
}

/// The `Organization` of `organisation`, under the document id `id`.
fn organization(id: &str, organisation: &Organisation) -> Organization {
    Organization {
        id: Some(id.to_owned()),
        name: Some(organisation.shown().into()),
        identifier: organisation
            .identifiers
            .iter()
            .map(|(system, value)| Identifier {
                system: Some(system.as_str().into()),
                value: Some(value.as_str().into()),
                ..Identifier::default()
            })
            .collect(),
        ..Organization::default()
    }
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}
