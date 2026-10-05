// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The categories of one access, classified with the deployment's
//! [`CategoryMap`] from the model ids of what the access delivered, read or
//! wrote (Regulation (EU) 2025/327 Annex II 3.2(c)).
//!
//! The data an access reaches are its root objects, each named by its
//! template id and its archetype id: the template key wins, then the
//! archetype key. Where the access carries no root object (a query of leaf
//! values or of an aggregate, or one that matched no row), the ids the
//! request constrains its data to stand in for it, its templates first. A
//! category is a set, since one access reaches data of several. An access the
//! map cannot classify is marked [`Classification::unclassified`] with its
//! ids as evidence, and is never refused for it. No specification governs the
//! classification: our own design.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::category::Category;
use crate::map::{CategoryMap, Mapping, Table};

/// What a category of an access was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Basis {
    /// A root object the access delivered or read.
    Returned,
    /// A root object the access wrote.
    Written,
    /// An id the request constrains its data to, where it carries no root
    /// object.
    Queried,
    /// The kind of request, which serves one category by construction.
    Construction,
}

impl Basis {
    /// The basis as a record writes it.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Returned => "returned",
            Self::Written => "written",
            Self::Queried => "queried",
            Self::Construction => "construction",
        }
    }
}

/// One root object an access reached, named by its model ids: never a value
/// it holds.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct RootObject {
    /// `archetype_details.template_id`, when it names one.
    pub template_id: Option<String>,
    /// `archetype_details.archetype_id`, or the archetype node id of a root.
    pub archetype_id: Option<String>,
    /// The `OBJECT_VERSION_ID` of the version the object is, when known.
    pub version_uid: Option<String>,
}

impl fmt::Debug for RootObject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RootObject")
            .field("template_id", &self.template_id.is_some())
            .field("archetype_id", &self.archetype_id.is_some())
            .field("version_uid", &self.version_uid.is_some())
            .finish()
    }
}

/// What an access shows of the data it reached.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Evidence {
    reached: Option<Basis>,
    objects: Vec<RootObject>,
    queried_templates: BTreeSet<String>,
    queried_archetypes: BTreeSet<String>,
    no_category: bool,
    unreadable: Option<String>,
}

impl Evidence {
    /// The evidence of an access that reached `objects` as `basis`
    /// ([`Basis::Returned`] or [`Basis::Written`]).
    #[must_use]
    pub fn reached(basis: Basis, objects: Vec<RootObject>) -> Self {
        Self {
            reached: Some(basis),
            objects,
            ..Self::default()
        }
    }

    /// The evidence of an access to a resource that holds no data of any
    /// category, such as an `EHR` or an `EHR_STATUS`.
    #[must_use]
    pub fn no_category() -> Self {
        Self {
            no_category: true,
            ..Self::default()
        }
    }

    /// The evidence of an access whose data could not be read for their
    /// ids, with `why`, a reason that names no value: a body in a format the
    /// reader does not read, or an object the access did not return.
    #[must_use]
    pub fn unreadable(why: impl Into<String>) -> Self {
        Self {
            unreadable: Some(why.into()),
            ..Self::default()
        }
    }

    /// This evidence, the request constraining its data to `templates` and
    /// `archetypes`.
    #[must_use]
    pub fn queried(mut self, templates: BTreeSet<String>, archetypes: BTreeSet<String>) -> Self {
        self.queried_templates = templates;
        self.queried_archetypes = archetypes;
        self
    }

    /// The root objects the access reached.
    #[must_use]
    pub fn objects(&self) -> &[RootObject] {
        &self.objects
    }
}

impl fmt::Debug for Evidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Evidence")
            .field("reached", &self.reached)
            .field("objects", &self.objects.len())
            .field("queried_templates", &self.queried_templates.len())
            .field("queried_archetypes", &self.queried_archetypes.len())
            .field("no_category", &self.no_category)
            .field("unreadable", &self.unreadable.is_some())
            .finish()
    }
}

/// Why an access is unclassified.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unclassified {
    /// A root object, or every id the request names, maps to no key.
    Unmapped,
    /// The access carries no root object and the request names no id.
    NamedNothing,
    /// The data could not be read for their ids, for the reason given.
    Unreadable(String),
}

impl Unclassified {
    /// The reason as a record writes it.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Self::Unmapped => "unmapped",
            Self::NamedNothing => "named-nothing",
            Self::Unreadable(why) => why,
        }
    }
}

/// The categories of one access, their bases, and the evidence.
///
/// `Debug` shows the categories and the marks, never an id: a template id
/// tied to a patient says what kind of care they had.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Classification {
    categories: BTreeMap<Category, BTreeSet<Basis>>,
    no_category: bool,
    unclassified: Option<Unclassified>,
    templates: BTreeSet<String>,
    archetypes: BTreeSet<String>,
    versions: BTreeSet<String>,
    unmapped: BTreeSet<String>,
    digest: Option<String>,
}

