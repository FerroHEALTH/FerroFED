// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document held to its profiles in full by FerroBRIDGE's FHIR R4
//! profile validator, `ferrobridge-validator`.
//!
//! [`ReceivedDocument::validate`] validates the document's `Bundle` against
//! the `Bundle` profile of its category, over a package set that holds that
//! profile and its whole dependency closure. The validator evaluates every
//! constraint the snapshots carry on every element in play: cardinality,
//! types, `fixed[x]` and `pattern[x]`, slicing with all five discriminator
//! kinds, every invariant of the core definitions and of each profile, the
//! terminology bindings, the target profile of each reference, the profile
//! each entry's slice names and each profile a resource claims in
//! `meta.profile`, and the content rules R4 gives a document
//! (<https://hl7.org/fhir/R4/validation.html>,
//! <https://hl7.org/fhir/R4/profiling.html>,
//! <https://hl7.org/fhir/R4/documents.html>). A binding the package set
//! cannot decide is asked of the terminology server when one is given, and
//! stays listed as not evaluated when none is. A `required` binding the
//! server refused or did not answer refuses the receipt, so an upstream
//! failure never reads as a pass.
//!
//! An issue of severity `error` or `fatal` refuses the document; every other
//! issue is a [`Finding`] the receipt reports as a warning, and every
//! constraint the validator met and did not evaluate is an [`Unevaluated`],
//! so nothing is passed silently. No message quotes a value of the
//! document: the validator's messages name paths and the values a profile
//! expects.

use ferrobridge_validator::issue::{Issue, Rule, Severity};
use ferrobridge_validator::report::{Constraint, Reason, Report};
use ferrobridge_validator::set::PackageSet;
use ferrobridge_validator::terminology::Terminology;
use ferrobridge_validator::validate::{ValidateError, Validator};
use fhir_types::codec::Value;
use http::StatusCode;

use crate::receive::ReceivedDocument;
use crate::receive::conform::{Finding, FindingKind, Unevaluated, Unread};

impl ReceivedDocument {
    /// Validates the document against `profile`, the canonical of its
    /// category's `Bundle` profile, over `set`, asking `terminology` the
    /// bindings the set cannot decide.
    ///
    /// # Errors
    ///
    /// [`ValidationError::NonConformant`] listing every issue of severity
    /// `error` or `fatal`, [`ValidationError::Terminology`] for a `required`
    /// binding `terminology` refused or did not answer, and
    /// [`ValidationError::Validator`] when the set
    /// cannot answer: a definition the validation reaches does not resolve,
    /// carries no snapshot, or cannot be read.
    pub async fn validate<T: Terminology + ?Sized + Sync>(
        &self,
        set: &PackageSet,
        profile: &str,
        terminology: Option<&T>,
    ) -> Result<Validated, ValidationError> {
        let document = Value::Object(self.tree().clone());
        let mut report = Validator::new(set)
            .validate_against(&document, profile)
            .map_err(|source| ValidationError::Validator {
                profile: profile.to_owned(),
                source: Box::new(source),
            })?;
        if let Some(terminology) = terminology {
            report.resolve_bindings(terminology).await;
        }
        validated(&report, profile)
    }
}

/// What a document that passed the validation still says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validated {
    cautions: Vec<Finding>,
    unevaluated: Vec<Unevaluated>,
}

impl Validated {
    /// Returns every issue of severity `warning` or `information`, in the
    /// order the validator found them.
    #[must_use]
    pub fn cautions(&self) -> &[Finding] {
        &self.cautions
    }

    /// Returns every constraint the validator met and did not evaluate,
    /// once per declaring profile, element and constraint.
    #[must_use]
    pub fn unevaluated(&self) -> &[Unevaluated] {
        &self.unevaluated
    }
}

/// Why a document did not pass the validation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ValidationError {
    /// The document breaks its profiles.
    #[error("the document breaks its profiles in {} places", findings.len())]
    NonConformant {
        /// Every issue of severity `error` or `fatal`, in the order found.
        findings: Vec<Finding>,
    },
    /// The terminology server refused, or did not answer, the check of a
    /// `required` binding the package set cannot decide.
    #[error("the terminology server did not check a required binding ({kind})")]
    Terminology {
        /// The failure class, such as `server-failure` or `refused`.
        kind: &'static str,
        /// The status the server answered with, when it answered.
        status: Option<StatusCode>,
    },
    /// The package set cannot answer the validation.
    #[error("the package set cannot validate a document against {profile}")]
    Validator {
        /// The `Bundle` profile the document was validated against.
        profile: String,
        /// Why.
        #[source]
        source: Box<ValidateError>,
    },
}

