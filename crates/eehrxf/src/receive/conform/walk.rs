// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The walk of one profile's snapshot over the document.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use fhir_types::codec::Object;
use fhir_types::codec::Value;

use crate::dataset::Element;
use crate::dataset::Max;
use crate::dataset::ResourceProfile;
use crate::dataset::constraint::DiscriminatorKind;
use crate::dataset::constraint::Pattern;
use crate::dataset::constraint::Slicing;
use crate::dataset::constraint::SlicingRules;
use crate::receive::conform::CheckError;
use crate::receive::conform::Finding;
use crate::receive::conform::FindingKind;
use crate::receive::conform::Unevaluated;
use crate::receive::conform::Unread;
use crate::receive::conform::values::Occurrence;
use crate::receive::conform::values::READ;
use crate::receive::conform::values::entry_reference_rule;
use crate::receive::conform::values::follow;
use crate::receive::conform::values::last_segment;
use crate::receive::conform::values::matches;
use crate::receive::conform::values::objects;
use crate::receive::conform::values::occurrences;
use crate::receive::conform::values::unread_pattern;

/// The state of one check over one or more profiles.
#[derive(Debug, Default)]
pub(super) struct Walk {
    pub(super) findings: Vec<Finding>,
    pub(super) unevaluated: BTreeSet<Unevaluated>,
}

/// One profile's snapshot, its elements grouped under their parent id.
struct Snapshot<'p> {
    url: &'p str,
    children: BTreeMap<&'p str, Vec<&'p Element>>,
    by_id: BTreeMap<&'p str, &'p Element>,
}

impl Walk {
    /// Walks `profile` over `root`, a resource of type `expected`.
    pub(super) fn profile(
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
        self.invariants(&snapshot, top, root);
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
        self.types(snapshot, element, slicing, &slices, found);
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

    /// Refuses an occurrence of a type-discriminated slicing whose resource
    /// type no slice names.
    // NOTE: R4 profiling lets an open slicing admit an occurrence no slice names; a received
    // document is held closed on type, so an entry no slice describes is refused, never
    // skipped (no specification governs this: our own design).
    fn types(
        &mut self,
        snapshot: &Snapshot<'_>,
        element: &Element,
        slicing: &Slicing,
        slices: &[&Element],
        found: &[Occurrence<'_>],
    ) {
        for discriminator in slicing.discriminators() {
            let path = discriminator.path();
            if discriminator.kind() != DiscriminatorKind::Type || path.contains('(') {
                continue;
            }
            let named: BTreeSet<&str> = slices
                .iter()
                .filter_map(|slice| {
                    let id = format!("{}.{path}", slice.path().as_str());
                    snapshot.by_id.get(id.as_str()).copied()
                })
                .flat_map(|target| target.types().iter().map(String::as_str))
                .collect();
            for (at, value) in found {
                let kind = value
                    .and_then(|value| {
                        if path == "$this" {
                            Some(value)
                        } else {
                            follow(value, path)
                        }
                    })
                    .and_then(|target| target.get("resourceType"))
                    .and_then(Value::as_str);
                if !kind.is_some_and(|kind| named.contains(kind)) {
                    self.find(snapshot, element, at, FindingKind::TypeNotAllowed);
                }
            }
        }
    }

    /// Evaluates the invariants of the profile's root element over `root`.
    ///
    /// One form is evaluated: an invariant that every entry of the listed
    /// resource types carries a reference at one element,
    /// `entry.where(resource.is(T) or ...).empty() or entry.where(resource.is(T)
    /// or ...).all(resource.<element>.reference.exists())`, the form of the EPS
    /// `eps-bundle-subject-ref` and `eps-bundle-patient-ref`. Every other
    /// invariant is listed as not evaluated, except the R4 document rules the
    /// reader holds (`bdl-7` to `bdl-11`).
    fn invariants(&mut self, snapshot: &Snapshot<'_>, top: &Element, root: &Object) {
        for invariant in top.invariants() {
            if READ.contains(&invariant.key()) {
                continue;
            }
            let rule = invariant.expression().and_then(entry_reference_rule);
            let (Some((types, field)), true) = (rule, invariant.is_error()) else {
                self.unread(snapshot, top, Unread::Invariant(invariant.key().to_owned()));
                continue;
            };
            let entries = root
                .get("entry")
                .and_then(Value::as_array)
                .unwrap_or_default();
            for (index, entry) in entries.iter().enumerate() {
                let resource = entry.get("resource");
                let kind = resource
                    .and_then(|resource| resource.get("resourceType"))
                    .and_then(Value::as_str);
                if !kind.is_some_and(|kind| types.iter().any(|listed| listed == kind)) {
                    continue;
                }
                let carried = resource
                    .and_then(|resource| resource.get(&field))
                    .and_then(|reference| reference.get("reference"))
                    .and_then(Value::as_str)
                    .is_some();
                if !carried {
                    self.find(
                        snapshot,
                        top,
                        &format!("Bundle.entry[{index}].resource"),
                        FindingKind::Invariant {
                            key: invariant.key().to_owned(),
                        },
                    );
                }
            }
        }
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
