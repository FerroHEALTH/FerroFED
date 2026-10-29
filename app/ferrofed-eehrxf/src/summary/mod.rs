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
//! - the `OperationOutcome` a lossy run returns is counted in that run's
//!   transformation report and never becomes an entry of the document;
//! - a term mapping a run wrote as no coding, because its `match` states a
//!   broader, a narrower or an unknown relation, is written into the
//!   section's narrative, naming the member it came from (eHN PS Art 10(3),
//!   [`eehrxf::terminology::withheld`]);
//! - with a terminology server configured, each code of a mapped resource
//!   in no preferred system gains every `equivalent` or `equal`
//!   translation into a preferred system after the original coding, and a
//!   server that refuses or does not answer fails the document
//!   ([`eehrxf::terminology::translation`]); with none, the code is
//!   transferred as recorded (eHN PS Art 10(7));
//! - a member's data are listed as mapped, beside every other member's,
//!   never merged with them, and two members' medication entries that name
//!   the same medicine, strength and route are marked in the section's
//!   narrative as a probable duplicate, naming both members ([`duplicates`],
//!   hazard I-02 of the clinical safety risk file);
//! - each section's narrative is generated and says that it is not
//!   exhaustive, names every member that gave no answer, and counts every
//!   composition no mapping covers, by member and template, which is
//!   reported and never dropped;
//! - a member that serves its own patient summary is left out, and each
//!   section names it as one that does, or as one that gave no answer when
//!   its own endpoint did not answer;
//! - a section with no entry is `nilknown` only when every member answered,
//!   none holds content for it and none serves its own summary, and
//!   `unavailable` otherwise;
//! - the medical alert and the functional status, which no openEHR content
//!   is named to feed ([`crate::patient_summary::UNSELECTED`]), are written
//!   in every summary as `unavailable`, with a narrative saying the gateway
//!   does not supply them, so no reader takes their absence for "none"
//!   (hazard I-03 of the clinical safety risk file).
//!
//! No specification governs how a federation authors a summary: these are
//! our own design, against the eHealth Network guideline's header elements
//! and the EPS composition's author, section and empty-reason elements.

pub mod duplicates;
pub mod mappings;
pub mod repeated;

use std::collections::{BTreeMap, BTreeSet};

use eehrxf::category::Category;
use eehrxf::document::{Document, DocumentError, EmptyReason};
use eehrxf::mapping::{Mapping, MappingError};
use eehrxf::terminology::Terminology;
use eehrxf::terminology::ehn;
use eehrxf::terminology::translation::{TranslationError, Translator};
use eehrxf::terminology::withheld::Withheld;
use eehrxf::transformation::{Direction, Pin, Report, ReportError, Tally};
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
use fhirconnect::engine::traverse::error::EngineError;
use fhirconnect::operations::error::OperationError;
use fhirconnect::operations::run::Settings;

use crate::patient_summary::{Reason, Section, UNSELECTED};
use crate::summary::mappings::{Mappings, MappingsError, slot_of};
use crate::transformation::preferred;

/// The extension that says why a required element has no value (FHIR R4
/// `data-absent-reason`).
pub const DATA_ABSENT_REASON: &str = "http://hl7.org/fhir/StructureDefinition/data-absent-reason";

/// The statement every section's narrative carries.
pub const NOT_EXHAUSTIVE: &str = "This section is not exhaustive: it holds what the members that answered record, mapped to the European exchange format.";

/// One narrative line for each composition `listed` names as listed once,
/// naming the organisations of the kept and the dropped copies.
fn listed_once(listed: &[ListedOnce], origins: &[Origin]) -> Vec<String> {
    let shown = |index: usize| {
        origins.get(index).map_or_else(
            || index.to_string(),
            |origin| origin.organisation.shown().to_owned(),
        )
    };
    listed
        .iter()
        .map(|once| {
            let dropped: Vec<String> = once.dropped.iter().map(|index| shown(*index)).collect();
            format!(
                "A record {} also holds as a copy of the one from {} is listed once, from {}.",
                dropped.join(", "),
                shown(once.kept),
                shown(once.kept)
            )
        })
        .collect()
}

