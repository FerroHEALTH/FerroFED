// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The crosswalk of a category's dataset to the eHealth Network guideline and
//! to the HL7 Europe profile of its document.
//!
//! The eHealth Network guideline, the Xt-EHR logical model and the HL7 Europe
//! profile are three views of one dataset. A [`Crosswalk`] keys each row on
//! the Xt-EHR element path and names, beside it, the eHealth Network element
//! ids, the producer obligation the obligations profile puts on the element,
//! the element of the HL7 Europe profile (a slice, where the profile slices)
//! that carries it, and the profiles a FHIRconnect context may map to in
//! order to feed it. The rows are data; [`Crosswalk::check`] holds them to the
//! packages they name, so a re-pinned package that moves an element, a slice
//! or an obligation fails the check.

#[cfg(feature = "patient-summary")]
pub mod patient_summary;

use std::collections::BTreeSet;

use crate::category::Root;
use crate::dataset::ABLE_TO_POPULATE;
use crate::dataset::Dataset;
use crate::dataset::ObligationProfile;
use crate::dataset::PRODUCER;
use crate::dataset::ResourceProfile;

/// The obligation code that a producer should be able to populate an
/// element.
///
/// FHIR Obligation Codes, `SHOULD:able-to-populate`
/// (<https://hl7.org/fhir/extensions/CodeSystem-obligation.html>).
pub const SHOULD_POPULATE: &str = "SHOULD:able-to-populate";

/// What the obligations profile asks of a producer for one element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Populate {
    /// `SHALL:able-to-populate`: a producer shall be able to populate it.
    Shall,
    /// `SHOULD:able-to-populate`: a producer should be able to populate it.
    Should,
}

impl Populate {
    /// Returns the obligation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Shall => ABLE_TO_POPULATE,
            Self::Should => SHOULD_POPULATE,
        }
    }

    /// Returns the strongest populate obligation `profile` puts on the
    /// element at `path` for a producer, or `None` when it puts none.
    #[must_use]
    pub fn of(profile: &ObligationProfile, path: &str) -> Option<Self> {
        let list = profile
            .obligations()
            .iter()
            .find(|(held, _)| held.as_str() == path)
            .map(|(_, list)| list)?;
        [Self::Shall, Self::Should].into_iter().find(|populate| {
            list.iter()
                .any(|obligation| obligation.binds(populate.code(), PRODUCER))
        })
    }
}

/// One element of the eHealth Network guideline's dataset, by its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EhnElement {
    /// The element id, such as `A.2.1.1`.
    pub id: &'static str,
    /// The element's name as the guideline's table gives it.
    pub name: &'static str,
}

/// One row of a crosswalk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Row {
    /// The Xt-EHR element path, the key of the row.
    pub path: &'static str,
    /// The eHealth Network element ids the element carries; empty when the
    /// guideline's dataset has no counterpart.
    pub ehn: &'static [&'static str],
    /// The populate obligation the obligations profile puts on the element
    /// for a producer.
    pub producer: Option<Populate>,
    /// The id of the element of the exchange-format profile that carries it,
    /// a slice where the profile slices.
    pub eps: &'static str,
    /// The canonical URLs of the profiles a FHIRconnect context may name at
    /// `context.profile.url` to feed the element; empty when no context
    /// feeds it and the document is assembled around it.
    pub contexts: &'static [&'static str],
}

/// An element whose openEHR source is open for the clinical safety review.
///
/// The crosswalk names where the element sits in each view; what openEHR
/// content may feed it is the review's to decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Gap {
    /// The Xt-EHR element path, which a row of the crosswalk carries.
    pub path: &'static str,
    /// The eHealth Network element id the gap is listed under.
    pub ehn: &'static str,
}

/// The crosswalk of one category's dataset.
#[derive(Debug, Clone, Copy)]
pub struct Crosswalk<'a> {
    /// The logical model and its obligations profile, the dataset the rows
    /// are keyed on.
    pub root: Root,
    /// The canonical URL of the exchange-format profile the rows name
    /// elements of.
    pub profile: &'static str,
    /// The id of the element of that profile whose slices are the sections,
    /// such as `Composition.section`.
    pub sliced: &'static str,
    /// The eHealth Network dataset elements the rows may name.
    pub ehn: &'a [EhnElement],
    /// The rows, one per Xt-EHR element path.
    pub rows: &'a [Row],
    /// The elements open for the clinical safety review.
    pub gaps: &'a [Gap],
}

