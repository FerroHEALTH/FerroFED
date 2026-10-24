// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The codes of a mapped resource that are in no preferred system,
//! translated through the configured terminology server.
//!
//! [`Translator::translate`] applies to a FHIR resource the rule
//! [`transcode_with`](crate::terminology::transcode::transcode_with)
//! applies to one openEHR text item. Each `CodeableConcept` of the
//! resource none of whose codings is in a preferred system is asked of
//! `ConceptMap/$translate` (<https://hl7.org/fhir/R4/terminology-service.html>)
//! for each original coding into each target system, and every `equivalent`
//! or `equal` match ([`ACCEPTED_EQUIVALENCES`](crate::terminology::ACCEPTED_EQUIVALENCES))
//! is added after the codings the concept carries, which stay as recorded.
//! With no server configured, the resource is left as it is: the codes are
//! transferred as recorded, the interim rule of the eHealth Network
//! Guidelines on Patient Summary, Release 3.4, Art 10(7). A server that
//! refuses or does not answer fails the translation; it is never taken as
//! no translation.
//!
//! A bare `Coding` element holds one code and has no room for a second, so
//! it is left as recorded. The resource's `meta` and the resources it
//! contains are not translated.

use std::collections::BTreeMap;

use fhir_types::codec::DecodeError;
use fhir_types::codec::EncodeError;
use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::coding::Coding;
use fhir_types::r4::resource::Resource;

use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::walk;
use crate::terminology::walk::Coded;
use crate::terminology::walk::Step;

/// Why the codes of a resource cannot be translated.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TranslationError {
    /// The resource does not encode as FHIR JSON.
    #[error("the resource does not encode as FHIR JSON")]
    Encode {
        /// Why.
        #[source]
        source: EncodeError,
    },
    /// The resource holds an element the R4 element table does not
    /// describe.
    #[error("{location} is an element the R4 element table does not describe")]
    Unwalkable {
        /// The element path, with indexes.
        location: String,
    },
    /// The resource with its translations added does not decode.
    #[error("the resource with its translations does not decode")]
    Decode {
        /// Why.
        #[source]
        source: DecodeError,
    },
    /// The terminology server gave no answer.
    #[error("the terminology server gave no translation")]
    Terminology(#[from] TerminologyError),
}

/// A concept to translate: where it is, and its codings as system and code.
struct Pending {
    location: String,
    steps: Vec<Step>,
    originals: Vec<(String, String)>,
}

/// The translator of the resources of one document: the configured
/// terminology, the systems it translates into, and every answer it has
/// had, so each code and target is asked once per document.
#[derive(Debug)]
pub struct Translator<'t> {
    terminology: &'t Terminology,
    targets: &'t [&'t str],
    asked: BTreeMap<(String, String, String), Vec<Coding>>,
}