/// The statement the narrative of a section the gateway does not supply
/// carries.
pub const NOT_SUPPLIED: &str = "The gateway does not supply this section: no openEHR content is named to feed it, so the section says nothing about whether the patient has any. Ask the care providers who treat the patient.";

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

/// A member that serves the patient summary itself, at its own endpoint:
/// its data are left out of the gateway's document, which says so in every
/// section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elsewhere {
    /// The member's own endpoint id.
    pub endpoint: String,
    /// The organisation that holds the member's data.
    pub organisation: Organisation,
    /// What its own endpoint answered.
    pub answer: Covered,
}

/// What a member's own endpoint answered the request for its summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Covered {
    /// Its own document.
    Document,
    /// That it holds no summary of the patient.
    Absent,
    /// Nothing: the §11.1 status says why.
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
    /// The compositions two or more origins answered with under one full
    /// version id, each listed once, from the origin kept.
    pub listed_once: Vec<ListedOnce>,
}

/// A composition two or more origins answered with under one full
/// `OBJECT_VERSION_ID` (Federation Tier §10.3, an imported copy), listed once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedOnce {
    /// The origin whose copy the section lists, by its index in the origins.
    pub kept: usize,
    /// The origins whose copy the section leaves out, by index.
    pub dropped: Vec<usize>,
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
    /// What each transformation did: one report per context mapping that
    /// ran, and one per template no mapping covers, in template order.
    pub transformations: Vec<Report>,
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
    /// A composition a member answered with is no JSON.
    #[error("a composition of the template {template} is no JSON")]
    Composition {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: serde_json::Error,
    },
    /// The document could not be written.
    #[error(transparent)]
    Document(#[from] DocumentError),
    /// A crosswalk gap names no EPS section slot.
    #[error("the gap {element} names no section of the patient summary composition")]
    Gap {
        /// The Xt-EHR element path of the gap.
        element: &'static str,
    },
    /// A section answer names an origin that was not given.
    #[error("a section answer names origin {index}, which was not given")]
    Origin {
        /// The index.
        index: usize,
    },
    /// The transformation of a composition a member answered with could not
    /// be counted.
    #[error("the transformation of a composition of the template {template} could not be counted")]
    Report {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: ReportError,
    },
    /// The codes of a mapped resource could not be translated, because the
    /// terminology server refused or did not answer, which fails the
    /// document rather than leaving a code untranslated.
    #[error(
        "the codes mapped from a composition of the template {template} could not be translated"
    )]
    Translation {
        /// The composition's template id.
        template: String,
        /// Why.
        #[source]
        source: TranslationError,
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
/// Each code in no preferred system is translated through `terminology`
/// when it names a server.
///
/// Every section names each member `elsewhere` lists as serving its own
/// summary, whose data it leaves out.
///
/// # Errors
///
/// [`AssemblyError::Unnamed`] for a header with no name,
/// [`AssemblyError::Mapping`] when a mapping refuses a composition, which
/// fails the document rather than leaving the composition out,
/// [`AssemblyError::Translation`] when the terminology server refuses or
/// does not answer a translation, and the other [`AssemblyError`] variants
/// for a document that cannot be written.
pub async fn assemble(
    request: &Request,
    author: &Author,
    (origins, sections, elsewhere): (&[Origin], &[SectionAnswers], &[Elsewhere]),
    (mappings, terminology): (&Mappings, &Terminology),
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
        reports: BTreeMap::new(),
    };
    let mut translator = Translator::new(terminology, &ehn::PATIENT_SUMMARY);
    for answers in sections {
        let section = assembly
            .section(answers, (origins, elsewhere), (mappings, &mut translator))
            .await?;
        assembly.document.with_section(section)?;
    }
    for slot in not_supplied()? {
        assembly.document.with_section(
            eehrxf::document::Section::new(slot, vec![String::from(NOT_SUPPLIED)])
                .empty_because(EmptyReason::Unavailable),
        )?;
    }
    Ok(Assembled {
        bundle: assembly.document.into_bundle()?,
        mapped: assembly.mapped,
        unmapped: assembly.unmapped,
        transformations: assembly.reports.into_values().collect(),
    })
}

