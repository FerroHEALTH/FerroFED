// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The constraints an element of a profile puts on an instance beyond its
//! cardinality: a required value and a slicing.
//!
//! A profile names the value an element must carry with `fixed[x]` (the
//! instance equals it) or `pattern[x]` (the instance carries at least it),
//! and divides a repeating element into slices with `slicing`
//! (<https://hl7.org/fhir/R4/elementdefinition.html>,
//! <https://hl7.org/fhir/R4/profiling.html#slicing>). The forms read here
//! are the ones a document check evaluates; any other `fixed[x]` or
//! `pattern[x]` is kept as [`Pattern::Unread`] under its key, so a reader
//! can say it was not evaluated.

use std::fmt;

/// A value an element must carry, read from its `fixed[x]` or `pattern[x]`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Pattern {
    /// A primitive whose JSON form is a string (`fixedUri`, `patternCode`,
    /// `fixedString` and the other string-valued primitives); fixed and
    /// pattern mean the same for a primitive, so the instance equals it.
    Primitive(String),
    /// A `patternCodeableConcept`: the instance carries a coding matching
    /// each coding listed, and the text when one is given.
    Concept {
        /// The codings the instance must carry.
        codings: Vec<CodingPattern>,
        /// The text the instance must carry, when the pattern gives one.
        text: Option<String>,
    },
    /// A `patternCoding`: the instance matches the coding.
    Coding(CodingPattern),
    /// A `fixed[x]` or `pattern[x]` form this model does not read, by its
    /// key (`fixedCodeableConcept`, `patternQuantity`).
    Unread {
        /// The key the snapshot carries, such as `patternQuantity`.
        key: String,
    },
}

/// The members of a `Coding` a pattern can require; an absent member is not
/// required.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodingPattern {
    pub(super) system: Option<String>,
    pub(super) version: Option<String>,
    pub(super) code: Option<String>,
    pub(super) display: Option<String>,
}

impl CodingPattern {
    /// Returns the code system the coding must carry, when it requires one.
    #[must_use]
    pub fn system(&self) -> Option<&str> {
        self.system.as_deref()
    }

    /// Returns the code system version the coding must carry, when it
    /// requires one.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// Returns the code the coding must carry, when it requires one.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    /// Returns the display the coding must carry, when it requires one.
    #[must_use]
    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

/// How a repeating element is divided into slices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slicing {
    pub(super) discriminators: Vec<Discriminator>,
    pub(super) ordered: bool,
    pub(super) rules: SlicingRules,
}

impl Slicing {
    /// Returns the discriminators that tell the slices apart, in snapshot
    /// order.
    #[must_use]
    pub fn discriminators(&self) -> &[Discriminator] {
        &self.discriminators
    }

    /// Returns whether the slices must occur in the order the snapshot lists
    /// them.
    #[must_use]
    pub const fn ordered(&self) -> bool {
        self.ordered
    }

    /// Returns whether an occurrence that matches no slice is admitted.
    #[must_use]
    pub const fn rules(&self) -> SlicingRules {
        self.rules
    }
}

/// One discriminator of a slicing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discriminator {
    pub(super) kind: DiscriminatorKind,
    pub(super) path: String,
}

impl Discriminator {
    /// Returns how the discriminator's path tells the slices apart.
    #[must_use]
    pub const fn kind(&self) -> DiscriminatorKind {
        self.kind
    }

    /// Returns the path, relative to the sliced element, the discriminator
    /// reads (`code`, `url`, `$this`, `resource`).
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// The R4 discriminator types
/// (<https://hl7.org/fhir/R4/valueset-discriminator-type.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DiscriminatorKind {
    /// `value`: the slices carry different fixed values at the path.
    Value,
    /// `exists`: the slices differ by whether the path is present.
    Exists,
    /// `pattern`: the slices carry different patterns at the path.
    Pattern,
    /// `type`: the slices differ by the type at the path.
    Type,
    /// `profile`: the slices differ by the profile the path conforms to.
    Profile,
}

impl DiscriminatorKind {
    /// Returns the kind a discriminator `type` code names, or `None` for a
    /// code outside the value set.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "value" => Some(Self::Value),
            "exists" => Some(Self::Exists),
            "pattern" => Some(Self::Pattern),
            "type" => Some(Self::Type),
            "profile" => Some(Self::Profile),
            _ => None,
        }
    }
}

impl fmt::Display for DiscriminatorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Value => "value",
            Self::Exists => "exists",
            Self::Pattern => "pattern",
            Self::Type => "type",
            Self::Profile => "profile",
        })
    }
}

/// The R4 slicing rules
/// (<https://hl7.org/fhir/R4/valueset-resource-slicing-rules.html>).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlicingRules {
    /// `closed`: every occurrence belongs to a slice.
    Closed,
    /// `open`: an occurrence may belong to no slice, anywhere.
    Open,
    /// `openAtEnd`: an occurrence may belong to no slice, after the slices.
    OpenAtEnd,
}

impl SlicingRules {
    /// Returns the rules a `slicing.rules` code names, or `None` for a code
    /// outside the value set.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "closed" => Some(Self::Closed),
            "open" => Some(Self::Open),
            "openAtEnd" => Some(Self::OpenAtEnd),
            _ => None,
        }
    }
}

/// One invariant of an element: an `ElementDefinition.constraint`
/// (<https://hl7.org/fhir/R4/elementdefinition-definitions.html#ElementDefinition.constraint>).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invariant {
    pub(super) key: String,
    pub(super) error: bool,
    pub(super) expression: Option<String>,
}

impl Invariant {
    /// Returns the invariant's key, such as `bdl-9`.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns whether breaking the invariant is an error, its severity
    /// `error`, rather than a warning.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.error
    }

    /// Returns the `FHIRPath` expression of the invariant, when it carries
    /// one.
    #[must_use]
    pub fn expression(&self) -> Option<&str> {
        self.expression.as_deref()
    }
}
