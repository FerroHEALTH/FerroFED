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
//! - a `pattern[x]` or `fixed[x]` value the model reads ([`Pattern`]);
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
//! occurrences that certainly belong to it. Invariants, terminology bindings
//! and the profiles of the other entries are not checked here.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt;

use fhir_types::codec::Object;
use fhir_types::codec::Value;

use crate::dataset::Element;
use crate::dataset::Max;
use crate::dataset::ResourceProfile;
use crate::dataset::constraint::CodingPattern;
use crate::dataset::constraint::DiscriminatorKind;
use crate::dataset::constraint::Pattern;
use crate::dataset::constraint::Slicing;
use crate::dataset::constraint::SlicingRules;
use crate::receive::ReceivedDocument;

// TODO(#808): hold the document to the invariants and terminology bindings of
// its profiles and to the profile each other entry claims.
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
        if walk.findings.is_empty() {
            Ok(Conformance {
                unevaluated: walk.unevaluated.into_iter().collect(),
            })
        } else {
            Err(CheckError::NonConformant {
                findings: walk.findings,
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
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pattern(key) => write!(f, "the {key} value"),
            Self::Discriminator { kind, path } => write!(f, "the {kind} discriminator at {path}"),
            Self::Order => f.write_str("the slice order"),
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

/// One occurrence of an element in the document: its location and its
/// value, absent when only its `_` sibling carries it.
type Occurrence<'d> = (String, Option<&'d Value>);

/// The state of one check over one or more profiles.
#[derive(Debug, Default)]
struct Walk {
    findings: Vec<Finding>,
    unevaluated: BTreeSet<Unevaluated>,
}

/// One profile's snapshot, its elements grouped under their parent id.
struct Snapshot<'p> {
    url: &'p str,
    children: BTreeMap<&'p str, Vec<&'p Element>>,
    by_id: BTreeMap<&'p str, &'p Element>,
}

impl Walk {
    /// Walks `profile` over `root`, a resource of type `expected`.
    fn profile(
        &mut self,
        profile: &ResourceProfile,
        expected: &'static str,
        root: &Object,
    ) -> Result<(), CheckError> {
        let wrong = || CheckError::ProfileType {
            url: profile.url().to_owned(),
            expected,
        };
        let top = profile.elements().first().ok_or_else(wrong)?;
        if top.path().as_str() != expected {
            return Err(wrong());
        }
        let mut children: BTreeMap<&str, Vec<&Element>> = BTreeMap::new();
        let mut by_id = BTreeMap::new();
        for element in profile.elements() {
            let id = element.path().as_str();
            by_id.insert(id, element);
            if let Some((parent, _)) = id.rsplit_once('.') {
                children.entry(parent).or_default().push(element);
            }
        }
        let snapshot = Snapshot {
            url: profile.url(),
            children,
            by_id,
        };
        self.children(&snapshot, expected, &[(expected.to_owned(), root)]);
        Ok(())
    }

    /// Checks every child element of `parent` over each of its occurrences.
    fn children(&mut self, snapshot: &Snapshot<'_>, parent: &str, nodes: &[(String, &Object)]) {
        let Some(children) = snapshot.children.get(parent) else {
            return;
        };
        for element in children {
            let id = element.path().as_str();
            let name = last_segment(id);
            if name.contains(':') {
                continue;
            }
            let mut every: Vec<Occurrence<'_>> = Vec::new();
            let mut sliced: BTreeMap<&str, Vec<(String, &Object)>> = BTreeMap::new();
            for (location, node) in nodes {
                let found = occurrences(node, location, name, element.types());
                self.bounds(snapshot, element, location, found.len());
                if let Some(pattern) = element.pattern() {
                    for (at, value) in &found {
                        self.pattern(snapshot, element, pattern, at, *value);
                    }
                }
                if let Some(slicing) = element.slicing() {
                    for (slice, members) in
                        self.slices(snapshot, parent, element, slicing, location, &found)
                    {
                        sliced.entry(slice).or_default().extend(members);
                    }
                }
                every.extend(found);
            }
            self.children(snapshot, id, &objects(&every));
            for (slice, members) in &sliced {
                self.children(snapshot, slice, members);
            }
        }
    }

    /// Holds `found` occurrences of `element` under one parent to its
    /// cardinality.
    fn bounds(&mut self, snapshot: &Snapshot<'_>, element: &Element, location: &str, found: usize) {
        let cardinality = element.cardinality();
        let min = cardinality.lower();
        let kind = if found < usize::try_from(min).unwrap_or(usize::MAX) {
            Some(FindingKind::TooFew { min, found })
        } else {
            match cardinality.upper() {
                Max::Bounded(max) if found > usize::try_from(max).unwrap_or(usize::MAX) => {
                    Some(FindingKind::TooMany { max, found })
                }
                _ => None,
            }
        };
        if let Some(kind) = kind {
            self.find(snapshot, element, location, kind);
        }
    }

    /// Holds one occurrence to the element's required value.
    fn pattern(
        &mut self,
        snapshot: &Snapshot<'_>,
        element: &Element,
        pattern: &Pattern,
        location: &str,
        value: Option<&Value>,
    ) {
        match matches(pattern, value) {
            Some(true) => {}
            Some(false) => self.find(snapshot, element, location, FindingKind::Pattern),
            None => self.unread(snapshot, element, unread_pattern(pattern)),
        }
    }

    /// Assigns the occurrences of a sliced element under one parent to its
    /// slices, holds each slice to its cardinality, and returns, per slice
    /// id, the object occurrences that certainly belong to it.
    fn slices<'p, 'd>(
        &mut self,
        snapshot: &Snapshot<'p>,
        parent: &str,
        element: &Element,
        slicing: &Slicing,
        location: &str,
        found: &[Occurrence<'d>],
    ) -> Vec<(&'p str, Vec<(String, &'d Object)>)> {
        if slicing.ordered() {
            self.unread(snapshot, element, Unread::Order);
        }
        let prefix = format!("{}:", last_segment(element.path().as_str()));
        let slices: Vec<&'p Element> = snapshot
            .children
            .get(parent)
            .into_iter()
            .flatten()
            .copied()
            .filter(|slice| {
                last_segment(slice.path().as_str())
                    .strip_prefix(&prefix)
                    .is_some_and(|name| !name.contains('/'))
            })
            .collect();
        let mut matched = vec![false; found.len()];
        let mut every_exact = true;
        let mut members = Vec::with_capacity(slices.len());
        for slice in slices {
            let mut admitted = 0_usize;
            let mut exact = true;
            let mut certain = Vec::new();
            for (position, (at, value)) in found.iter().enumerate() {
                match self.discriminate(snapshot, element, slicing, slice, *value) {
                    Fit::No => {}
                    Fit::Yes => {
                        admitted += 1;
                        if let Some(flag) = matched.get_mut(position) {
                            *flag = true;
                        }
                        certain.push((at.clone(), *value));
                    }
                    Fit::Possibly => {
                        admitted += 1;
                        exact = false;
                    }
                }
            }
            let min = slice.cardinality().lower();
            if admitted < usize::try_from(min).unwrap_or(usize::MAX) {
                self.find(
                    snapshot,
                    slice,
                    location,
                    FindingKind::TooFew {
                        min,
                        found: admitted,
                    },
                );
            } else if exact {
                self.bounds(snapshot, slice, location, admitted);
            }
            every_exact &= exact;
            members.push((slice.path().as_str(), objects(&certain)));
        }
        if every_exact && slicing.rules() == SlicingRules::Closed {
            for ((at, _), hit) in found.iter().zip(&matched) {
                if !hit {
                    self.find(snapshot, element, at, FindingKind::NoSlice);
                }
            }
        }
        members
    }

    /// Returns whether `value` belongs to `slice` by every discriminator of
    /// `slicing`.
    fn discriminate(
        &mut self,
        snapshot: &Snapshot<'_>,
        element: &Element,
        slicing: &Slicing,
        slice: &Element,
        value: Option<&Value>,
    ) -> Fit {
        let mut fit = Fit::Yes;
        for discriminator in slicing.discriminators() {
            let path = discriminator.path();
            let unread = || Unread::Discriminator {
                kind: discriminator.kind(),
                path: path.to_owned(),
            };
            let readable = !path.contains('(');
            let target = if path == "$this" {
                Some(slice)
            } else {
                snapshot
                    .by_id
                    .get(format!("{}.{path}", slice.path().as_str()).as_str())
                    .copied()
            };
            let at = if path == "$this" {
                value
            } else {
                value.and_then(|value| follow(value, path))
            };
            let verdict = match (discriminator.kind(), readable, target) {
                (DiscriminatorKind::Value | DiscriminatorKind::Pattern, true, Some(target)) => {
                    target.pattern().and_then(|pattern| matches(pattern, at))
                }
                (DiscriminatorKind::Exists, true, Some(target)) => {
                    let present = at.is_some();
                    if target.cardinality().is_required() {
                        Some(present)
                    } else if target.cardinality().upper() == Max::Bounded(0) {
                        Some(!present)
                    } else {
                        None
                    }
                }
                (DiscriminatorKind::Type, true, Some(target)) => {
                    let kind = at
                        .and_then(|at| at.get("resourceType"))
                        .and_then(Value::as_str);
                    kind.map(|kind| target.types().iter().any(|code| code == kind))
                }
                _ => None,
            };
            match verdict {
                Some(true) => {}
                Some(false) => return Fit::No,
                None => {
                    self.unread(snapshot, element, unread());
                    fit = Fit::Possibly;
                }
            }
        }
        fit
    }

    /// Records a finding.
    fn find(
        &mut self,
        snapshot: &Snapshot<'_>,
        element: &Element,
        location: &str,
        kind: FindingKind,
    ) {
        self.findings.push(Finding {
            profile: snapshot.url.to_owned(),
            element: element.path().as_str().to_owned(),
            location: location.to_owned(),
            kind,
        });
    }

    /// Records a constraint the check does not evaluate.
    fn unread(&mut self, snapshot: &Snapshot<'_>, element: &Element, reason: Unread) {
        self.unevaluated.insert(Unevaluated {
            profile: snapshot.url.to_owned(),
            element: element.path().as_str().to_owned(),
            reason,
        });
    }
}

/// Whether an occurrence belongs to a slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fit {
    /// Every discriminator admits it.
    Yes,
    /// A discriminator refuses it.
    No,
    /// No discriminator refuses it, and one could not be evaluated.
    Possibly,
}

