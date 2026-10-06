// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient summary document in the HL7 Europe Patient Summary form
//! (features `patient-summary` and `fhir-r4`).
//!
//! A FHIR document is a `Bundle` of type `document` whose first entry is a
//! `Composition` and whose other entries are every resource the composition
//! references (<https://hl7.org/fhir/R4/documents.html>). A [`Document`]
//! collects the parts, names each entry by a `fullUrl` of the form `{base}/{type}/{id}` under the
//! base it is given, and writes the `Bundle` the HL7 Europe Patient Summary
//! `bundle-eu-eps` profile, 1.0.0-ballot, constrains, with the
//! `composition-eu-eps` composition. The sections are the profile's section
//! slices ([`SLOTS`]), each with its LOINC code, its title, a generated
//! narrative, its entries and, when it has none, an empty reason. What feeds
//! each section is the caller's: this module decides no clinical content.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use fhir_types::codec::{Json, Object, Path, Value};
use fhir_types::r4::bundle::{Bundle, BundleEntry};
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::composition::{Composition, CompositionSection};
use fhir_types::r4::identifier::Identifier;
use fhir_types::r4::meta::Meta;
use fhir_types::r4::narrative::Narrative;
use fhir_types::r4::patient::Patient;
use fhir_types::r4::reference::Reference;
use fhir_types::r4::resource::Resource;

/// The canonical URL of the HL7 Europe Patient Summary `Bundle` profile.
pub const EPS_BUNDLE: &str = "http://hl7.eu/fhir/eps/StructureDefinition/bundle-eu-eps";

/// The canonical URL of the HL7 Europe Patient Summary `Patient` profile.
pub const EPS_PATIENT: &str = "http://hl7.eu/fhir/eps/StructureDefinition/patient-eu-eps";

/// The LOINC code system, which codes the document type and every section.
pub const LOINC: &str = "http://loinc.org";

/// The LOINC code the `composition-eu-eps` profile fixes as
/// `Composition.type`, a patient summary document.
pub const PATIENT_SUMMARY: &str = "60591-5";

/// The code system of a section's empty reason, FHIR R4 `list-empty-reason`
/// (<https://hl7.org/fhir/R4/valueset-list-empty-reason.html>).
pub const EMPTY_REASON: &str = "http://terminology.hl7.org/CodeSystem/list-empty-reason";

/// The identifier system of a document identifier written as a URN, RFC 3986
/// (<https://hl7.org/fhir/R4/identifier-registry.html>).
pub const URN_SYSTEM: &str = "urn:ietf:rfc:3986";

/// One section slice of the `composition-eu-eps` profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Slot {
    /// The slice name, such as `sectionAllergies`.
    pub slice: &'static str,
    /// The LOINC code the slice's `code` pattern fixes.
    pub code: &'static str,
    /// The section title the document writes.
    pub title: &'static str,
}

/// Builds the slot of the slice `$slice`.
macro_rules! slot {
    ($slice:literal, $code:literal, $title:literal) => {
        Slot {
            slice: $slice,
            code: $code,
            title: $title,
        }
    };
}

/// Every section slice of the `composition-eu-eps` profile, in the profile's
/// order.
pub const SLOTS: [Slot; 17] = [
    slot!("sectionProblems", "11450-4", "Problems"),
    slot!("sectionAllergies", "48765-2", "Allergies and Intolerances"),
    slot!("sectionMedications", "10160-0", "Medication Summary"),
    slot!("sectionImmunizations", "11369-6", "Immunizations"),
    slot!("sectionResults", "30954-2", "Results"),
    slot!("sectionProceduresHx", "47519-4", "History of Procedures"),
    slot!("sectionMedicalDevices", "46264-8", "Medical Devices"),
    slot!("sectionAdvanceDirectives", "42348-3", "Advance Directives"),
    slot!("sectionAlert", "104605-1", "Alerts"),
    slot!("sectionFunctionalStatus", "47420-5", "Functional Status"),
    slot!("sectionPregnancyHx", "10162-6", "History of Pregnancy"),
    slot!("sectionPatientStory", "81338-6", "Patient Story"),
    slot!("sectionPlanOfCare", "18776-5", "Plan of Care"),
    slot!("sectionSocialHistory", "29762-2", "Social History"),
    slot!("sectionVitalSigns", "8716-3", "Vital Signs"),
    slot!("sectionTravelHx", "10182-4", "Travel History"),
    slot!("sectionPatientHx", "11329-0", "Patient History"),
];

/// Returns the slot of the section slice `slice`, or `None` for a name the
/// profile does not slice.
#[must_use]
pub fn slot(slice: &str) -> Option<&'static Slot> {
    SLOTS.iter().find(|slot| slot.slice == slice)
}