impl fmt::Debug for Classification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Classification")
            .field("categories", &self.categories)
            .field("no_category", &self.no_category)
            .field("unclassified", &self.unclassified.as_ref().map(|_| "set"))
            .field("templates", &self.templates.len())
            .field("archetypes", &self.archetypes.len())
            .field("versions", &self.versions.len())
            .field("unmapped", &self.unmapped.len())
            .finish_non_exhaustive()
    }
}

impl Classification {
    /// The categories, each with what it was read from.
    #[must_use]
    pub fn categories(&self) -> &BTreeMap<Category, BTreeSet<Basis>> {
        &self.categories
    }

    /// Whether the access reached a resource of no category, or only data
    /// the map declares `none`, and nothing unclassified.
    #[must_use]
    pub fn is_no_category(&self) -> bool {
        self.no_category && self.categories.is_empty() && self.unclassified.is_none()
    }

    /// Why the access, or a part of it, is unclassified; `None` when the
    /// map classified all of it.
    #[must_use]
    pub fn unclassified(&self) -> Option<&Unclassified> {
        self.unclassified.as_ref()
    }

    /// The template ids of the evidence.
    #[must_use]
    pub fn templates(&self) -> &BTreeSet<String> {
        &self.templates
    }

    /// The archetype ids of the evidence.
    #[must_use]
    pub fn archetypes(&self) -> &BTreeSet<String> {
        &self.archetypes
    }

    /// The version uids of the root objects the access reached.
    #[must_use]
    pub fn versions(&self) -> &BTreeSet<String> {
        &self.versions
    }

    /// The ids the map holds no key for.
    #[must_use]
    pub fn unmapped(&self) -> &BTreeSet<String> {
        &self.unmapped
    }

    /// The digest of the map the access was classified under.
    #[must_use]
    pub fn digest(&self) -> Option<&str> {
        self.digest.as_deref()
    }

    fn add(&mut self, mapping: &Mapping, basis: Basis) {
        match mapping {
            Mapping::NoCategory => self.no_category = true,
            Mapping::Categories(categories) => {
                for category in categories {
                    self.categories
                        .entry(category.clone())
                        .or_default()
                        .insert(basis);
                }
            }
        }
    }
}

impl CategoryMap {
    /// The categories of an access that showed `evidence`.
    #[must_use]
    pub fn classify(&self, evidence: &Evidence) -> Classification {
        let mut classified = Classification {
            digest: self.digest().map(str::to_owned),
            no_category: evidence.no_category,
            ..Classification::default()
        };
        if evidence.no_category {
            return classified;
        }
        if let Some(why) = &evidence.unreadable {
            classified.unclassified = Some(Unclassified::Unreadable(why.clone()));
        }
        if let (Some(basis), false) = (evidence.reached, evidence.objects.is_empty()) {
            for object in &evidence.objects {
                self.object(object, basis, &mut classified);
            }
            return classified;
        }
        self.queried(evidence, &mut classified);
        classified
    }

    /// Classifies one root object into `classified`: its template key,
    /// else its archetype key, else it is unmapped.
    fn object(&self, object: &RootObject, basis: Basis, classified: &mut Classification) {
        classified.templates.extend(object.template_id.clone());
        classified.archetypes.extend(object.archetype_id.clone());
        classified.versions.extend(object.version_uid.clone());
        let mapping = object
            .template_id
            .as_deref()
            .and_then(|template| self.template(template))
            .or_else(|| {
                object
                    .archetype_id
                    .as_deref()
                    .and_then(|archetype| self.archetype(archetype))
            });
        if let Some(mapping) = mapping {
            classified.add(mapping, basis);
            return;
        }
        classified.unmapped.extend(
            object
                .template_id
                .iter()
                .chain(&object.archetype_id)
                .cloned(),
        );
        if classified.unclassified.is_none() {
            classified.unclassified = Some(Unclassified::Unmapped);
        }
    }

    /// Classifies the ids the request constrains its data to: its template
    /// keys, else its archetype keys; unclassified when none maps.
    fn queried(&self, evidence: &Evidence, classified: &mut Classification) {
        classified.templates.clone_from(&evidence.queried_templates);
        classified
            .archetypes
            .clone_from(&evidence.queried_archetypes);
        let mut mapped = false;
        for (ids, table) in [
            (&evidence.queried_templates, Table::Templates),
            (&evidence.queried_archetypes, Table::Archetypes),
        ] {
            if mapped {
                break;
            }
            for id in ids {
                let found = match table {
                    Table::Templates => self.template(id),
                    Table::Archetypes => self.archetype(id),
                };
                match found {
                    Some(mapping) => {
                        classified.add(mapping, Basis::Queried);
                        mapped = true;
                    }
                    None => {
                        classified.unmapped.insert(id.clone());
                    }
                }
            }
        }
        if !mapped && classified.unclassified.is_none() {
            let named =
                !evidence.queried_templates.is_empty() || !evidence.queried_archetypes.is_empty();
            classified.unclassified = Some(if named {
                Unclassified::Unmapped
            } else {
                Unclassified::NamedNothing
            });
        }
    }
}