/// Returns the occurrences whose value is an object, the ones an element's
/// children are checked over.
fn objects<'d>(found: &[Occurrence<'d>]) -> Vec<(String, &'d Object)> {
    found
        .iter()
        .filter_map(|(at, value)| {
            value
                .and_then(Value::as_object)
                .map(|object| (at.clone(), object))
        })
        .collect()
}

/// Returns the last segment of an element id, a slice name included.
fn last_segment(id: &str) -> &str {
    id.rsplit_once('.').map_or(id, |(_, last)| last)
}

/// Returns the occurrences of the element `name` in `node`.
///
/// A choice element `value[x]` occurs as each key `value<Type>` whose type
/// the element admits; a primitive occurs where its value or its `_` sibling
/// does (<https://hl7.org/fhir/R4/json.html#primitive>).
fn occurrences<'d>(
    node: &'d Object,
    location: &str,
    name: &str,
    types: &[String],
) -> Vec<Occurrence<'d>> {
    let keys: Vec<String> = match name.strip_suffix("[x]") {
        Some(stem) => types
            .iter()
            .map(|code| format!("{stem}{}", capitalized(code)))
            .collect(),
        None => vec![name.to_owned()],
    };
    let mut found = Vec::new();
    for key in keys {
        let at = format!("{location}.{key}");
        let values = node.get(&key);
        let siblings = node.get(&format!("_{key}"));
        match (values, siblings) {
            (Some(Value::Array(items)), _) => {
                found.extend(items.iter().enumerate().map(|(index, item)| {
                    let item = (!item.is_null()).then_some(item);
                    (format!("{at}[{index}]"), item)
                }));
            }
            (Some(value), _) => found.push((at, Some(value))),
            (None, Some(Value::Array(items))) => {
                found.extend((0..items.len()).map(|index| (format!("{at}[{index}]"), None)));
            }
            (None, Some(_)) => found.push((at, None)),
            (None, None) => {}
        }
    }
    found
}