/// Why a section has no entry (FHIR R4 `list-empty-reason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum EmptyReason {
    /// `nilknown`: there is nothing to record.
    NilKnown,
    /// `unavailable`: the information is not available.
    Unavailable,
}

impl EmptyReason {
    /// Returns the code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NilKnown => "nilknown",
            Self::Unavailable => "unavailable",
        }
    }
}

/// One section of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    slot: &'static Slot,
    paragraphs: Vec<String>,
    entries: Vec<String>,
    authors: Vec<String>,
    empty: Option<EmptyReason>,
}

impl Section {
    /// Creates the section of `slot`, its generated narrative the
    /// `paragraphs`, with no entry and no author.
    #[must_use]
    pub fn new(slot: &'static Slot, paragraphs: Vec<String>) -> Self {
        Self {
            slot,
            paragraphs,
            entries: Vec::new(),
            authors: Vec::new(),
            empty: None,
        }
    }

    /// Returns the slot.
    #[must_use]
    pub const fn slot(&self) -> &'static Slot {
        self.slot
    }

    /// Returns this section with the entry `full_url`, the `fullUrl` of a
    /// resource the document holds ([`Document::add`]).
    #[must_use]
    pub fn with_entry(mut self, full_url: String) -> Self {
        self.entries.push(full_url);
        self
    }

    /// Returns this section with the author `full_url`, the `fullUrl` of a
    /// resource the document holds.
    #[must_use]
    pub fn with_author(mut self, full_url: String) -> Self {
        self.authors.push(full_url);
        self
    }

    /// Returns this section with `reason` as the reason it has no entry,
    /// which the document writes only while it has none (`cmp-2`).
    #[must_use]
    pub const fn empty_because(mut self, reason: EmptyReason) -> Self {
        self.empty = Some(reason);
        self
    }
}

/// Why a document cannot be written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DocumentError {
    /// A resource carries no `id`, so it has no `fullUrl`.
    #[error("a {resource} resource carries no id")]
    NoId {
        /// The resource type.
        resource: String,
    },
    /// A resource could not be encoded or decoded as FHIR JSON.
    #[error("a resource could not be encoded as FHIR JSON")]
    Codec(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A section names an entry or an author the document does not hold.
    #[error("the section {slice} references a resource the document does not hold")]
    Dangling {
        /// The section slice.
        slice: &'static str,
    },
    /// Two sections share one slice.
    #[error("the section {slice} is given twice")]
    Repeated {
        /// The section slice.
        slice: &'static str,
    },
}

/// The patient summary document as it is assembled.
#[derive(Debug, Clone)]
pub struct Document {
    base: String,
    identifier: String,
    timestamp: String,
    title: String,
    patient: Option<String>,
    authors: Vec<String>,
    entries: Vec<(String, Resource)>,
    sections: Vec<Section>,
}

