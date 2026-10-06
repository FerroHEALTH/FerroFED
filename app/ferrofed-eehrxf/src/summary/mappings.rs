// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mappings a deployment supplies, one context mapping per
//! entry, each feeding one section of the patient summary.
//!
//! A FHIRconnect 1.0.0 context mapping maps one operational template to one
//! profile, so a section a template feeds through several profiles takes
//! one entry per profile. Each entry is compiled once, when the
//! configuration is read, and held to the crosswalk: its profile must be one
//! the crosswalk admits for an entry of its section
//! ([`Row::contexts`](eehrxf::crosswalk::Row)), so no mapping can put a
//! resource in a section whose EPS entry slices do not take it. No
//! specification governs how a deployment names its mappings: our own
//! design.

use std::collections::BTreeMap;
use std::path::PathBuf;

use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use eehrxf::document::{self, Slot};
use eehrxf::mapping::{Mapping, MappingError};

use crate::patient_summary::Section;

/// One context mapping as the deployment names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The section the mapping feeds.
    pub section: Section,
    /// The OPT 1.4 operational template, as text.
    pub opt: String,
    /// The model and context mapping files.
    pub files: Vec<PathBuf>,
    /// The `metadata.name` of the context mapping to compile.
    pub context: String,
}

/// Why the mappings cannot be used.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MappingsError {
    /// A context mapping does not compile.
    #[error("the context mapping {context} of the section {section} does not compile")]
    Compile {
        /// The section slug.
        section: &'static str,
        /// The context name.
        context: String,
        /// Why.
        #[source]
        source: MappingError,
    },
    /// A context maps to a profile the crosswalk does not admit for any
    /// entry of the section.
    #[error(
        "the context mapping {context} maps to {profile}, which no entry of the section {section} takes"
    )]
    Profile {
        /// The section slug.
        section: &'static str,
        /// The context name.
        context: String,
        /// The profile it maps to.
        profile: String,
    },
    /// The crosswalk names no EPS section slice for the section.
    #[error("the crosswalk names no section slice for {section}")]
    Slot {
        /// The section slug.
        section: &'static str,
    },
}

/// The compiled mappings, by section and template.
#[derive(Debug, Default)]
pub struct Mappings {
    by: BTreeMap<(Section, String), Vec<Mapping>>,
}

impl Mappings {
    /// Compiles every one of `sources` and holds each to the crosswalk.
    ///
    /// # Errors
    ///
    /// [`MappingsError::Compile`] for a context mapping that does not
    /// compile, [`MappingsError::Profile`] for one whose profile the
    /// section's entries do not take, and [`MappingsError::Slot`] for a
    /// section the crosswalk carries in no slice.
    pub fn compile(sources: &[Source]) -> Result<Self, MappingsError> {
        let mut by: BTreeMap<(Section, String), Vec<Mapping>> = BTreeMap::new();
        for source in sources {
            let section = source.section;
            let mapping = Mapping::compile(&source.opt, &source.files, &[source.context.as_str()])
                .map_err(|error| MappingsError::Compile {
                    section: section.slug(),
                    context: source.context.clone(),
                    source: error,
                })?;
            let admitted = admitted(section)?;
            let mut templates = Vec::new();
            for context in mapping.contexts() {
                if !admitted.contains(&context.profile) {
                    return Err(MappingsError::Profile {
                        section: section.slug(),
                        context: source.context.clone(),
                        profile: context.profile.to_owned(),
                    });
                }
                templates.push(context.template.to_owned());
            }
            if let Some(template) = templates.into_iter().next() {
                by.entry((section, template)).or_default().push(mapping);
            }
        }
        Ok(Self { by })
    }

    /// Returns the mappings that feed `section` from compositions of
    /// `template`, in the order the deployment named them.
    #[must_use]
    pub fn of(&self, section: Section, template: &str) -> &[Mapping] {
        self.by
            .get(&(section, template.to_owned()))
            .map_or(&[], Vec::as_slice)
    }

    /// Returns how many context mappings are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by.values().map(Vec::len).sum()
    }

    /// Returns whether no context mapping is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by.is_empty()
    }
}

/// Returns the EPS section slot the crosswalk carries `section` in.
///
/// # Errors
///
/// [`MappingsError::Slot`] when the crosswalk names no slice for it, or the
/// document knows no slot by that name.
pub fn slot_of(section: Section) -> Result<&'static Slot, MappingsError> {
    PATIENT_SUMMARY
        .row(section.element())
        .and_then(|row| row.eps.strip_prefix("Composition.section:"))
        .and_then(document::slot)
        .ok_or(MappingsError::Slot {
            section: section.slug(),
        })
}

/// The profiles the crosswalk admits for an entry of `section`.
fn admitted(section: Section) -> Result<Vec<&'static str>, MappingsError> {
    let slot = slot_of(section)?;
    let entries = format!("Composition.section:{}.entry", slot.slice);
    let below = format!("{}.", section.element());
    Ok(PATIENT_SUMMARY
        .rows
        .iter()
        .filter(|row| row.path.starts_with(&below) && row.eps.starts_with(&entries))
        .flat_map(|row| row.contexts.iter().copied())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{admitted, slot_of};
    use crate::patient_summary::Section;

    #[test]
    fn every_section_has_a_slot_and_admits_a_profile() {
        for section in Section::ALL {
            let slot = slot_of(section).unwrap();
            assert!(!slot.code.is_empty(), "{section:?}");
            assert!(!admitted(section).unwrap().is_empty(), "{section:?}");
        }
    }

    #[test]
    fn the_allergies_take_the_european_allergy_profile() {
        assert_eq!(
            slot_of(Section::AllergiesAndIntolerances).unwrap().slice,
            "sectionAllergies"
        );
        assert_eq!(
            admitted(Section::AllergiesAndIntolerances).unwrap(),
            ["http://hl7.eu/fhir/base/StructureDefinition/allergyIntolerance-eu-core"]
        );
    }
}
