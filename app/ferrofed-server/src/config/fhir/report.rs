// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[[fhir.report]]` entries.
//!
//! Each names a category of report the gateway writes from the members'
//! compositions for the document list, `Laboratory-Reports`,
//! `Discharge-Reports` or `Medical-Imaging`, with the operational template those compositions
//! are of: an OPT 1.4, or an ADL 2 template with the archetypes it
//! specialises or fills a slot with. A laboratory report without an entry
//! is written with the template of its `[[fhir.receive]]` entry. No
//! specification governs the configuration: our own design.

use std::path::PathBuf;

use eehrxf::category::Category;
use ferrofed_eehrxf::laboratory::Served;
use ferrofed_eehrxf::prescriptions::mapped::Template;
use ferrofed_eehrxf::report::{ServedError, Writes, Written};
use serde::Deserialize;

use super::{Fhir, FhirError, FhirSettings};

/// One `[[fhir.report]]` entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReportEntry {
    /// The category, by its HL7 Europe priority category code:
    /// `Laboratory-Reports`, `Discharge-Reports` or `Medical-Imaging`.
    pub category: String,
    /// The operational template the members' compositions of the category
    /// are of: an OPT 1.4 `.opt`, or an ADL 2 template beside `archetypes`.
    pub template: PathBuf,
    /// The archetypes an ADL 2 `template` specialises or fills a slot with,
    /// each an ADL file.
    pub archetypes: Vec<PathBuf>,
}

/// Why a `[[fhir.report]]` entry is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReportError {
    /// The entry names a category the gateway writes no report of.
    #[error(
        "fhir.report[{position}].category {category:?} is no category the gateway writes reports of"
    )]
    Category {
        /// The position of the entry.
        position: usize,
        /// The category as written.
        category: String,
    },
    /// Two entries name one category.
    #[error("fhir.report[{position}] names {category} a second time")]
    Twice {
        /// The position of the second entry.
        position: usize,
        /// The category.
        category: &'static str,
    },
    /// A file of the entry cannot be read.
    #[error("fhir.report[{position}] {} cannot be read", path.display())]
    File {
        /// The position of the entry.
        position: usize,
        /// The path.
        path: PathBuf,
        /// Why.
        #[source]
        source: std::io::Error,
    },
}

/// A `[[fhir.report]]` entry's template, read.
#[derive(Debug)]
pub(super) struct Declared {
    /// The template's text.
    text: String,
    /// Each archetype, its file name and its text; none for an OPT 1.4.
    archetypes: Vec<(String, String)>,
    /// Whether the template is an OPT 1.4.
    opt14: bool,
}

impl Declared {
    /// The template as the shipped mappings compile against it.
    pub(super) fn template(&self) -> Template<'_> {
        if self.opt14 {
            Template::Opt14(&self.text)
        } else {
            Template::Adl2 {
                template: &self.text,
                archetypes: &self.archetypes,
            }
        }
    }
}

impl Fhir {
    /// Reads the `[[fhir.report]]` entry of `category`, when there is one.
    ///
    /// # Errors
    ///
    /// [`ReportError::Category`] for an entry of a category the gateway
    /// writes no report of, [`ReportError::Twice`] for a category named
    /// twice, and [`ReportError::File`] for a file that cannot be read.
    pub(super) fn declared(&self, category: Category) -> Result<Option<Declared>, FhirError> {
        let mut found = None;
        for (position, entry) in self.report.iter().enumerate() {
            let written =
                Written::of_category(&entry.category).ok_or_else(|| ReportError::Category {
                    position,
                    category: entry.category.clone(),
                })?;
            if written.category() != category.code() {
                continue;
            }
            if found.is_some() {
                return Err(ReportError::Twice {
                    position,
                    category: written.category(),
                }
                .into());
            }
            found = Some((position, entry));
        }
        let Some((position, entry)) = found else {
            return Ok(None);
        };
        let read = |path: &PathBuf| {
            std::fs::read_to_string(path).map_err(|source| ReportError::File {
                position,
                path: path.clone(),
                source,
            })
        };
        let mut archetypes = Vec::with_capacity(entry.archetypes.len());
        for path in &entry.archetypes {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            archetypes.push((name, read(path)?));
        }
        // NOTE: no specification governs this: our own design; an OPT 1.4 is named `.opt`, and any
        // other template is read as ADL 2 with its archetypes, as `[[fhir.serve]]` reads one.
        let opt14 = entry
            .template
            .extension()
            .is_some_and(|extension| extension == "opt");
        Ok(Some(Declared {
            text: read(&entry.template)?,
            archetypes,
            opt14,
        }))
    }