impl Document {
    /// Creates the document named `identifier`, a URN, written at
    /// `timestamp`, a FHIR `instant`, titled `title`, its entries named under
    /// the absolute FHIR base `base`.
    #[must_use]
    pub fn new(base: &str, identifier: String, timestamp: String, title: String) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            identifier,
            timestamp,
            title,
            patient: None,
            authors: Vec::new(),
            entries: Vec::new(),
            sections: Vec::new(),
        }
    }

    /// Returns the `fullUrl` of the resource `resource_type`/`id`
    /// under the document's base.
    #[must_use]
    pub fn full_url(&self, resource_type: &str, id: &str) -> String {
        format!("{}/{resource_type}/{id}", self.base)
    }

    /// Adds `patient`, under its `id`, as the document's subject, claiming
    /// the `patient-eu-eps` profile, and returns its `fullUrl`.
    ///
    /// # Errors
    ///
    /// [`DocumentError::NoId`] for a patient with no `id`, and
    /// [`DocumentError::Codec`] when it cannot be encoded.
    pub fn with_patient(&mut self, mut patient: Patient) -> Result<String, DocumentError> {
        let meta = patient.meta.get_or_insert_with(Meta::default);
        meta.profile.push(EPS_PATIENT.into());
        let added = self.add(&[Resource::Patient(Box::new(patient))])?;
        let full_url = added
            .into_iter()
            .next()
            .ok_or_else(|| DocumentError::NoId {
                resource: String::from("Patient"),
            })?;
        self.patient = Some(full_url.clone());
        Ok(full_url)
    }

    /// Adds `authored` and names it an author of the composition, returning
    /// its `fullUrl`.
    ///
    /// # Errors
    ///
    /// The errors of [`Document::add`].
    pub fn with_author(&mut self, authored: &Resource) -> Result<String, DocumentError> {
        let full_url = self
            .add(std::slice::from_ref(authored))?
            .into_iter()
            .next()
            .ok_or_else(|| DocumentError::NoId {
                resource: String::from("author"),
            })?;
        self.authors.push(full_url.clone());
        Ok(full_url)
    }

    /// Adds `section`.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Repeated`] for a slice the document already holds.
    pub fn with_section(&mut self, section: Section) -> Result<(), DocumentError> {
        if self
            .sections
            .iter()
            .any(|held| held.slot.slice == section.slot.slice)
        {
            return Err(DocumentError::Repeated {
                slice: section.slot.slice,
            });
        }
        self.sections.push(section);
        Ok(())
    }

    /// Adds `resources`, the output of one mapping run that reference one
    /// another by `Type/id`, and returns the `fullUrl` of each, in order.
    ///
    /// A resource with no `id` takes the next free `{type}-{n}` id, and one
    /// whose `Type/id` the document already holds takes `{id}-{n}`, every
    /// reference among `resources` to the renamed one following it, so two
    /// runs over the same composition never share a `fullUrl` and none is
    /// merged into the other (FHIR R4 `bdl-7`).
    ///
    /// # Errors
    ///
    /// [`DocumentError::Codec`] when a resource cannot be encoded or read
    /// back.
    pub fn add(&mut self, resources: &[Resource]) -> Result<Vec<String>, DocumentError> {
        let mut objects = Vec::with_capacity(resources.len());
        for resource in resources {
            objects.push(Json::to_json(resource).map_err(codec)?);
        }
        let mut taken: BTreeSet<String> = self.entries.iter().map(|(url, _)| url.clone()).collect();
        let mut renamed: Vec<(String, String)> = Vec::new();
        let mut keys = Vec::with_capacity(objects.len());
        for object in &mut objects {
            let kind = text(object, "resourceType")
                .unwrap_or("Resource")
                .to_owned();
            let held = text(object, "id").map(str::to_owned);
            let stem = held.clone().unwrap_or_else(|| kind.to_ascii_lowercase());
            let id = free(
                &mut taken,
                |id| self.full_url(&kind, id),
                &stem,
                held.is_some(),
            );
            if let Some(held) = held.filter(|held| *held != id) {
                renamed.push((format!("{kind}/{held}"), format!("{kind}/{id}")));
            }
            object.insert(String::from("id"), Value::String(id.clone()));
            keys.push(self.full_url(&kind, &id));
        }
        let mut added = Vec::with_capacity(objects.len());
        for (mut object, full_url) in objects.into_iter().zip(keys) {
            if !renamed.is_empty() {
                for value in object.values_mut() {
                    rename(value, &renamed);
                }
            }
            let resource =
                Resource::from_json(&object, &mut Path::root("Resource")).map_err(codec)?;
            self.entries.push((full_url.clone(), resource));
            added.push(full_url);
        }
        Ok(added)
    }

    /// Writes the document `Bundle`: the composition first, then every
    /// resource in the order it was added.
    ///
    /// The composition is `final`, of the LOINC type [`PATIENT_SUMMARY`],
    /// about the patient, by every author, with each section in the order
    /// it was added. A section with no entry carries its empty reason.
    ///
    /// # Errors
    ///
    /// [`DocumentError::Dangling`] when a section names a `fullUrl` the
    /// document does not hold, or no patient was added.
    pub fn into_bundle(self) -> Result<Bundle, DocumentError> {
        let held: BTreeSet<&str> = self.entries.iter().map(|(url, _)| url.as_str()).collect();
        let mut sections = Vec::with_capacity(self.sections.len());
        for section in &self.sections {
            if section
                .entries
                .iter()
                .chain(&section.authors)
                .any(|url| !held.contains(url.as_str()))
            {
                return Err(DocumentError::Dangling {
                    slice: section.slot.slice,
                });
            }
            sections.push(composition_section(section));
        }
        let subject = self
            .patient
            .clone()
            .ok_or(DocumentError::Dangling { slice: "subject" })?;
        let composition = Composition {
            id: Some(
                self.identifier
                    .rsplit(':')
                    .next()
                    .unwrap_or("composition")
                    .to_owned(),
            ),
            identifier: Some(Identifier {
                system: Some(URN_SYSTEM.into()),
                value: Some(self.identifier.as_str().into()),
                ..Identifier::default()
            }),
            status: "final".into(),
            r#type: CodeableConcept {
                coding: vec![loinc(PATIENT_SUMMARY)],
                ..CodeableConcept::default()
            },
            subject: Some(reference(&subject)),
            date: self.timestamp.as_str().into(),
            author: self
                .authors
                .iter()
                .map(|author| reference(author))
                .collect(),
            title: self.title.as_str().into(),
            section: sections,
            ..Composition::default()
        };
        let composition_url = self.full_url(
            "Composition",
            composition.id.as_deref().unwrap_or("composition"),
        );
        let mut entry = vec![BundleEntry {
            full_url: Some(composition_url.into()),
            resource: Some(Resource::Composition(Box::new(composition))),
            ..BundleEntry::default()
        }];
        entry.extend(
            self.entries
                .into_iter()
                .map(|(full_url, resource)| BundleEntry {
                    full_url: Some(full_url.into()),
                    resource: Some(resource),
                    ..BundleEntry::default()
                }),
        );
        Ok(Bundle {
            meta: Some(Meta {
                profile: vec![EPS_BUNDLE.into()],
                ..Meta::default()
            }),
            identifier: Some(Identifier {
                system: Some(URN_SYSTEM.into()),
                value: Some(self.identifier.into()),
                ..Identifier::default()
            }),
            r#type: "document".into(),
            timestamp: Some(self.timestamp.into()),
            entry,
            ..Bundle::default()
        })
    }
}