/// The outcome of `report`, a validation against `profile`.
fn validated(report: &Report, profile: &str) -> Result<Validated, ValidationError> {
    let (refusals, cautions): (Vec<&Issue>, Vec<&Issue>) = report
        .issues()
        .iter()
        .partition(|issue| matches!(issue.severity, Severity::Error | Severity::Fatal));
    if !refusals.is_empty() {
        return Err(ValidationError::NonConformant {
            findings: refusals
                .into_iter()
                .map(|issue| finding(issue, profile))
                .collect(),
        });
    }
    // NOTE: FHIR R4 terminologies.html §4.1.5.1: a `required` value SHALL be in the value
    // set, so a check the server did not answer leaves the receipt undecided.
    if let Some((kind, status)) = report.unevaluated().iter().find_map(|unevaluated| {
        let required = match &unevaluated.constraint {
            Constraint::Binding { strength, .. } => strength == "required",
            Constraint::MaxValueSet { .. } => true,
            _ => false,
        };
        match unevaluated.reason {
            Reason::TerminologyFailed { kind, status } if required => Some((kind, status)),
            _ => None,
        }
    }) {
        return Err(ValidationError::Terminology { kind, status });
    }
    Ok(Validated {
        cautions: cautions
            .into_iter()
            .map(|issue| finding(issue, profile))
            .collect(),
        unevaluated: report
            .unevaluated()
            .iter()
            .map(|unevaluated| Unevaluated {
                profile: unevaluated.structure.clone(),
                element: unevaluated.element.clone(),
                reason: Unread::Constraint {
                    constraint: described(&unevaluated.constraint),
                    reason: unevaluated.reason.to_string(),
                },
            })
            .collect(),
    })
}

/// The finding of `issue`, found validating against `profile`: a constraint
/// names the profile that declares it, and a message of the validator the
/// profile validated against.
fn finding(issue: &Issue, profile: &str) -> Finding {
    let (profile, element) = match &issue.rule {
        Rule::Constraint { key, source } => (source.clone(), key.clone()),
        Rule::Message(id) => (profile.to_owned(), id.code().to_owned()),
    };
    Finding {
        profile,
        element,
        location: issue.expression.clone(),
        kind: FindingKind::Issue {
            severity: issue.severity.code(),
            code: issue.code.code(),
            message: issue.message.clone(),
        },
    }
}

/// The words that name `constraint` in a warning.
fn described(constraint: &Constraint) -> String {
    match constraint {
        Constraint::Slicing => "the slicing".to_owned(),
        Constraint::Discriminator { kind, path } => format!("the {kind} discriminator {path}"),
        Constraint::DiscriminatorValueSet { path, value_set } => {
            format!("the discriminator {path} over the value set {value_set}")
        }
        Constraint::Invariant { key, .. } => format!("the invariant {key}"),
        Constraint::Binding {
            strength,
            value_set,
        } => value_set.as_ref().map_or_else(
            || format!("the {strength} binding"),
            |value_set| format!("the {strength} binding to {value_set}"),
        ),
        Constraint::MaxValueSet { value_set } => format!("the maximum value set {value_set}"),
        Constraint::TypeProfile { profiles } => {
            format!("the type profiles {}", profiles.join(", "))
        }
        Constraint::TargetProfile { profiles } => {
            format!("the target profiles {}", profiles.join(", "))
        }
        Constraint::Aggregation { modes } => format!("the aggregation {}", modes.join(", ")),
        Constraint::Versioning { rule } => format!("the versioning {rule}"),
        Constraint::ClaimedProfile { url } => format!("the claimed profile {url}"),
        Constraint::ExtensionContext => "the extension context".to_owned(),
        Constraint::ContextInvariant { expression } => {
            format!("the context invariant {expression}")
        }
    }
}