/// The narrative line of the elements one run dropped, by path, never by
/// value, so a loss is never silent in the document (hazard I-03 of the
/// clinical safety risk file); `None` when it dropped none.
fn dropped_line(dropped: &Tally) -> Option<String> {
    if dropped.count() == 0 {
        return None;
    }
    let paths: Vec<&str> = dropped.paths().keys().map(String::as_str).collect();
    let named = if paths.is_empty() {
        String::new()
    } else {
        format!(" ({})", paths.join(", "))
    };
    Some(format!(
        "{} element(s) of a record could not be carried into this section{named}, so the record holds more than the section shows",
        dropped.count()
    ))
}

/// What one mapping run added: the `fullUrl` of the mapped resource, the
/// narrative line of each term mapping it wrote as no coding, and the
/// probable-duplicate key of its medication entry.
type Run = (Option<String>, Vec<String>, Option<duplicates::Key>);

/// What running one mapping over one composition came to.
enum Outcome {
    /// The context's condition does not hold over the composition.
    NotApplicable,
    /// The composition holds the context's start archetype `count` times,
    /// which one resource cannot carry apart, so the context was not run.
    Repeated {
        /// The start archetype.
        archetype: String,
        /// How many entries of it the composition holds.
        count: usize,
    },
    /// The context ran.
    Ran(Run),
}

/// Whether `refused` is the engine's answer that the context's own condition
/// does not hold over the composition (FHIRconnect 1.0.0
/// `basics/preprocessor.adoc`), which leaves the composition to the section's
/// other mappings.
fn not_applicable(refused: &MappingError) -> bool {
    let MappingError::Run { source } = refused else {
        return false;
    };
    let OperationError::Mapping { source } = &**source else {
        return false;
    };
    matches!(**source, EngineError::NotApplicable { .. })
}

/// The slots of the sections no openEHR content is named to feed, the gaps
/// of [`UNSELECTED`], which every summary writes as not supplied.
fn not_supplied() -> Result<Vec<&'static eehrxf::document::Slot>, AssemblyError> {
    UNSELECTED
        .iter()
        .filter(|unselected| matches!(unselected.reason, Reason::Gap { .. }))
        .map(|unselected| {
            eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY
                .row(unselected.element)
                .and_then(|row| row.eps.strip_prefix("Composition.section:"))
                .and_then(eehrxf::document::slot)
                .ok_or(AssemblyError::Gap {
                    element: unselected.element,
                })
        })
        .collect()
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
    reports: BTreeMap<(String, Option<String>), Report>,
}