/// The composition section `section` writes.
fn composition_section(section: &Section) -> CompositionSection {
    CompositionSection {
        title: Some(section.slot.title.into()),
        code: Some(CodeableConcept {
            coding: vec![loinc(section.slot.code)],
            ..CodeableConcept::default()
        }),
        author: section.authors.iter().map(|url| reference(url)).collect(),
        text: Some(narrative(section.slot.title, &section.paragraphs)),
        entry: section.entries.iter().map(|url| reference(url)).collect(),
        // NOTE: FHIR R4 Composition cmp-2: an empty reason stands only beside no entry.
        empty_reason: section
            .empty
            .filter(|_| section.entries.is_empty())
            .map(|reason| CodeableConcept {
                coding: vec![Coding {
                    system: Some(EMPTY_REASON.into()),
                    code: Some(reason.code().into()),
                    ..Coding::default()
                }],
                ..CodeableConcept::default()
            }),
        ..CompositionSection::default()
    }
}

/// The generated narrative headed `title`, one XHTML paragraph per entry of
/// `paragraphs`, every character escaped (FHIR R4 Narrative, `txt-1`).
#[must_use]
pub fn narrative(title: &str, paragraphs: &[String]) -> Narrative {
    let mut div = String::from(r#"<div xmlns="http://www.w3.org/1999/xhtml">"#);
    // NOTE: writing to a String cannot fail, so each result is dropped.
    let _written: std::fmt::Result = write!(div, "<h3>{}</h3>", escaped(title));
    for paragraph in paragraphs {
        let _written: std::fmt::Result = write!(div, "<p>{}</p>", escaped(paragraph));
    }
    div.push_str("</div>");
    Narrative {
        status: "generated".into(),
        div: div.into(),
        ..Narrative::default()
    }
}

/// `text` with the five XML special characters escaped.
fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// The LOINC coding of `code`.
fn loinc(code: &str) -> Coding {
    Coding {
        system: Some(LOINC.into()),
        code: Some(code.into()),
        ..Coding::default()
    }
}

/// The literal reference `url`.
fn reference(url: &str) -> Reference {
    Reference {
        reference: Some(url.into()),
        ..Reference::default()
    }
}

/// The string member `key` of `object`.
fn text<'a>(object: &'a Object, key: &str) -> Option<&'a str> {
    match object.get(key) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}

/// The first id from `stem` whose `fullUrl`, as `url` writes it, is not in
/// `taken`, which it is then added to: `stem` itself when `held` names it,
/// and `{stem}-{n}` otherwise.
fn free(
    taken: &mut BTreeSet<String>,
    url: impl Fn(&str) -> String,
    stem: &str,
    held: bool,
) -> String {
    let mut candidate = if held {
        stem.to_owned()
    } else {
        format!("{stem}-1")
    };
    let mut next = 1_u32;
    while taken.contains(&url(&candidate)) {
        next = next.saturating_add(1);
        candidate = format!("{stem}-{next}");
    }
    taken.insert(url(&candidate));
    candidate
}

/// Rewrites every `reference` member under `value` that names a renamed
/// `Type/id`, by the pairs `renamed`.
fn rename(value: &mut Value, renamed: &[(String, String)]) {
    match value {
        Value::Object(object) => {
            for (key, member) in object.iter_mut() {
                if let (true, Value::String(text)) = (key == "reference", &mut *member)
                    && let Some((_, to)) = renamed.iter().find(|(from, _)| from == text)
                {
                    to.clone_into(text);
                } else {
                    rename(member, renamed);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                rename(item, renamed);
            }
        }
        _ => {}
    }
}

/// The codec failure `source`.
fn codec(source: impl std::error::Error + Send + Sync + 'static) -> DocumentError {
    DocumentError::Codec(Box::new(source))
}
