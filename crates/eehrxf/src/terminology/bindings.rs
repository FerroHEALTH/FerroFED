// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `required` bindings of a profile, held over one resource.
//!
//! A `required` binding means "the concept in this element SHALL be from the
//! specified value set" (<https://hl7.org/fhir/R4/terminologies.html#required>).
//! [`check_required`] finds every `code`, `Coding` and `CodeableConcept` of
//! the resource by its R4 type, and holds each one an element of the
//! profile binds `required` to the vendored expansion of its value set: a
//! `code` must be one of the expansion's codes, a `Coding` one of its
//! system and code pairs, and a `CodeableConcept` must carry one coding
//! that is, as `$validate-code` reads a `codeableConcept`
//! (<https://hl7.org/fhir/R4/valueset-operation-validate-code.html>).
//!
//! What the check cannot evaluate it names, never passes: a binding whose
//! value set no expansion carries, one inside a slice, and one on an element
//! of a type that carries no code. [`BindingCheck::complete`] asks the
//! terminology server's `$validate-code` for each concept a missing
//! expansion left open, when a server is configured.

use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Path;
use fhir_types::codec::Value;
use fhir_types::r4::codeable_concept::CodeableConcept;
use fhir_types::r4::coding::Coding;

use crate::dataset::ResourceProfile;
use crate::dataset::constraint::BindingStrength;
use crate::receive::ReceivedDocument;
use crate::terminology::Candidate;
use crate::terminology::Terminology;
use crate::terminology::TerminologyError;
use crate::terminology::expansion::Expansions;
use crate::terminology::walk;
use crate::terminology::walk::Coded;
use crate::terminology::walk::Occurrence;

/// What a binding check found and what it could not evaluate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BindingCheck {
    findings: Vec<BindingFinding>,
    unevaluated: Vec<UnevaluatedBinding>,
    pending: Vec<Pending>,
}

impl BindingCheck {
    /// Returns every occurrence whose concept is not in its value set, in
    /// walk order.
    #[must_use]
    pub fn findings(&self) -> &[BindingFinding] {
        &self.findings
    }

    /// Returns every `required` binding the check did not evaluate, once
    /// each, ordered by element.
    #[must_use]
    pub fn unevaluated(&self) -> &[UnevaluatedBinding] {
        &self.unevaluated
    }

    /// Asks `terminology` for each concept a missing expansion left open,
    /// and returns the check with those bindings evaluated.
    ///
    /// With no server configured the check is returned as it is, every
    /// binding without an expansion still named as not evaluated.
    ///
    /// # Errors
    ///
    /// The [`TerminologyError`] of the first `$validate-code` that gives no
    /// answer: a binding is never taken as held for a call that failed.
    pub async fn complete(mut self, terminology: &Terminology) -> Result<Self, TerminologyError> {
        if !terminology.is_configured() || self.pending.is_empty() {
            return Ok(self);
        }
        for pending in std::mem::take(&mut self.pending) {
            if !terminology
                .validate(&pending.finding.value_set, &pending.concept)
                .await?
            {
                self.findings.push(pending.finding);
            }
        }
        self.unevaluated
            .retain(|binding| binding.gap != BindingGap::NoExpansion);
        Ok(self)
    }
}

/// An occurrence whose concept is not in the value set its element is bound
/// to.
///
/// It names where it is and the value set, and never quotes the concept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingFinding {
    /// The profile's canonical URL.
    pub profile: String,
    /// The element id that carries the binding.
    pub element: String,
    /// Where in the resource, as an element path with indexes.
    pub location: String,
    /// The canonical URL of the value set.
    pub value_set: String,
}

/// A `required` binding the check did not evaluate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnevaluatedBinding {
    /// The profile's canonical URL.
    pub profile: String,
    /// The element id that carries the binding.
    pub element: String,
    /// The canonical URL of the value set, when the binding names one.
    pub value_set: Option<String>,
    /// Why it was not evaluated.
    pub gap: BindingGap,
}

/// Why a `required` binding was not evaluated.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum BindingGap {
    /// No package read carries an expansion of the value set.
    NoExpansion,
    /// The binding names no value set.
    NoValueSet,
    /// The element lies in a slice, whose occurrences the check does not
    /// assign.
    Sliced,
    /// The element is of a type that carries no code, by its type code.
    Type(String),
}

/// A concept a missing expansion left open.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pending {
    finding: BindingFinding,
    concept: Candidate,
}

/// Why a resource cannot be checked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingError {
    /// The resource carries an element the R4 table does not describe.
    #[error("{location} is an element the R4 element table does not describe")]
    Unreadable {
        /// The element path.
        location: String,
    },
    /// A coded element does not decode as its R4 type.
    #[error("{location} does not decode as its R4 type")]
    Decode {
        /// The element path.
        location: String,
        /// The decode failure.
        #[source]
        source: fhir_types::codec::DecodeError,
    },
}