impl Assembly {
    /// The section `answers` fill, its entries mapped with `mappings`,
    /// translated by `translator` and added to the document, naming each
    /// member `elsewhere` lists.
    async fn section(
        &mut self,
        answers: &SectionAnswers,
        (origins, elsewhere): (&[Origin], &[Elsewhere]),
        (mappings, translator): (&Mappings, &mut Translator<'_>),
    ) -> Result<eehrxf::document::Section, AssemblyError> {
        let slot = slot_of(answers.section)?;
        let mut entries = Vec::new();
        let mut authors = BTreeSet::new();
        let mut silent: Vec<String> = elsewhere
            .iter()
            .filter_map(|member| match &member.answer {
                Covered::Silent { status } => {
                    Some(format!("{} ({status})", member.organisation.shown()))
                }
                Covered::Document | Covered::Absent => None,
            })
            .collect();
        let mut uncovered: BTreeMap<(usize, String), usize> = BTreeMap::new();
        let mut withheld = Vec::new();
        let mut keys = Vec::new();
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
                    self.uncovered(&composition.template);
                    continue;
                }
                let member = self.member(*index, origin)?;
                let mut applied = false;
                for mapping in runs {
                    let (entry, lines, key) = match self
                        .run(mapping, composition, &member, translator)
                        .await?
                    {
                        Outcome::Ran(ran) => ran,
                        Outcome::NotApplicable => continue,
                        Outcome::Repeated { archetype, count } => {
                            withheld.push(format!(
                                    "A record from {} holds {count} entries of {archetype}, which the mapping cannot carry apart, so this section leaves the record out.",
                                    origin.organisation.shown()
                                ));
                            continue;
                        }
                    };
                    applied = true;
                    entries.extend(entry);
                    keys.extend(key.map(|key| (origin.organisation.shown().to_owned(), key)));
                    withheld.extend(
                        lines
                            .into_iter()
                            .map(|line| format!("{line}, from {}.", origin.organisation.shown())),
                    );
                }
                if !applied {
                    // NOTE: FHIRconnect 1.0.0 `basics/preprocessor.adoc`: a context whose condition
                    // does not hold is not run, so a composition no context applies to is reported.
                    *uncovered
                        .entry((*index, composition.template.clone()))
                        .or_default() += 1;
                    self.unmapped.insert((*index, composition.uid.clone()));
                    self.uncovered(&composition.template);
                    continue;
                }
                authors.insert(member);
                self.mapped.insert((*index, composition.uid.clone()));
            }
        }
        let shown = authors_shown(&authors, &self.members, origins);
        let mut paragraphs = narrative(entries.len(), (&silent, &uncovered), origins, &shown);
        paragraphs.extend(withheld);
        paragraphs.extend(duplicates::lines(&keys));
        paragraphs.extend(listed_once(&answers.listed_once, origins));
        paragraphs.extend(covered_elsewhere(elsewhere));
        let mut section = eehrxf::document::Section::new(slot, paragraphs);
        for author in authors {
            section = section.with_author(author);
        }
        if entries.is_empty() {
            // NOTE: no specification governs this: our own design; a member that serves its own
            // summary was not mapped here, so the section cannot know it holds nothing.
            let reason = if silent.is_empty() && uncovered.is_empty() && elsewhere.is_empty() {
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

    /// Runs `mapping` over `composition` for the member at `member`,
    /// translates what it mapped with `translator`, adds it, and
    /// returns the `fullUrl` of the mapped resource and the narrative line
    /// of each term mapping the run wrote as no coding.
    async fn run(
        &mut self,
        mapping: &Mapping,
        composition: &Held,
        member: &str,
        translator: &mut Translator<'_>,
    ) -> Result<Outcome, AssemblyError> {
        if let Some(archetype) = mapping.start_archetype(&composition.template) {
            let count =
                repeated::occurrences(&composition.composition, &archetype).map_err(|source| {
                    AssemblyError::Composition {
                        template: composition.template.clone(),
                        source,
                    }
                })?;
            if count > 1 {
                return Ok(Outcome::Repeated { archetype, count });
            }
        }
        let context = CallContext::new()
            .with_patient(reference(&self.subject))
            .with_who(reference(&self.device))
            .with_on_behalf_of(reference(member));
        let (answer, warnings) = match mapping.to_fhir_with_warnings(
            &composition.composition,
            &self.settings,
            context,
        ) {
            Ok(ran) => ran,
            Err(refused) if not_applicable(&refused) => return Ok(Outcome::NotApplicable),
            Err(source) => {
                return Err(AssemblyError::Mapping {
                    template: composition.template.clone(),
                    source,
                });
            }
        };
        let withheld = warnings
            .iter()
            .filter_map(Withheld::of)
            .map(|mapping| mapping.narrative())
            .collect();
        let mapped: Vec<Resource> = answer
            .bundle()
            .entry
            .iter()
            .filter_map(|entry| entry.resource.clone())
            .collect();
        let mut resources = Vec::with_capacity(mapped.len());
        for resource in mapped {
            resources.push(match resource {
                Resource::Provenance(_) | Resource::OperationOutcome(_) => resource,
                other => translator
                    .translate(&other, preferred)
                    .await
                    .map_err(|source| AssemblyError::Translation {
                        template: composition.template.clone(),
                        source,
                    })?,
            });
        }
        let dropped = self.transformed(mapping, &composition.template, &resources)?;
        let mut withheld: Vec<String> = withheld;
        withheld.extend(dropped_line(&dropped));
        // NOTE: HL7 Europe Patient Summary 1.0.0-ballot `bundle-eu-eps` slices `Bundle.entry` by the
        // document's content and names no `OperationOutcome`; the run's losses go to its report.
        let entries: Vec<Resource> = resources
            .into_iter()
            .filter(|resource| !matches!(resource, Resource::OperationOutcome(_)))
            .collect();
        let key = duplicates::Key::of(&entries);
        Ok(Outcome::Ran((
            self.document.add(&entries)?.into_iter().next(),
            withheld,
            key,
        )))
    }

    /// Counts what `mapping` did to a composition of `template`, which it
    /// mapped to `resources`, into the report of its context.
    fn transformed(
        &mut self,
        mapping: &Mapping,
        template: &str,
        resources: &[Resource],
    ) -> Result<Tally, AssemblyError> {
        // NOTE: the mappings are keyed by the template their context maps, so a context
        // for `template` exists; the template alone is named should it not.
        let pin = mapping.pin(template).unwrap_or_else(|| Pin {
            template: template.to_owned(),
            ..Pin::default()
        });
        let key = (template.to_owned(), pin.context.clone());
        let mut report = Report::new(Direction::Provided, Category::PatientSummary, pin);
        for resource in resources {
            match resource {
                Resource::Provenance(_) => {}
                Resource::OperationOutcome(outcome) => report.count_losses(outcome),
                other => {
                    report.count_coded(other, preferred).map_err(|source| {
                        AssemblyError::Report {
                            template: template.to_owned(),
                            source,
                        }
                    })?;
                }
            }
        }
        let dropped = report.dropped().clone();
        self.absorb(key, report);
        Ok(dropped)
    }

    /// Counts one composition of `template` no mapping covers as dropped
    /// whole, in the report of that template.
    fn uncovered(&mut self, template: &str) {
        let pin = Pin {
            template: template.to_owned(),
            ..Pin::default()
        };
        let mut report = Report::new(Direction::Provided, Category::PatientSummary, pin);
        report.count_uncovered();
        self.absorb((template.to_owned(), None), report);
    }

    /// Adds `report` to the report held under `key`, or holds it there.
    fn absorb(&mut self, key: (String, Option<String>), report: Report) {
        match self.reports.get_mut(&key) {
            Some(held) => held.absorb(&report),
            None => {
                self.reports.insert(key, report);
            }
        }
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

/// The paragraphs that name each member of `elsewhere` that answered as
/// serving its own patient summary, which holds its data for the section.
fn covered_elsewhere(elsewhere: &[Elsewhere]) -> Vec<String> {
    elsewhere
        .iter()
        .filter_map(|member| {
            let shown = member.organisation.shown();
            match member.answer {
                Covered::Document => Some(format!(
                    "Not mapped here: the records of {shown}, which serves its own patient summary."
                )),
                Covered::Absent => Some(format!(
                    "Not mapped here: the records of {shown}, which serves its own patient summary and holds none for this patient."
                )),
                Covered::Silent { .. } => None,
            }
        })
        .collect()
}

/// The patient of `request`: the identifier it was asked by, and the
/// header the identity binding holds, each element as the binding gives it.
///
/// An element the binding does not hold is left out, and the birth date,
/// which the EPS `Patient` requires, then carries a `data-absent-reason` of
/// `unknown` (eHN PS Art 10(5)).
fn patient(request: &Request) -> Patient {
    header_patient(
        &request.patient_id,
        (&request.system, &request.value),
        &request.header,
    )
}

/// Returns the `Patient` with the id `id` that the identity binding's
/// `header` describes, named by the identifier `system`|`value` it was asked
/// by, each element as the binding gives it.
///
/// An element the binding does not hold is left out, and the birth date
/// then carries a `data-absent-reason` of `unknown` (eHN PS Art 10(5)).
#[must_use]
pub fn header_patient(id: &str, (system, value): (&str, &str), header: &PatientHeader) -> Patient {
    Patient {
        id: Some(id.to_owned()),
        identifier: vec![Identifier {
            system: Some(system.into()),
            value: Some(value.into()),
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
