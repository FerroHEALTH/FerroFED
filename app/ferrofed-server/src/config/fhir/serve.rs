// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[[fhir.serve]]` entries.
//!
//! Each names the members that serve electronic prescriptions or
//! dispensations, each on its own endpoint or, for prescriptions, in the map
//! mode from its openEHR medication orders. No specification governs the
//! configuration: our own design.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eehrxf::category::Category;
use ferrofed_eehrxf::prescriptions::mapped::{MapMode, Template};
use ferrofed_eehrxf::serving::Serving;
use serde::Deserialize;

use super::{Fhir, FhirError};

/// One `[[fhir.serve]]` entry: the members that serve one Art 14(1)
/// category resource by resource.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServeEntry {
    /// The category, by its HL7 Europe priority category code:
    /// `Electronic-Prescriptions` or `Electronic-Dispensations`.
    pub category: String,
    /// The members that serve it, each by its node id alone, which serves it
    /// on its own endpoint, or as a table that names its mode.
    pub members: Vec<ServeMember>,
}

/// One member of a `[[fhir.serve]]` entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ServeMember {
    /// The node id of a member that serves the category on its own
    /// `hl7-fhir-rest` endpoint.
    Native(String),
    /// A member with its mode written out.
    Declared(DeclaredMember),
}

impl ServeMember {
    /// The member's node id.
    #[must_use]
    pub fn node(&self) -> &str {
        match self {
            Self::Native(node) => node,
            Self::Declared(declared) => &declared.member,
        }
    }
}

/// A `[[fhir.serve]]` member written as a table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DeclaredMember {
    /// The node id of the member.
    pub member: String,
    /// How the member serves the category.
    pub mode: ServeMode,
    /// The operational template its medication order compositions are of,
    /// in the map mode: an OPT 1.4 `.opt`, or an ADL 2 template beside
    /// `archetypes`.
    pub template: Option<PathBuf>,
    /// The archetypes an ADL 2 `template` specialises or fills a slot with,
    /// each an ADL file.
    pub archetypes: Vec<PathBuf>,
    /// The language each composition of a received dispensation is written
    /// in at the member in the map mode, an ISO 639-1 code, with `territory`.
    pub language: Option<String>,
    /// The territory each such composition is written in, an ISO 3166-1
    /// code, with `language`.
    pub territory: Option<String>,
}

/// How a member serves a category on the FHIR face.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServeMode {
    /// On its own `hl7-fhir-rest` endpoint, passed through.
    #[default]
    Native,
    /// From its openEHR medication orders, read over ITS-REST and mapped
    /// by the face.
    Map,
}

impl Fhir {
    /// Reads the `[[fhir.serve]]` entries.
    ///
    /// # Errors
    ///
    /// [`FhirError::Serving`] for the first entry that is refused.
    pub(super) fn serving(&self) -> Result<Serving, FhirError> {
        let sources: Vec<ferrofed_eehrxf::serving::Source> = self
            .serve
            .iter()
            .map(|entry| ferrofed_eehrxf::serving::Source {
                category: entry.category.clone(),
                members: entry
                    .members
                    .iter()
                    .map(|member| member.node().to_owned())
                    .collect(),
            })
            .collect();
        Ok(Serving::compile(&sources)?)
    }

    /// Compiles the mapping of every member `[[fhir.serve]]` names in the
    /// map mode, by its node id.
    ///
    /// # Errors
    ///
    /// [`FhirError::MapCategory`] for a map-mode member of a category other
    /// than prescriptions, [`FhirError::MapTemplate`] for a map-mode member
    /// with no template or a native one with one, [`FhirError::MapFile`] for
    /// a file that cannot be read, and [`FhirError::Mapped`] when the MPD
    /// project does not compile against the template.
    pub(super) fn mapped(&self) -> Result<BTreeMap<String, MapMode>, FhirError> {
        let mut mapped = BTreeMap::new();
        for (position, entry) in self.serve.iter().enumerate() {
            for member in &entry.members {
                let ServeMember::Declared(declared) = member else {
                    continue;
                };
                let named = || declared.member.clone();
                let template = match (declared.mode, &declared.template) {
                    (ServeMode::Native, None) if declared.archetypes.is_empty() => continue,
                    (ServeMode::Map, Some(template)) => template,
                    _ => {
                        return Err(FhirError::MapTemplate {
                            position,
                            member: named(),
                        });
                    }
                };
                if entry.category != Category::Prescription.code() {
                    return Err(FhirError::MapCategory {
                        position,
                        member: named(),
                    });
                }
                let read = |path: &Path| {
                    std::fs::read_to_string(path).map_err(|source| FhirError::MapFile {
                        position,
                        member: named(),
                        path: path.to_path_buf(),
                        source,
                    })
                };
                let text = read(template)?;
                let archetypes = super::archetypes(&declared.archetypes, |path| read(path))?;
                // NOTE: no specification governs this: our own design; an OPT 1.4 is named `.opt`,
                // and any other template is read as ADL 2 with its archetypes.
                let written = if template
                    .extension()
                    .is_some_and(|extension| extension == "opt")
                {
                    Template::Opt14(&text)
                } else {
                    Template::Adl2 {
                        template: &text,
                        archetypes: &archetypes,
                    }
                };
                let mut mode = MapMode::compile(written).map_err(|source| FhirError::Mapped {
                    position,
                    member: named(),
                    source: Box::new(source),
                })?;
                match (&declared.language, &declared.territory) {
                    (Some(language), Some(territory)) => {
                        mode = mode.with_defaults(language, territory);
                    }
                    (None, None) => {}
                    _ => {
                        return Err(FhirError::MapDefaults {
                            position,
                            member: named(),
                        });
                    }
                }
                mapped.insert(declared.member.clone(), mode);
            }
        }
        // NOTE: RM 1.1.0 COMPOSITION.language and territory are 1..1, so a composition a received
        // dispensation is written as needs both.
        for (position, entry) in self.serve.iter().enumerate() {
            if entry.category != Category::Dispensation.code() {
                continue;
            }
            for member in &entry.members {
                if mapped
                    .get(member.node())
                    .is_some_and(|mode| !mode.composes())
                {
                    return Err(FhirError::MapUndefaulted {
                        position,
                        member: member.node().to_owned(),
                    });
                }
            }
        }
        Ok(mapped)
    }
}
