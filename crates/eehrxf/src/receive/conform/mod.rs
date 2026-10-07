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
//! of a type-discriminated slicing names is refused, and the root invariants
//! of one form, a reference every entry of listed types carries, are
//! evaluated. Other invariants and terminology bindings are not checked
//! here. [`check_resource`] holds any one resource, such as an entry of a
//! document, to a profile by the same rules.

mod values;
mod walk;

use std::fmt;

use fhir_types::codec::{Object, Value};

use crate::dataset::ResourceProfile;
use crate::dataset::constraint::DiscriminatorKind;
use crate::receive::ReceivedDocument;
use crate::receive::conform::walk::Walk;

// TODO(#808): hold the document to the other invariants and the terminology
// bindings of its profiles and to the profile each other entry claims.
impl ReceivedDocument {
    /// Checks the document against `bundle`, a profile of `Bundle`, and its
    /// `Composition` against `composition`, a profile of `Composition`.
    ///
    /// # Errors
    ///
    /// Returns [`CheckError::ProfileType`] when a profile constrains another
    /// resource type, and [`CheckError::NonConformant`] listing every finding
    /// of both profiles when the document breaks one.
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
            .ok_or(CheckError::NoComposition)?;
        let mut walk = Walk::default();
        walk.profile(bundle, "Bundle", root)?;
        walk.profile(composition, "Composition", first)?;
        walk.finish()
    }
}

/// Checks `resource`, a resource of type `expected` in FHIR JSON, against
/// `profile` by the rules [`ReceivedDocument::check`] applies to a
/// document's `Bundle` and `Composition`: the profile an entry of a document
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
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pattern(key) => write!(f, "the {key} value"),
            Self::Discriminator { kind, path } => write!(f, "the {kind} discriminator at {path}"),
            Self::Order => f.write_str("the slice order"),
            Self::Invariant(key) => write!(f, "the invariant {key}"),
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
    /// slicing names.
    TypeNotAllowed,
    /// The document breaks an invariant of the profile, by its key.
    Invariant {
        /// The invariant's key.
        key: String,
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
    /// The document's tree carries no `Composition` as its first entry.
    #[error("the document carries no Composition to check")]
    NoComposition,
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
