// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A received document checked against the profiles of its category.
//!
//! [`ReceivedDocument::check`] walks the snapshot of a `Bundle` profile over
//! the document and the snapshot of a `Composition` profile over its first
//! entry (<https://hl7.org/fhir/R4/profiling.html>), and evaluates, at every
//! occurrence of every element the snapshot lists:
//!
//! - the cardinality, per occurrence of the parent element, a primitive
//!   carried only by its `_` extension sibling counting as present
//!   (<https://hl7.org/fhir/R4/json.html#primitive>);
//! - a `pattern[x]` or `fixed[x]` value the model reads ([`Pattern`](crate::dataset::constraint::Pattern));
//! - a slicing whose discriminators are `value` or `pattern` over a value the
//!   slice fixes, `exists`, or `type` over a contained resource: each
//!   occurrence is assigned to the slices it matches, each slice's
//!   cardinality is held, and a `closed` slicing refuses an occurrence that
//!   matches none.
//!
//! What the check cannot evaluate it says: a discriminator of kind
//! `profile`, a discriminator path through a function (`resolve()`), an
//! ordered slicing and a pattern form the model does not read are listed in
//! the [`Conformance`], never passed silently. A slice one of them
//! discriminates is still held to its lower bound over the occurrences the
//! other discriminators admit, because an occurrence they refuse belongs to
//! it under no reading, and the contents of a slice are checked over the
//! occurrences that certainly belong to it. An occurrence of a type no slice
//! of a type-discriminated slicing names is refused where the slicing is
//! `closed`, or `openAtEnd` and the occurrence comes before a named one, and
//! admitted where it is `open`. The root invariants
//! of one form, a reference every entry of listed types carries, are
//! evaluated, and every other invariant is listed as not evaluated. The
//! walk holds no terminology binding: [`ReceivedDocument::check_bindings`]
//! and [`check_required`](crate::terminology::bindings::check_required) hold
//! the `required` ones, and what they find or cannot evaluate converts into
//! a [`Finding`] or an [`Unevaluated`] of this module. [`check_resource`]
//! holds any one resource, such as an entry of a document, to a profile by
//! the same rules.
//!
//! A received document is held to every constraint of its profiles and of
//! the profiles its entries claim by FerroBRIDGE's validator
//! ([`ReceivedDocument::validate`](crate::receive::ReceivedDocument::validate)),
//! whose issues and unevaluated constraints convert into the same
//! [`Finding`] and [`Unevaluated`].

mod values;
mod walk;

use std::fmt;

use fhir_types::codec::{Object, Value};

use crate::dataset::ResourceProfile;
use crate::dataset::constraint::DiscriminatorKind;
use crate::receive::ReceivedDocument;
use crate::receive::conform::walk::Walk;
use crate::terminology::bindings::BindingFinding;
use crate::terminology::bindings::BindingGap;
use crate::terminology::bindings::UnevaluatedBinding;

impl ReceivedDocument {
    /// Checks the document against `bundle`, a profile of `Bundle`, and its
    /// `Composition` against `composition`, a profile of `Composition`.
    ///
    /// # Panics
    ///
    /// Never: every `ReceivedDocument` carries its `Composition` as its first
    /// entry, because reading one refuses a document that does not.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::ProfileType`] when a profile constrains another
    /// resource type, and [`CheckError::NonConformant`] listing every finding
    /// of both profiles when the document breaks one.
    #[expect(
        clippy::expect_used,
        reason = "ReceivedDocument::read and the rebuild after displays are its only constructors, \
                  and both refuse a document whose first entry is no Composition"
    )]
    pub fn check(
        &self,
        bundle: &ResourceProfile,
        composition: &ResourceProfile,
    ) -> Result<Conformance, CheckError> {
        let root = self.tree();
        let first = root
            .get("entry")
            .and_then(Value::as_array)
            .and_then(<[Value]>::first)
            .and_then(|entry| entry.get("resource"))
            .and_then(Value::as_object)
            .expect("a received document should carry its Composition as its first entry");
        let mut walk = Walk::default();
        walk.profile(bundle, "Bundle", root)?;
        walk.profile(composition, "Composition", first)?;
        walk.finish()
    }
}

