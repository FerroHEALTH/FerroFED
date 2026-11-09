// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FHIRconnect mappings a deployment supplies, each feeding one section
//! of the patient summary.
//!
//! A FHIRconnect 1.0.0 context mapping maps one operational template to one
//! profile. A context that maps to an entry profile writes entries of its
//! section, so a section a template feeds through several profiles takes
//! one entry per profile, and its profile must be one the crosswalk admits
//! for an entry of its section ([`Row::contexts`](eehrxf::crosswalk::Row)).
//! A context that maps to the summary's own `composition-eu-eps`, the root
//! context of `eehrxf`'s EPS project, writes the whole composition and
//! enters each section through a sub-context (FHIRconnect 1.0.0
//! `types-of-mapping-files/sub-contexts.adoc`); a section takes from its run
//! the entries of its own slot, each held to the same crosswalk profiles
//! when the run is taken. So no mapping can put a resource in a section
//! whose EPS entry slices do not take it. Each context is compiled once,
//! when the configuration is read, however many sections name it. No
//! specification governs how a deployment names its mappings: our own
//! design.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::prescriptions::mapped::{MapModeError, Template};
use eehrxf::crosswalk::patient_summary::PATIENT_SUMMARY;
use eehrxf::document::form::{self, PATIENT_SUMMARY_FORM, Slot};
use eehrxf::mapping::{Mapping, MappingError};

use crate::patient_summary::Section;

/// One context mapping as the deployment names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The section the mapping feeds.
    pub section: Section,
    /// The operational template, as text: an OPT 1.4, or an ADL 2 template
    /// when `archetypes` holds the archetypes it specialises.
    pub opt: String,
    /// The archetypes an ADL 2 template specialises, each a file name and
    /// its text; none for an OPT 1.4.
    pub archetypes: Vec<(String, String)>,
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
    /// The template does not read, or its operational template cannot be
    /// built.
    #[error("the template of the section {section} is refused")]
    Template {
        /// The section slug.
        section: &'static str,
        /// Why.
        #[source]
        source: Box<MapModeError>,
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

/// What a compiled context mapping writes, which decides what a section
/// takes from its run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Writes {
    /// Entries of the section: the context maps to an entry profile the
    /// section admits, and every resource it writes but a `Provenance` and a
    /// `Medication` it names is an entry.
    Entries,
    /// The summary's composition: the context maps to `composition-eu-eps`,
    /// and the section takes the entries of its own slot.
    Composition,
}

/// One compiled context mapping feeding a section.
#[derive(Debug, Clone)]
pub struct Feed {
    mapping: Arc<Mapping>,
    writes: Writes,
    id: usize,
}

impl Feed {
    /// Returns the compiled mapping.
    #[must_use]
    pub fn mapping(&self) -> &Mapping {
        &self.mapping
    }

    /// Returns what the mapping writes.
    #[must_use]
    pub const fn writes(&self) -> Writes {
        self.writes
    }

    /// Returns the number of the compiled mapping, the same for every
    /// section that names it, so one composition is run through it once.
    #[must_use]
    pub const fn id(&self) -> usize {
        self.id
    }
}

/// What a context mapping is compiled from: the template, its archetypes,
/// the files and the context name.
type Compiled = (String, Vec<(String, String)>, Vec<PathBuf>, String);

/// The compiled mappings, by section and template.
#[derive(Debug, Default)]
pub struct Mappings {
    by: BTreeMap<(Section, String), Vec<Feed>>,
}

impl Mappings {
    /// Compiles every one of `sources` and holds each to the crosswalk.
    ///
    /// # Errors
    ///
    /// [`MappingsError::Compile`] for a context mapping that does not
    /// compile, [`MappingsError::Profile`] for one whose profile is neither
    /// one the section's entries take nor the summary's composition, and
    /// [`MappingsError::Slot`] for a section the crosswalk carries in no
    /// slice.
    pub fn compile(sources: &[Source]) -> Result<Self, MappingsError> {
        let mut by: BTreeMap<(Section, String), Vec<Feed>> = BTreeMap::new();
        let mut compiled: BTreeMap<Compiled, (Arc<Mapping>, usize)> = BTreeMap::new();
        for source in sources {
            let section = source.section;
            let key = (
                source.opt.clone(),
                source.archetypes.clone(),
                source.files.clone(),
                source.context.clone(),
            );
            let (mapping, id) = if let Some(held) = compiled.get(&key) {
                held.clone()
            } else {
                let held = (Arc::new(compile_one(source)?), compiled.len());
                compiled.insert(key, held.clone());
                held
            };
            let admitted = admitted(section)?;
            let mut feeds = Vec::new();
            for context in mapping.contexts() {
                let writes = if admitted.contains(&context.profile) {
                    Writes::Entries
                } else if context.profile == PATIENT_SUMMARY_FORM.composition {
                    Writes::Composition
                } else {
                    return Err(MappingsError::Profile {
                        section: section.slug(),
                        context: source.context.clone(),
                        profile: context.profile.to_owned(),
                    });
                };
                feeds.push((context.template.to_owned(), writes));
            }
            if let Some((template, writes)) = feeds.into_iter().next() {
                by.entry((section, template)).or_default().push(Feed {
                    mapping: Arc::clone(&mapping),
                    writes,
                    id,
                });
            }
        }
        Ok(Self { by })
    }

    /// Returns the mappings that feed `section` from compositions of
    /// `template`, in the order the deployment named them.
    #[must_use]
    pub fn of(&self, section: Section, template: &str) -> &[Feed] {
        self.by
            .get(&(section, template.to_owned()))
            .map_or(&[], Vec::as_slice)
    }

    /// Returns how many context mappings feed a section, a mapping that
    /// feeds two sections counted for each.
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

/// Compiles the context mapping `source` names.
///
/// # Errors
///
/// [`MappingsError::Template`] for a template that does not read, and
/// [`MappingsError::Compile`] for a context mapping that does not compile.
fn compile_one(source: &Source) -> Result<Mapping, MappingsError> {
    let section = source.section;
    let written = if source.archetypes.is_empty() {
        Template::Opt14(&source.opt)
    } else {
        Template::Adl2 {
            template: &source.opt,
            archetypes: &source.archetypes,
        }
    };
    let refused = |error| MappingsError::Template {
        section: section.slug(),
        source: Box::new(error),
    };
    let template = written.source().map_err(refused)?;
    Mapping::compile_template(&template, &source.files, &[source.context.as_str()]).map_err(
        |error| MappingsError::Compile {
            section: section.slug(),
            context: source.context.clone(),
            source: error,
        },
    )
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
        .and_then(form::slot)
        .ok_or(MappingsError::Slot {
            section: section.slug(),
        })
}

/// The profiles the crosswalk admits for an entry of `section`.
///
/// # Errors
///
/// [`MappingsError::Slot`] when the crosswalk names no slice for it.
pub(crate) fn admitted(section: Section) -> Result<Vec<&'static str>, MappingsError> {
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