    /// Compiles the shipped discharge mapping against the template of the
    /// `Discharge-Reports` `[[fhir.report]]` entry, when there is one.
    ///
    /// # Errors
    ///
    /// The [`FhirError::Report`] of an entry that is refused, and
    /// [`FhirError::Discharge`] when the mapping does not compile against
    /// the template.
    pub(super) fn discharge(
        &self,
    ) -> Result<Option<ferrofed_eehrxf::discharge::Served>, FhirError> {
        self.served(
            Category::Discharge,
            ferrofed_eehrxf::discharge::Served::compile,
            |source| FhirError::Discharge { source },
        )
    }

    /// Compiles the shipped imaging mapping against the template of the
    /// `Medical-Imaging` `[[fhir.report]]` entry, when there is one.
    ///
    /// # Errors
    ///
    /// The [`FhirError::Report`] of an entry that is refused, and
    /// [`FhirError::Imaging`] when the mapping does not compile against the
    /// template.
    pub(super) fn imaging(&self) -> Result<Option<ferrofed_eehrxf::imaging::Served>, FhirError> {
        self.served(
            Category::Imaging,
            ferrofed_eehrxf::imaging::Served::compile,
            |source| FhirError::Imaging { source },
        )
    }

    /// Compiles a shipped mapping with `compile` against the template of the
    /// `[[fhir.report]]` entry of `category`, when there is one, refused as
    /// `refused` says.
    fn served<T>(
        &self,
        category: Category,
        compile: impl FnOnce(Template<'_>) -> Result<T, ServedError>,
        refused: impl FnOnce(Box<ServedError>) -> FhirError,
    ) -> Result<Option<T>, FhirError> {
        let Some(declared) = self.declared(category)? else {
            return Ok(None);
        };
        compile(declared.template())
            .map(Some)
            .map_err(|source| refused(Box::new(source)))
    }

    /// Compiles the shipped laboratory mapping against the template of the
    /// `Laboratory-Reports` `[[fhir.report]]` entry, else of its
    /// `[[fhir.receive]]` entry, when there is one.
    ///
    /// # Errors
    ///
    /// [`FhirError::ReceiveTemplate`] when the template cannot be read, and
    /// [`FhirError::Laboratory`] when the mapping does not compile against it.
    pub(super) fn laboratory(&self) -> Result<Option<Served>, FhirError> {
        if let Some(declared) = self.declared(Category::Laboratory)? {
            return Served::compile(declared.template())
                .map(Some)
                .map_err(|source| FhirError::Laboratory {
                    source: Box::new(source),
                });
        }
        // NOTE: no specification governs this: our own design; the template a member's received
        // reports are written in is the one its laboratory compositions are of.
        let Some((position, entry)) = self
            .receive
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.category == Category::Laboratory.code())
        else {
            return Ok(None);
        };
        let opt = std::fs::read_to_string(&entry.template).map_err(|source| {
            FhirError::ReceiveTemplate {
                position,
                path: entry.template.clone(),
                source,
            }
        })?;
        Served::compile(Template::Opt14(&opt))
            .map(Some)
            .map_err(|source| FhirError::Laboratory {
                source: Box::new(source),
            })
    }
}

impl FhirSettings {
    /// The mapping the document list writes a report of the category
    /// `written` with, or `None` when the face writes none of it.
    #[must_use]
    pub fn writes(&self, written: Written) -> Option<&dyn Writes> {
        let served: Option<&dyn Writes> = match written {
            Written::Laboratory => Some(self.laboratory.as_deref()?),
            Written::Discharge => Some(self.discharge.as_deref()?),
            Written::Imaging => Some(self.imaging.as_deref()?),
        };
        served
    }
}