/// Checks every `required` binding of `profile` over `resource`, found at
/// `location`, against `expansions`.
///
/// The resources `resource` carries are not walked: each is held to the
/// profile it claims.
///
/// # Errors
///
/// [`BindingError::Unreadable`] for an element the R4 table does not
/// describe, and [`BindingError::Decode`] for a coded element that does not
/// decode as its type.
pub fn check_required(
    profile: &ResourceProfile,
    resource: &Object,
    location: &str,
    expansions: &Expansions,
) -> Result<BindingCheck, BindingError> {
    let found =
        walk::coded(resource, location, false).map_err(|refused| BindingError::Unreadable {
            location: refused.location,
        })?;
    let mut check = BindingCheck::default();
    for element in profile.elements() {
        let Some(binding) = element
            .binding()
            .filter(|binding| binding.strength() == BindingStrength::Required)
        else {
            continue;
        };
        let id = element.path().as_str();
        let value_set = binding.value_set().map(str::to_owned);
        let unevaluated = |gap| UnevaluatedBinding {
            profile: profile.url().to_owned(),
            element: id.to_owned(),
            value_set: value_set.clone(),
            gap,
        };
        let types = element.types();
        if let Some(other) = types
            .iter()
            .find(|code| !matches!(code.as_str(), "code" | "Coding" | "CodeableConcept"))
        {
            check
                .unevaluated
                .push(unevaluated(BindingGap::Type(other.clone())));
            continue;
        }
        if id.contains(':') {
            check.unevaluated.push(unevaluated(BindingGap::Sliced));
            continue;
        }
        let Some(url) = &value_set else {
            check.unevaluated.push(unevaluated(BindingGap::NoValueSet));
            continue;
        };
        let occurrences: Vec<&Occurrence<'_>> =
            found.iter().filter(|found| found.element == id).collect();
        if occurrences.is_empty() {
            continue;
        }
        let expanded = expansions.has(url);
        if !expanded {
            check.unevaluated.push(unevaluated(BindingGap::NoExpansion));
        }
        for occurrence in occurrences {
            let finding = BindingFinding {
                profile: profile.url().to_owned(),
                element: id.to_owned(),
                location: occurrence.location.clone(),
                value_set: url.clone(),
            };
            if expanded {
                if !held(expansions, url, &occurrence.value) {
                    check.findings.push(finding);
                }
            } else {
                let concept = concept(profile.url(), id, occurrence)?;
                check.pending.push(Pending { finding, concept });
            }
        }
    }
    check.unevaluated.sort();
    check.unevaluated.dedup();
    Ok(check)
}

impl ReceivedDocument {
    /// Checks the `required` bindings of `bundle`, a profile of `Bundle`,
    /// over the document, and of `composition`, a profile of `Composition`,
    /// over its first entry, against `expansions`.
    ///
    /// # Errors
    ///
    /// The [`BindingError`] of [`check_required`], and
    /// [`BindingError::Unreadable`] for a document whose first entry is no
    /// resource.
    pub fn check_bindings(
        &self,
        bundle: &ResourceProfile,
        composition: &ResourceProfile,
        expansions: &Expansions,
    ) -> Result<BindingCheck, BindingError> {
        let root = self.tree();
        let location = "Bundle.entry[0].resource";
        let first = root
            .get("entry")
            .and_then(Value::as_array)
            .and_then(<[Value]>::first)
            .and_then(|entry| entry.get("resource"))
            .and_then(Value::as_object)
            .ok_or_else(|| BindingError::Unreadable {
                location: location.to_owned(),
            })?;
        let mut check = check_required(bundle, root, "Bundle", expansions)?;
        let entry = check_required(composition, first, location, expansions)?;
        check.findings.extend(entry.findings);
        check.unevaluated.extend(entry.unevaluated);
        check.pending.extend(entry.pending);
        check.unevaluated.sort();
        check.unevaluated.dedup();
        Ok(check)
    }
}

/// Returns whether the expansion of `url` holds `value`.
fn held(expansions: &Expansions, url: &str, value: &Coded<'_>) -> bool {
    let coding_held = |coding: &Object| {
        let system = coding.get("system").and_then(|system| system.as_str());
        coding
            .get("code")
            .and_then(|code| code.as_str())
            .is_some_and(|code| {
                system.is_some() && expansions.contains(url, system, code) == Some(true)
            })
    };
    match value {
        Coded::Code(code) => expansions.contains(url, None, code) == Some(true),
        Coded::Coding(coding) => coding_held(coding),
        Coded::Concept(concept) => concept
            .get("coding")
            .and_then(|codings| codings.as_array())
            .unwrap_or_default()
            .iter()
            .filter_map(|coding| coding.as_object())
            .any(coding_held),
    }
}

/// Returns the concept `$validate-code` is asked about for `occurrence`.
fn concept(
    profile: &str,
    element: &str,
    occurrence: &Occurrence<'_>,
) -> Result<Candidate, BindingError> {
    let decode_error = |source| BindingError::Decode {
        location: occurrence.location.clone(),
        source,
    };
    Ok(match occurrence.value {
        Coded::Code(code) => Candidate::Code {
            code: code.to_owned(),
            context: format!("{profile}#{element}"),
        },
        Coded::Coding(coding) => Candidate::Coding(Box::new(
            Coding::from_json(coding, &mut Path::root("Coding")).map_err(decode_error)?,
        )),
        Coded::Concept(concept) => Candidate::Concept(Box::new(
            CodeableConcept::from_json(concept, &mut Path::root("CodeableConcept"))
                .map_err(decode_error)?,
        )),
    })
}