/// Checks `resource`, a resource of type `expected` in FHIR JSON, against
/// `profile`.
///
/// The rules are those [`ReceivedDocument::check`] applies to a document's
/// `Bundle` and `Composition`; the profile is one an entry of a document
/// claims in its `meta.profile`, or one its slice names.
///
/// # Errors
///
/// Returns [`CheckError::ProfileType`] when `profile` constrains another
/// resource type, and [`CheckError::NonConformant`] listing every finding
/// when the resource breaks it.
pub fn check_resource(
    profile: &ResourceProfile,
    expected: &'static str,
    resource: &Object,
) -> Result<Conformance, CheckError> {
    let mut walk = Walk::default();
    walk.profile(profile, expected, resource)?;
    walk.finish()
}

impl Walk {
    /// The conformance of a walk with no finding, or the findings it made.
    fn finish(self) -> Result<Conformance, CheckError> {
        if self.findings.is_empty() {
            Ok(Conformance {
                unevaluated: self.unevaluated.into_iter().collect(),
            })
        } else {
            Err(CheckError::NonConformant {
                findings: self.findings,
            })
        }
    }
}

/// What a passing check could not evaluate.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conformance {
    unevaluated: Vec<Unevaluated>,
}

impl Conformance {
    /// Returns every constraint the check could not evaluate, ordered by
    /// profile and element.
    #[must_use]
    pub fn unevaluated(&self) -> &[Unevaluated] {
        &self.unevaluated
    }

    /// Returns the conformance of a check that lists `unevaluated` and
    /// found nothing.
    #[cfg(any(feature = "prescription", feature = "dispensation"))]
    pub(crate) fn listing(unevaluated: impl IntoIterator<Item = Unevaluated>) -> Self {
        Self::default().join(Self {
            unevaluated: unevaluated.into_iter().collect(),
        })
    }

    /// Returns the conformance of two checks that both passed: what either
    /// could not evaluate, ordered by profile and element, each once.
    #[cfg(any(feature = "prescription", feature = "dispensation"))]
    pub(crate) fn join(mut self, other: Self) -> Self {
        self.unevaluated.extend(other.unevaluated);
        self.unevaluated.sort();
        self.unevaluated.dedup();
        self
    }
}

/// One constraint of a profile the check did not evaluate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Unevaluated {
    /// The profile's canonical URL.
    pub profile: String,
    /// The element id that carries the constraint.
    pub element: String,
    /// Which constraint it is.
    pub reason: Unread,
}

/// The kind of a constraint the check does not evaluate.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Unread {
    /// A `fixed[x]` or `pattern[x]` form the model does not read, by key.
    Pattern(String),
    /// A slicing discriminator of this kind over this path.
    Discriminator {
        /// The discriminator kind.
        kind: DiscriminatorKind,
        /// The path it reads.
        path: String,
    },
    /// The order an ordered slicing asks of its slices.
    Order,
    /// An invariant of the profile, by its key.
    Invariant(String),
    /// A constraint FerroBRIDGE's validator met and did not evaluate.
    Constraint {
        /// The constraint, in words, such as `the invariant ips-comp-1`.
        constraint: String,
        /// Why the validator did not evaluate it.
        reason: String,
    },
    /// A `required` binding, to the value set it names, and why it was not
    /// evaluated.
    Binding {
        /// The canonical URL of the value set, when the binding names one.
        value_set: Option<String>,
        /// Why it was not evaluated.
        gap: BindingGap,
    },
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pattern(key) => write!(f, "the {key} value"),
            Self::Discriminator { kind, path } => write!(f, "the {kind} discriminator at {path}"),
            Self::Order => f.write_str("the slice order"),
            Self::Invariant(key) => write!(f, "the invariant {key}"),
            Self::Constraint { constraint, reason } => write!(f, "{constraint} ({reason})"),
            Self::Binding { value_set, gap } => {
                let value_set = value_set.as_deref().unwrap_or("no value set");
                match gap {
                    BindingGap::NoExpansion => write!(
                        f,
                        "the required binding to {value_set}, of which no expansion is read"
                    ),
                    BindingGap::NoValueSet => {
                        f.write_str("the required binding, which names no value set")
                    }
                    BindingGap::Sliced => {
                        write!(f, "the required binding to {value_set} inside a slice")
                    }
                    BindingGap::Type(code) => write!(
                        f,
                        "the required binding to {value_set} on an element of type {code}"
                    ),
                }
            }
        }
    }
}