/// Returns `code` with its first letter upper case, the form a choice key
/// takes (`valueString`).
fn capitalized(code: &str) -> String {
    let mut characters = code.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// Returns the value at a dotted `path` below `value`, the first item of an
/// array on the way.
fn follow<'d>(value: &'d Value, path: &str) -> Option<&'d Value> {
    path.split('.').try_fold(value, |at, key| {
        let next = at.get(key)?;
        match next {
            Value::Array(items) => items.first(),
            other => Some(other),
        }
    })
}

/// Returns whether `value` carries `pattern`, or `None` when the model does
/// not read the pattern's form.
fn matches(pattern: &Pattern, value: Option<&Value>) -> Option<bool> {
    match pattern {
        Pattern::Primitive(text) => Some(value.and_then(Value::as_str) == Some(text.as_str())),
        Pattern::Concept { codings, text } => {
            let Some(value) = value else {
                return Some(false);
            };
            let carried: &[Value] = value
                .get("coding")
                .and_then(Value::as_array)
                .unwrap_or_default();
            let codes = codings
                .iter()
                .all(|wanted| carried.iter().any(|coding| coding_matches(wanted, coding)));
            let texts = text
                .as_deref()
                .is_none_or(|wanted| value.get("text").and_then(Value::as_str) == Some(wanted));
            Some(codes && texts)
        }
        Pattern::Coding(wanted) => Some(value.is_some_and(|coding| coding_matches(wanted, coding))),
        _ => None,
    }
}

/// Returns whether `coding` carries every member `wanted` requires.
fn coding_matches(wanted: &CodingPattern, coding: &Value) -> bool {
    [
        ("system", wanted.system()),
        ("version", wanted.version()),
        ("code", wanted.code()),
        ("display", wanted.display()),
    ]
    .into_iter()
    .all(|(key, required)| {
        required.is_none_or(|required| coding.get(key).and_then(Value::as_str) == Some(required))
    })
}

/// Returns the reason a pattern the model does not read is listed under.
fn unread_pattern(pattern: &Pattern) -> Unread {
    match pattern {
        Pattern::Unread { key } => Unread::Pattern(key.clone()),
        _ => Unread::Pattern(String::from("pattern")),
    }
}
