// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The record of a semantic transformation an access applied.
//!
//! The eHealth Network General Guidelines (Release 3.1, Art 6(3)) ask that
//! "Entities performing semantic transformations, such as `NCPeHs` in
//! `MyHealth@EU`, shall enable logging sufficient for examination of events
//! potentially leading to patient safety issues." A [`Transformation`] is
//! that log for one mapping between openEHR and the European exchange
//! format: which way it ran, the Art 14(1) category, the mapping and
//! terminology versions in force, and how many elements it transcoded, left
//! untranscoded (eHealth Network Guidelines on Patient Summary Art 10(7)) or
//! dropped, each counted by its element path.
//!
//! A path names where an element sits in its model, such as the FHIR element
//! path `AllergyIntolerance.code` or the openEHR RM path `/`, and never what
//! the element holds: a transformation carries no clinical value, no code
//! value and no patient identifier. It rides on the access record of the
//! access that applied it, so the two are one record.

use std::collections::BTreeMap;

use crate::category::Category;

/// Which way a transformation ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// From openEHR to the exchange format: data provided (Regulation (EU)
    /// 2025/327 Annex II 2.1).
    Provided,
    /// From the exchange format to openEHR: data received (Annex II 2.2).
    Received,
}

impl Direction {
    /// The direction as a record writes it: `provided` or `received`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Provided => "provided",
            Self::Received => "received",
        }
    }
}

/// The mapping a transformation ran, by the names and versions it was
/// compiled from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Mapping {
    /// The FHIRconnect context mapping, or `None` when no mapping covered
    /// the data.
    pub context: Option<String>,
    /// The openEHR template id the mapping maps.
    pub template: String,
    /// The canonical URL of the profile the mapping maps to, when one ran.
    pub profile: Option<String>,
    /// The file names of the model and context mappings.
    pub files: Vec<String>,
    /// The file name of the FHIR package the received data were held to,
    /// for a received document.
    pub package: Option<String>,
}

/// Elements of one fate, counted, with the count at each path that names
/// one.
///
/// An element whose path cannot be told without quoting a value is counted
/// with no path, so [`Elements::count`] is at least the sum of the paths'
/// counts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Elements {
    count: usize,
    paths: BTreeMap<String, usize>,
}

impl Elements {
    /// The elements `paths` counts at each path, and `count` in all; a
    /// `count` below the sum of the paths' counts is raised to it.
    #[must_use]
    pub fn new(count: usize, paths: BTreeMap<String, usize>) -> Self {
        let named = paths
            .values()
            .fold(0_usize, |sum, held| sum.saturating_add(*held));
        Self {
            count: count.max(named),
            paths,
        }
    }

    /// Counts one element, at `path` when it names one.
    pub fn add(&mut self, path: Option<&str>) {
        self.count = self.count.saturating_add(1);
        if let Some(path) = path {
            let held = self.paths.entry(path.to_owned()).or_default();
            *held = held.saturating_add(1);
        }
    }

    /// Adds every element `other` counts.
    pub fn extend(&mut self, other: &Self) {
        self.count = self.count.saturating_add(other.count);
        for (path, count) in &other.paths {
            let held = self.paths.entry(path.clone()).or_default();
            *held = held.saturating_add(*count);
        }
    }

    /// The number of elements.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// The count at each path, in path order.
    #[must_use]
    pub const fn paths(&self) -> &BTreeMap<String, usize> {
        &self.paths
    }
}

/// One semantic transformation an access applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transformation {
    /// Which way it ran.
    pub direction: Direction,
    /// The Art 14(1) category of the data it transformed, when it names one.
    pub category: Option<Category>,
    /// The mapping it ran.
    pub mapping: Mapping,
    /// The terminology releases and code system versions in force, each as
    /// `<name>|<version>`.
    pub terminology: Vec<String>,
    /// The coded elements carried in a preferred code system.
    pub transcoded: Elements,
    /// The coded elements carried in no preferred code system, as text or in
    /// another system (eHealth Network Guidelines on Patient Summary Art
    /// 10(7)).
    pub untranscoded: Elements,
    /// The elements the mapping did not carry.
    pub dropped: Elements,
}