impl From<UnevaluatedBinding> for Unevaluated {
    fn from(binding: UnevaluatedBinding) -> Self {
        Self {
            profile: binding.profile,
            element: binding.element,
            reason: Unread::Binding {
                value_set: binding.value_set,
                gap: binding.gap,
            },
        }
    }
}

impl From<BindingFinding> for Finding {
    fn from(finding: BindingFinding) -> Self {
        Self {
            profile: finding.profile,
            element: finding.element,
            location: finding.location,
            kind: FindingKind::NotInValueSet {
                value_set: finding.value_set,
            },
        }
    }
}

/// One way a document breaks a profile.
///
/// A finding names where it is and what the profile asks, and never quotes
/// a value of the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The profile's canonical URL.
    pub profile: String,
    /// The element id the finding is about.
    pub element: String,
    /// Where in the document, as an element path with indexes, such as
    /// `Composition.section[2]`.
    pub location: String,
    /// What the profile asks that the document does not do.
    pub kind: FindingKind,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}: {}", self.element, self.location, self.kind)
    }
}

/// What a finding reports.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FindingKind {
    /// Fewer occurrences than the element's lower bound.
    TooFew {
        /// The lower bound.
        min: u32,
        /// How many the document carries.
        found: usize,
    },
    /// More occurrences than the element's upper bound.
    TooMany {
        /// The upper bound.
        max: u32,
        /// How many the document carries.
        found: usize,
    },
    /// The value differs from the element's `fixed[x]` or `pattern[x]`.
    Pattern,
    /// The occurrence matches no slice of a `closed` slicing.
    NoSlice,
    /// The occurrence is of a resource type no slice of a type-discriminated
    /// slicing names, where the slicing's rules admit no such occurrence.
    TypeNotAllowed,
    /// The document breaks an invariant of the profile, by its key.
    Invariant {
        /// The invariant's key.
        key: String,
    },
    /// The concept is not in the value set a `required` binding names.
    NotInValueSet {
        /// The canonical URL of the value set.
        value_set: String,
    },
    /// An issue FerroBRIDGE's validator found.
    Issue {
        /// Its R4 `issue-severity` code.
        severity: &'static str,
        /// Its R4 `issue-type` code.
        code: &'static str,
        /// The validator's message, which names paths and the values a
        /// profile expects, never a value of the document.
        message: String,
    },
}

impl fmt::Display for FindingKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFew { min, found } => {
                write!(f, "{found} occurrences, at least {min} required")
            }
            Self::TooMany { max, found } => write!(f, "{found} occurrences, at most {max} allowed"),
            Self::Pattern => f.write_str("the value differs from the profile's required value"),
            Self::NoSlice => f.write_str("the occurrence matches no slice of a closed slicing"),
            Self::TypeNotAllowed => f.write_str("the resource type is one no slice names"),
            Self::Invariant { key } => write!(f, "the invariant {key} does not hold"),
            Self::NotInValueSet { value_set } => {
                write!(
                    f,
                    "the concept is not in the required value set {value_set}"
                )
            }
            Self::Issue { message, .. } => f.write_str(message),
        }
    }
}

/// Why a check gives no [`Conformance`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CheckError {
    /// A profile constrains another resource type than the one it was given
    /// for.
    #[error("the profile {url} does not constrain {expected}")]
    ProfileType {
        /// The profile's canonical URL.
        url: String,
        /// The resource type it was given for.
        expected: &'static str,
    },
    /// A coded element the R4 element table does not describe, or that does
    /// not decode as its type, so its bindings cannot be read.
    #[error("{location} is a coded element whose bindings cannot be read")]
    Unreadable {
        /// The element path.
        location: String,
    },
    /// The document breaks a profile.
    #[error("the document breaks its profiles in {} places: {}", findings.len(), Listed(findings))]
    NonConformant {
        /// Every finding, in walk order.
        findings: Vec<Finding>,
    },
}

/// Renders a finding list on one line.
struct Listed<'a>(&'a [Finding]);

impl fmt::Display for Listed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (position, finding) in self.0.iter().enumerate() {
            if position > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{finding}")?;
        }
        Ok(())
    }
}
