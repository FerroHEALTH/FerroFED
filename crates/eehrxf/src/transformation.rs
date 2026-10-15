// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What one semantic transformation did (feature `fhir-r4`).
//!
//! The eHealth Network General Guidelines (Release 3.1, Art 6(3)) ask the
//! entity that performs a semantic transformation to "enable logging
//! sufficient for examination of events potentially leading to patient
//! safety issues". A [`Report`] holds the facts of one mapping between
//! openEHR and the exchange format that such a log needs: which way it ran,
//! the category, the mapping and terminology versions in force, and every
//! element it transcoded, left untranscoded or dropped, counted by the path
//! that names it.
//!
//! - A coded element (a `Coding`, or a `CodeableConcept` with the codings it
//!   holds) is transcoded when one of its codings is in a preferred code
//!   system, and untranscoded otherwise: it travels as text, or in a system
//!   the receiver need not read (eHealth Network Guidelines on Patient
//!   Summary Art 10(7)). A `code` primitive is bound by FHIR itself, so it
//!   is no transcoding and is not counted.
//! - A dropped element is one the mapping run declared it did not carry
//!   (its `OperationOutcome` issues of severity `warning`), or a whole
//!   composition no mapping covers.
//!
//! A path is the R4 element path of the element's definition, such as
//! `AllergyIntolerance.code`, for a declared loss too, never what the element
//! holds or the index it sits at: a report carries no clinical value, no code
//! value and no patient identifier. The crate records nowhere: the system
//! that applied the transformation hands the report to its log.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use fhir_types::codec::Json;
use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::operation_outcome::OperationOutcome;
use fhir_types::r4::resource::Resource;

use crate::category::Category;
use crate::terminology::ehn;
use crate::terminology::walk::{self, Coded};

/// Which way a transformation ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// From openEHR to the exchange format.
    Provided,
    /// From the exchange format to openEHR.
    Received,
}

/// The mapping a transformation ran, by the names it was compiled from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pin {
    /// The FHIRconnect context mapping, or `None` when no mapping covered
    /// the data.
    pub context: Option<String>,
    /// The openEHR template id it maps.
    pub template: String,
    /// The canonical URL of the profile it maps to, when one ran.
    pub profile: Option<String>,
    /// The file names of its model and context mappings.
    pub files: Vec<String>,
    /// The file name of the FHIR package a received document was held to.
    pub package: Option<String>,
}

/// Elements of one fate: how many, and how many at each path that names
/// one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    count: usize,
    paths: BTreeMap<String, usize>,
}

impl Tally {
    /// Counts one element, at `path` when it names one.
    fn add(&mut self, path: Option<&str>) {
        self.count = self.count.saturating_add(1);
        if let Some(path) = path {
            let held = self.paths.entry(path.to_owned()).or_default();
            *held = held.saturating_add(1);
        }
    }

    /// Adds every element `other` counts.
    fn extend(&mut self, other: &Self) {
        self.count = self.count.saturating_add(other.count);
        for (path, count) in &other.paths {
            let held = self.paths.entry(path.clone()).or_default();
            *held = held.saturating_add(*count);
        }
    }

    /// Returns the number of elements.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Returns the count at each path, in path order; an element whose path
    /// cannot be told without a value is counted in [`Tally::count`] alone.
    #[must_use]
    pub const fn paths(&self) -> &BTreeMap<String, usize> {
        &self.paths
    }
}

/// What one transformation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    direction: Direction,
    category: Category,
    pin: Pin,
    terminology: BTreeSet<String>,
    transcoded: Tally,
    untranscoded: Tally,
    dropped: Tally,
}

/// Why a report cannot be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReportError {
    /// A resource does not encode as FHIR JSON.
    #[error("a transformed resource does not encode as FHIR JSON")]
    Encode {
        /// Why.
        #[source]
        source: fhir_types::codec::EncodeError,
    },
    /// A resource holds an element the R4 element table does not describe.
    #[error("{location} is an element the R4 element table does not describe")]
    Unwalkable {
        /// The element path, with indexes.
        location: String,
    },
}

impl Report {
    /// Starts the report of a transformation that ran `direction` over data
    /// of `category` through the mapping `pin`, nothing counted yet.
    ///
    /// The terminology in force starts with the releases the preferred code
    /// systems of [`ehn`] are read from.
    #[must_use]
    pub fn new(direction: Direction, category: Category, pin: Pin) -> Self {
        Self {
            direction,
            category,
            pin,
            terminology: ehn::RELEASES
                .iter()
                .map(|(name, version)| format!("{name}|{version}"))
                .collect(),
            transcoded: Tally::default(),
            untranscoded: Tally::default(),
            dropped: Tally::default(),
        }
    }