impl Crosswalk<'_> {
    /// Returns the row of the Xt-EHR element `path`, when the crosswalk has
    /// one.
    #[must_use]
    pub fn row(&self, path: &str) -> Option<&Row> {
        self.rows.iter().find(|row| row.path == path)
    }

    /// Returns every defect of the crosswalk against `dataset` and the
    /// exchange-format `profile`, in row order and then in profile order; an
    /// empty list means the crosswalk holds.
    ///
    /// A crosswalk holds when `dataset` and `profile` are the ones it names;
    /// every row names an element of the model, once, with the producer
    /// obligation the obligations profile puts on it, eHealth Network ids of
    /// its catalogue, and an element of the profile whose types admit every
    /// context profile the row lists; every element a producer
    /// `SHALL:able-to-populate` has a row; every required slice of the
    /// sliced element is named by a row; and every gap names a row that
    /// carries its eHealth Network id.
    #[must_use]
    pub fn check(&self, dataset: Dataset<'_>, profile: &ResourceProfile) -> Vec<Defect> {
        let mut defects = Vec::new();
        if dataset.model().url() != self.root.model
            || dataset.obligations().url() != self.root.obligations
        {
            defects.push(Defect::Dataset {
                model: dataset.model().url().to_owned(),
            });
        }
        if profile.url() != self.profile {
            defects.push(Defect::Profile {
                url: profile.url().to_owned(),
            });
        }
        let mut seen = BTreeSet::new();
        for row in self.rows {
            if !seen.insert(row.path) {
                defects.push(Defect::Duplicate { path: row.path });
            }
            self.check_row(row, dataset, profile, &mut defects);
        }
        for element in dataset.producer_elements() {
            let path = element.path().as_str();
            if !seen.contains(path) {
                defects.push(Defect::Uncovered {
                    path: path.to_owned(),
                });
            }
        }
        for element in profile.elements() {
            let id = element.path().as_str();
            let slice = id
                .strip_prefix(self.sliced)
                .and_then(|rest| rest.strip_prefix(':'))
                .is_some_and(|name| !name.contains('.'));
            if slice
                && element.cardinality().is_required()
                && !self.rows.iter().any(|row| row.eps == id)
            {
                defects.push(Defect::RequiredSlice { id: id.to_owned() });
            }
        }
        for gap in self.gaps {
            if !self
                .row(gap.path)
                .is_some_and(|row| row.ehn.contains(&gap.ehn))
            {
                defects.push(Defect::Gap {
                    path: gap.path,
                    ehn: gap.ehn,
                });
            }
        }
        defects
    }

    /// Pushes the defects of one row.
    fn check_row(
        &self,
        row: &Row,
        dataset: Dataset<'_>,
        profile: &ResourceProfile,
        defects: &mut Vec<Defect>,
    ) {
        if dataset.model().element(row.path).is_none() {
            defects.push(Defect::Path { path: row.path });
        }
        let package = Populate::of(dataset.obligations(), row.path);
        if package != row.producer {
            defects.push(Defect::Obligation {
                path: row.path,
                row: row.producer,
                package,
            });
        }
        for id in row.ehn {
            if !self.ehn.iter().any(|element| element.id == *id) {
                defects.push(Defect::Ehn { path: row.path, id });
            }
        }
        let Some(element) = profile.element(row.eps) else {
            defects.push(Defect::Slice {
                path: row.path,
                eps: row.eps,
            });
            return;
        };
        for context in row.contexts {
            // NOTE: FHIR R4 References, canonical (<https://hl7.org/fhir/R4/references.html#canonical>):
            // a canonical may carry a `|version` suffix, so a target is compared without it.
            let admitted = element.target_profiles().iter().any(|target| {
                target
                    .split_once('|')
                    .map_or(target.as_str(), |(url, _)| url)
                    == *context
            });
            if !admitted {
                defects.push(Defect::Context {
                    path: row.path,
                    eps: row.eps,
                    profile: context,
                });
            }
        }
    }
}

/// One way a crosswalk fails to hold against the packages it names.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum Defect {
    /// The dataset checked against is not the one the crosswalk names.
    #[error("the crosswalk is checked against the dataset {model}, which it does not name")]
    Dataset {
        /// The model the dataset holds.
        model: String,
    },
    /// The profile checked against is not the one the crosswalk names.
    #[error("the crosswalk is checked against the profile {url}, which it does not name")]
    Profile {
        /// The profile's URL.
        url: String,
    },
    /// Two rows carry one Xt-EHR element path.
    #[error("two rows carry {path}")]
    Duplicate {
        /// The repeated path.
        path: &'static str,
    },
    /// A row names an element path the logical model lacks.
    #[error("the row {path} names no element of the logical model")]
    Path {
        /// The row's path.
        path: &'static str,
    },
    /// A row states a producer obligation the obligations profile does not
    /// put on the element.
    #[error("the row {path} states the producer obligation {row:?}, the package {package:?}")]
    Obligation {
        /// The row's path.
        path: &'static str,
        /// The obligation the row states.
        row: Option<Populate>,
        /// The obligation the package puts on the element.
        package: Option<Populate>,
    },
    /// A row names an eHealth Network id the crosswalk's catalogue lacks.
    #[error("the row {path} names the eHealth Network element {id}, which the catalogue lacks")]
    Ehn {
        /// The row's path.
        path: &'static str,
        /// The id.
        id: &'static str,
    },
    /// A row names an element or slice the exchange-format profile lacks.
    #[error("the row {path} names {eps}, which the profile lacks")]
    Slice {
        /// The row's path.
        path: &'static str,
        /// The profile element id the row names.
        eps: &'static str,
    },
    /// A row admits a context profile its profile element does not target.
    #[error("the row {path} admits contexts of {profile}, which {eps} does not target")]
    Context {
        /// The row's path.
        path: &'static str,
        /// The profile element id the row names.
        eps: &'static str,
        /// The context profile the element does not target.
        profile: &'static str,
    },
    /// An element a producer `SHALL:able-to-populate` has no row.
    #[error("{path} is SHALL:able-to-populate for a producer and has no row")]
    Uncovered {
        /// The element path.
        path: String,
    },
    /// A required slice of the sliced element is named by no row.
    #[error("the required slice {id} is named by no row")]
    RequiredSlice {
        /// The slice's element id.
        id: String,
    },
    /// A gap names no row, or a row that does not carry its eHealth Network
    /// id.
    #[error("the gap {ehn} at {path} names no row carrying that id")]
    Gap {
        /// The gap's path.
        path: &'static str,
        /// The gap's eHealth Network id.
        ehn: &'static str,
    },
}
