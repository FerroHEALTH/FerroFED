// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The display of every coding of a received document.
//!
//! The openEHR side needs the text of every coded term: `DV_CODED_TEXT.value`
//! is mandatory (RM Release 1.1.0, Data Types IM, `DV_CODED_TEXT`). A coding
//! the document sends without a `display` has its display looked up with the
//! terminology server's `$lookup` before the document is mapped; with no
//! server configured, it is refused. The original document is kept as it was
//! received: only the copy the mapping reads carries the looked-up displays.

use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::bundle::Bundle;
use fhir_types::r4::resource::Resource;

use crate::receive::ReceiveError;
use crate::receive::ReceivedDocument;
use crate::receive::entries;
use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::walk;
use crate::terminology::walk::Coded;
use crate::terminology::walk::Step;

/// A coding of a received document that carries no display.
///
/// It names where the coding is, never its code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingDisplay {
    /// Where in the document, as an element path with indexes.
    pub location: String,
    steps: Vec<Step>,
    system: Option<String>,
    code: Option<String>,
}

/// Why a received document's codings cannot all be displayed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DisplayError {
    /// A coding carries no display and no terminology server is configured
    /// to look one up.
    #[error(
        "{location} is a coding with no display, and no terminology server is configured to look one up"
    )]
    Missing {
        /// Where the coding is.
        location: String,
        /// The refusal of the lookup.
        #[source]
        source: TerminologyError,
    },
    /// A coding carries no display, and no system or no code to look one
    /// up by.
    #[error("{location} is a coding with no display and no system and code to look one up by")]
    Uncoded {
        /// Where the coding is.
        location: String,
    },
    /// The lookup of a coding's display gave no answer.
    #[error("the display of the coding at {location} could not be looked up")]
    Lookup {
        /// Where the coding is.
        location: String,
        /// The terminology failure, with the server's status.
        #[source]
        source: TerminologyError,
    },
    /// The document holds an element no walk can read.
    #[error("the document's codings cannot be read")]
    Receive(#[from] ReceiveError),
}

impl ReceivedDocument {
    /// Returns every coding of the document, in every entry, that carries
    /// no display, each object's members in name order.
    ///
    /// # Errors
    ///
    /// [`ReceiveError::Unreadable`] for an element the R4 table does not
    /// describe.
    pub fn missing_displays(&self) -> Result<Vec<MissingDisplay>, ReceiveError> {
        let found = walk::coded(self.tree(), "Bundle", true).map_err(|refused| {
            ReceiveError::Unreadable {
                location: refused.location,
            }
        })?;
        Ok(found
            .into_iter()
            .filter_map(|occurrence| {
                let Coded::Coding(coding) = occurrence.value else {
                    return None;
                };
                let text = |key: &str| {
                    coding
                        .get(key)
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .map(str::to_owned)
                };
                // NOTE: FHIR R4 Coding: a display present and not empty is the text a reader
                // shows, so only a coding with none needs one looked up.
                text("display").is_none().then(|| MissingDisplay {
                    location: occurrence.location,
                    steps: occurrence.steps,
                    system: text("system"),
                    code: text("code"),
                })
            })
            .collect())
    }

    /// Returns the document with the display of every coding that carries
    /// none looked up with `terminology`'s `$lookup`; its original text is
    /// the text received.
    ///
    /// # Errors
    ///
    /// [`DisplayError::Missing`] for a coding with no display when no
    /// server is configured, [`DisplayError::Uncoded`] for one with no
    /// system or no code, [`DisplayError::Lookup`] for a lookup that gives
    /// no answer, and [`DisplayError::Receive`] for a document whose
    /// codings cannot be read or that no longer reads once displayed.
    pub async fn displayed(&self, terminology: &Terminology) -> Result<Self, DisplayError> {
        let missing = self.missing_displays()?;
        if missing.is_empty() {
            return Ok(self.clone());
        }
        let mut tree = self.tree.clone();
        for coding in missing {
            let (Some(system), Some(code)) = (&coding.system, &coding.code) else {
                return Err(DisplayError::Uncoded {
                    location: coding.location,
                });
            };
            let display = match terminology.display(system, code).await {
                Ok(display) => display,
                Err(source @ TerminologyError::Unconfigured { .. }) => {
                    return Err(DisplayError::Missing {
                        location: coding.location,
                        source,
                    });
                }
                Err(source) => {
                    return Err(DisplayError::Lookup {
                        location: coding.location,
                        source,
                    });
                }
            };
            let target =
                follow(&mut tree, &coding.steps).ok_or_else(|| ReceiveError::Unreadable {
                    location: coding.location.clone(),
                })?;
            target.insert(String::from("display"), Value::String(display));
        }
        Ok(self.rebuilt(tree)?)
    }

    /// Returns the document whose decoded form is `tree`, its original text
    /// kept.
    fn rebuilt(&self, tree: Object) -> Result<Self, ReceiveError> {
        let bundle = Bundle::from_json(&tree, &mut Path::root("Bundle"))
            .map_err(|source| ReceiveError::Decode { source })?;
        let Some(Resource::Composition(composition)) = bundle
            .entry
            .first()
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(ReceiveError::NoComposition);
        };
        let composition = composition.clone();
        let index = entries::patient(&tree)?;
        let Some(Resource::Patient(patient)) = bundle
            .entry
            .get(index)
            .and_then(|entry| entry.resource.as_ref())
        else {
            return Err(ReceiveError::SubjectUnresolved);
        };
        let patient = patient.clone();
        Ok(Self {
            original: self.original.clone(),
            tree,
            bundle,
            composition,
            patient,
        })
    }
}

/// Returns the object `steps` lead to from `tree`.
fn follow<'t>(tree: &'t mut Object, steps: &[Step]) -> Option<&'t mut Object> {
    let (first, rest) = steps.split_first()?;
    let Step::Key(key) = first else {
        return None;
    };
    let mut at = tree.get_mut(key)?;
    for step in rest {
        at = match (step, at) {
            (Step::Key(key), Value::Object(object)) => object.get_mut(key)?,
            (Step::Index(index), Value::Array(items)) => items.get_mut(*index)?,
            _ => return None,
        };
    }
    match at {
        Value::Object(object) => Some(object),
        _ => None,
    }
}