    /// Counts every coded element of `resource`, transcoded when one of its
    /// codings has a system `preferred` admits, and adds every code system
    /// version its codings name to the terminology in force.
    ///
    /// The resource's `meta` and the resources it contains are not counted.
    ///
    /// # Errors
    ///
    /// [`ReportError::Encode`] for a resource that does not encode, and
    /// [`ReportError::Unwalkable`] for one holding an element the R4 table
    /// does not describe.
    pub fn count_coded(
        &mut self,
        resource: &Resource,
        preferred: impl Fn(&str) -> bool,
    ) -> Result<(), ReportError> {
        let object = Json::to_json(resource).map_err(|source| ReportError::Encode { source })?;
        let kind = object
            .get("resourceType")
            .and_then(Value::as_str)
            .unwrap_or("Resource")
            .to_owned();
        let found =
            walk::coded(&object, &kind, false).map_err(|unwalkable| ReportError::Unwalkable {
                location: unwalkable.location,
            })?;
        let mut concepts: Vec<String> = Vec::new();
        for occurrence in &found {
            if occurrence.element.starts_with("Meta.") {
                continue;
            }
            let codings: Vec<&Object> = match &occurrence.value {
                Coded::Code(_) => continue,
                Coded::Concept(concept) => {
                    concepts.push(format!("{}.coding", occurrence.location));
                    concept
                        .get("coding")
                        .and_then(Value::as_array)
                        .map(|codings| codings.iter().filter_map(Value::as_object).collect())
                        .unwrap_or_default()
                }
                Coded::Coding(coding) => {
                    if concepts
                        .iter()
                        .any(|concept| occurrence.location.starts_with(concept.as_str()))
                    {
                        continue;
                    }
                    vec![*coding]
                }
            };
            let mut carried = false;
            for coding in codings {
                let system = coding.get("system").and_then(Value::as_str);
                carried |= system.is_some_and(&preferred);
                if let (Some(system), Some(version)) =
                    (system, coding.get("version").and_then(Value::as_str))
                {
                    self.terminology.insert(format!("{system}|{version}"));
                }
            }
            let tally = if carried {
                &mut self.transcoded
            } else {
                &mut self.untranscoded
            };
            tally.add(Some(occurrence.element));
        }
        Ok(())
    }

    /// Counts every loss the mapping run declared in `outcome`: each issue
    /// of severity `warning`, at the R4 element path its `expression`
    /// locates, and with no path when the loss has no FHIR element.
    pub fn count_losses(&mut self, outcome: &OperationOutcome) {
        for issue in &outcome.issue {
            if issue.severity.value.as_deref() != Some("warning") {
                continue;
            }
            let path = issue
                .expression
                .first()
                .and_then(|expression| expression.value.as_deref())
                .map(element_path);
            self.dropped.add(path.as_deref());
        }
    }

    /// Counts one whole composition no mapping covers as dropped, at the RM
    /// path of its root.
    pub fn count_uncovered(&mut self) {
        self.dropped.add(Some("/"));
    }

    /// Adds every element `other` counts and every version it names; the
    /// direction, the category and the pin stay this report's.
    pub fn absorb(&mut self, other: &Self) {
        self.terminology.extend(other.terminology.iter().cloned());
        self.transcoded.extend(&other.transcoded);
        self.untranscoded.extend(&other.untranscoded);
        self.dropped.extend(&other.dropped);
    }

    /// Returns which way the transformation ran.
    #[must_use]
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// Returns the category of the data it transformed.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// Returns the mapping it ran.
    #[must_use]
    pub const fn pin(&self) -> &Pin {
        &self.pin
    }

    /// Returns the terminology releases and code system versions in force,
    /// each `<name>|<version>`, in order.
    #[must_use]
    pub const fn terminology(&self) -> &BTreeSet<String> {
        &self.terminology
    }

    /// Returns the coded elements carried in a preferred code system.
    #[must_use]
    pub const fn transcoded(&self) -> &Tally {
        &self.transcoded
    }

    /// Returns the coded elements carried in no preferred code system.
    #[must_use]
    pub const fn untranscoded(&self) -> &Tally {
        &self.untranscoded
    }

    /// Returns the elements the mapping did not carry.
    #[must_use]
    pub const fn dropped(&self) -> &Tally {
        &self.dropped
    }
}

/// Returns the R4 element path of the element a loss's `expression` locates:
/// the path with every repetition index removed, so
/// `Condition.code.coding[1].code` counts at `Condition.code.coding.code`.
// NOTE: R4 OperationOutcome.issue.expression admits only element names, repetition indicators
// and the default child accessor, so dropping the bracketed indexes leaves the definition path.
fn element_path(expression: &str) -> String {
    let mut path = String::with_capacity(expression.len());
    let mut indexed = false;
    for character in expression.chars() {
        match character {
            '[' => indexed = true,
            ']' => indexed = false,
            _ if !indexed => path.push(character),
            _ => {}
        }
    }
    path
}