impl<'t> Translator<'t> {
    /// Returns the translator that asks `terminology` for translations into
    /// each of `targets`, nothing asked yet.
    #[must_use]
    pub const fn new(terminology: &'t Terminology, targets: &'t [&'t str]) -> Self {
        Self {
            terminology,
            targets,
            asked: BTreeMap::new(),
        }
    }

    /// Returns `resource` with every `equivalent` or `equal` translation of
    /// each of its concepts that has no coding whose system `preferred`
    /// admits added after the concept's own codings.
    ///
    /// With no server configured, `resource` is answered as it is.
    ///
    /// # Errors
    ///
    /// [`TranslationError::Terminology`] for a `$translate` that gives no
    /// answer, [`TranslationError::Encode`] and [`TranslationError::Decode`]
    /// for a resource that does not encode or decode, and
    /// [`TranslationError::Unwalkable`] for one holding an element the R4
    /// table does not describe.
    pub async fn translate(
        &mut self,
        resource: &Resource,
        preferred: impl Fn(&str) -> bool + Sync,
    ) -> Result<Resource, TranslationError> {
        // NOTE: eHN PS Art 10(7) is the rule when no transcoding exists, and the server is
        // optional, so with none configured each code is transferred as recorded.
        if !self.terminology.is_configured() {
            return Ok(resource.clone());
        }
        let mut object =
            Json::to_json(resource).map_err(|source| TranslationError::Encode { source })?;
        let kind = object
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or("Resource")
            .to_owned();
        let pending = untranscoded(&object, &kind, &preferred)?;
        if pending.is_empty() {
            return Ok(resource.clone());
        }
        let mut changed = false;
        for concept in pending {
            let added = self.equivalents(&concept.originals).await?;
            if added.is_empty() {
                continue;
            }
            changed = true;
            let codings = codings_at(&mut object, &concept.steps).ok_or_else(|| {
                TranslationError::Unwalkable {
                    location: concept.location.clone(),
                }
            })?;
            for coding in &added {
                let encoded =
                    Json::to_json(coding).map_err(|source| TranslationError::Encode { source })?;
                codings.push(Value::Object(encoded));
            }
        }
        if !changed {
            return Ok(resource.clone());
        }
        Resource::from_json(&object, &mut Path::root(&kind))
            .map_err(|source| TranslationError::Decode { source })
    }

    /// Returns every accepted translation of `originals` into the targets,
    /// in order, leaving out a coding the originals or an earlier
    /// translation already carry.
    async fn equivalents(
        &mut self,
        originals: &[(String, String)],
    ) -> Result<Vec<Coding>, TerminologyError> {
        let mut added: Vec<Coding> = Vec::new();
        for (system, code) in originals {
            for target in self.targets {
                let key = (system.clone(), code.clone(), (*target).to_owned());
                if !self.asked.contains_key(&key) {
                    let found = self.terminology.translate(system, code, target).await?;
                    self.asked.insert(key.clone(), found);
                }
                for found in self.asked.get(&key).into_iter().flatten() {
                    let pair = identity(found);
                    let known = pair.is_some_and(|(system, code)| {
                        originals
                            .iter()
                            .any(|(held, value)| held == system && value == code)
                    });
                    if !known && !added.iter().any(|held| identity(held) == pair) {
                        added.push(found.clone());
                    }
                }
            }
        }
        Ok(added)
    }
}

/// Returns every concept of `object` none of whose codings has a system
/// `preferred` admits and which carries at least one coding with a system
/// and a code.
fn untranscoded(
    object: &Object,
    kind: &str,
    preferred: &impl Fn(&str) -> bool,
) -> Result<Vec<Pending>, TranslationError> {
    let found =
        walk::coded(object, kind, false).map_err(|unwalkable| TranslationError::Unwalkable {
            location: unwalkable.location,
        })?;
    let mut pending = Vec::new();
    for occurrence in found {
        if occurrence.element.starts_with("Meta.") {
            continue;
        }
        let Coded::Concept(concept) = occurrence.value else {
            continue;
        };
        let codings: Vec<&Object> = concept
            .get("coding")
            .and_then(Value::as_array)
            .map(|codings| codings.iter().filter_map(Value::as_object).collect())
            .unwrap_or_default();
        let carried = codings.iter().any(|coding| {
            coding
                .get("system")
                .and_then(Value::as_str)
                .is_some_and(preferred)
        });
        if carried {
            continue;
        }
        let originals: Vec<(String, String)> = codings
            .iter()
            .filter_map(|coding| {
                let system = coding.get("system").and_then(Value::as_str)?;
                let code = coding.get("code").and_then(Value::as_str)?;
                Some((system.to_owned(), code.to_owned()))
            })
            .collect();
        if originals.is_empty() {
            continue;
        }
        pending.push(Pending {
            location: occurrence.location,
            steps: occurrence.steps,
            originals,
        });
    }
    Ok(pending)
}

/// Returns the `coding` array of the concept `steps` reach from `object`.
fn codings_at<'o>(object: &'o mut Object, steps: &[Step]) -> Option<&'o mut Vec<Value>> {
    let (first, rest) = steps.split_first()?;
    let Step::Key(key) = first else {
        return None;
    };
    let mut node = object.get_mut(key)?;
    for step in rest {
        node = match (step, node) {
            (Step::Key(key), Value::Object(inner)) => inner.get_mut(key)?,
            (Step::Index(index), Value::Array(items)) => items.get_mut(*index)?,
            _ => return None,
        };
    }
    let Value::Object(concept) = node else {
        return None;
    };
    match concept.get_mut("coding")? {
        Value::Array(codings) => Some(codings),
        _ => None,
    }
}

/// Returns the system and code of `coding`, when it carries both.
fn identity(coding: &Coding) -> Option<(&str, &str)> {
    let system = coding.system.as_ref()?.value.as_deref()?;
    let code = coding.code.as_ref()?.value.as_deref()?;
    Some((system, code))
}
