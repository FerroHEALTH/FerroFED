// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The format-neutral dataset model of the exchange format.
//!
//! Art 15(1)(a) leaves the "harmonised datasets" to an implementing act. The
//! Xt-EHR joint action publishes them as FHIR logical models, one per
//! priority category with its building blocks, each paired with a profile
//! stating what a producer and a consumer must do with every element
//! (`xtehr.eu.ehds.models`, CC0-1.0). This module reads that package as data:
//! a [`DatasetModel`] holds every logical model, keyed on its canonical URL,
//! with its elements keyed on their element paths, and every obligations
//! profile over them. No element is a Rust type of its own; a path such as
//! `EHDSPatientSummary.allergiesAndIntolerances` is the key a crosswalk to
//! the openEHR side and to the FHIR document is written against.

mod read;

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::io::Read;

use crate::category::Root;

/// The extension URL of an element obligation.
///
/// FHIR R5 Obligation Extension, `http://hl7.org/fhir/StructureDefinition/obligation`
/// (<https://hl7.org/fhir/extensions/StructureDefinition-obligation.html>).
pub const OBLIGATION_EXTENSION: &str = "http://hl7.org/fhir/StructureDefinition/obligation";

/// The actor the Xt-EHR package declares for a producer of a dataset.
pub const PRODUCER: &str = "https://www.xt-ehr.eu/specifications/fhir/actor-producer";

/// The actor the Xt-EHR package declares for a consumer of a dataset.
pub const CONSUMER: &str = "https://www.xt-ehr.eu/specifications/fhir/actor-consumer";

/// The obligation code that a producer is able to populate an element.
///
/// FHIR Obligation Codes, `SHALL:able-to-populate`
/// (<https://hl7.org/fhir/extensions/CodeSystem-obligation.html>).
pub const ABLE_TO_POPULATE: &str = "SHALL:able-to-populate";

/// The name and version of a FHIR package, from its `package.json`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackageId {
    name: String,
    version: String,
}

impl PackageId {
    /// Returns the package name, such as `xtehr.eu.ehds.models`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the package version, such as `1.0.0`.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}#{}", self.name, self.version)
    }
}

/// The path of one element of a logical model, as its `ElementDefinition.id`.
///
/// The id is unique within a model and names a choice slice too
/// (`EHDSLaboratoryObservation.result.value[x]:valueString`), so it is the
/// key; the first segment is the model's name.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ElementPath(String);

impl ElementPath {
    /// Returns the path as written in the package.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the name of the model the path starts at.
    #[must_use]
    pub fn model(&self) -> &str {
        self.0
            .split_once('.')
            .map_or(self.0.as_str(), |(model, _)| model)
    }

    /// Returns the path of the element one level up, or `None` for the root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        self.0
            .rsplit_once('.')
            .map(|(parent, _)| Self(parent.to_owned()))
    }
}

impl fmt::Display for ElementPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The upper bound of an element's cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Max {
    /// At most this many occurrences; `0` removes the element.
    Bounded(u32),
    /// Any number of occurrences, written `*`.
    Unbounded,
}

/// How often an element may occur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cardinality {
    min: u32,
    max: Max,
}

impl Cardinality {
    /// Returns the least number of occurrences.
    #[must_use]
    pub const fn lower(&self) -> u32 {
        self.min
    }

    /// Returns the greatest number of occurrences.
    #[must_use]
    pub const fn upper(&self) -> Max {
        self.max
    }

    /// Returns whether the element must occur at least once.
    #[must_use]
    pub const fn is_required(&self) -> bool {
        self.min > 0
    }
}

impl fmt::Display for Cardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.max {
            Max::Bounded(max) => write!(f, "{}..{max}", self.min),
            Max::Unbounded => write!(f, "{}..*", self.min),
        }
    }
}

/// One element of a logical model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    path: ElementPath,
    cardinality: Cardinality,
    types: Vec<String>,
    short: Option<String>,
}

impl Element {
    /// Returns the element's path.
    #[must_use]
    pub const fn path(&self) -> &ElementPath {
        &self.path
    }

    /// Returns how often the element may occur.
    #[must_use]
    pub const fn cardinality(&self) -> Cardinality {
        self.cardinality
    }

    /// Returns the type codes the element admits, in package order: a FHIR
    /// data type (`CodeableConcept`), `Base` for a group, or the canonical
    /// URL of another logical model.
    #[must_use]
    pub fn types(&self) -> &[String] {
        &self.types
    }

    /// Returns the element's short label, when it carries one.
    #[must_use]
    pub fn short(&self) -> Option<&str> {
        self.short.as_deref()
    }
}

/// The elements of one snapshot, in package order, indexed by path.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Elements {
    list: Vec<Element>,
    index: BTreeMap<ElementPath, usize>,
}

impl Elements {
    /// Returns the element at `path`, when the snapshot has one.
    fn get(&self, path: &str) -> Option<&Element> {
        self.index
            .get(&ElementPath(path.to_owned()))
            .and_then(|position| self.list.get(*position))
    }
}

/// One logical model of the package, with its elements in package order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalModel {
    url: String,
    name: String,
    base: Option<String>,
    elements: Elements,
}

impl LogicalModel {
    /// Returns the canonical URL of the model.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the model's computable name, such as `EHDSPatientSummary`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the canonical URL of the model it specializes, when it names
    /// one.
    #[must_use]
    pub fn base(&self) -> Option<&str> {
        self.base.as_deref()
    }

    /// Returns every element of the model's snapshot, in package order.
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements.list
    }

    /// Returns the element at `path`, when the model has one.
    #[must_use]
    pub fn element(&self, path: &str) -> Option<&Element> {
        self.elements.get(path)
    }
}

/// One obligation an element carries for one or more actors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Obligation {
    code: String,
    actors: Vec<String>,
}

impl Obligation {
    /// Returns the obligation code, such as `SHALL:able-to-populate`.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the canonical URLs of the actors the obligation binds; an
    /// obligation that names none binds every actor.
    #[must_use]
    pub fn actors(&self) -> &[String] {
        &self.actors
    }

    /// Returns whether the obligation is `code` and binds `actor`.
    #[must_use]
    pub fn binds(&self, code: &str, actor: &str) -> bool {
        self.code == code
            && (self.actors.is_empty() || self.actors.iter().any(|named| named == actor))
    }
}

/// A profile that constrains a logical model with element obligations.
///
/// Its snapshot holds every element of the model it constrains, and may
/// reach further: into the elements of a logical type an element names
/// (`EHDSHealthProfessional.name.family`) and into the type slices of a
/// choice element (`EHDSObservation.result.value[x]:valueQuantity`). The
/// obligations sit on the elements of that snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObligationProfile {
    url: String,
    name: String,
    constrains: String,
    elements: Elements,
    obligations: BTreeMap<ElementPath, Vec<Obligation>>,
}

impl ObligationProfile {
    /// Returns the canonical URL of the profile.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the profile's computable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the canonical URL of the logical model it constrains.
    #[must_use]
    pub fn constrains(&self) -> &str {
        &self.constrains
    }

    /// Returns every element of the profile's snapshot, in package order.
    #[must_use]
    pub fn elements(&self) -> &[Element] {
        &self.elements.list
    }

    /// Returns the element of the profile's snapshot at `path`, when it has
    /// one.
    #[must_use]
    pub fn element(&self, path: &str) -> Option<&Element> {
        self.elements.get(path)
    }

    /// Returns the obligations of every element that carries one, by path.
    #[must_use]
    pub const fn obligations(&self) -> &BTreeMap<ElementPath, Vec<Obligation>> {
        &self.obligations
    }

    /// Returns the paths whose obligations include `code` for `actor`, in
    /// path order.
    pub fn paths_with<'a>(
        &'a self,
        code: &'a str,
        actor: &'a str,
    ) -> impl Iterator<Item = &'a ElementPath> + 'a {
        self.obligations.iter().filter_map(move |(path, list)| {
            list.iter()
                .any(|obligation| obligation.binds(code, actor))
                .then_some(path)
        })
    }
}

/// One category's dataset: a logical model with its obligations profile.
#[derive(Debug, Clone, Copy)]
pub struct Dataset<'a> {
    model: &'a LogicalModel,
    obligations: &'a ObligationProfile,
}

impl<'a> Dataset<'a> {
    /// Returns the logical model.
    #[must_use]
    pub const fn model(&self) -> &'a LogicalModel {
        self.model
    }

    /// Returns the obligations profile over it.
    #[must_use]
    pub const fn obligations(&self) -> &'a ObligationProfile {
        self.obligations
    }

    /// Returns the elements a producer shall be able to populate, in path
    /// order: the elements an exchange-format document of the category must
    /// be able to carry.
    pub fn producer_elements(&self) -> impl Iterator<Item = &'a Element> + 'a {
        let profile = self.obligations;
        // NOTE: no specification governs this: our own design; reading puts
        // every obligation on an element of the profile's own snapshot.
        profile
            .paths_with(ABLE_TO_POPULATE, PRODUCER)
            .filter_map(move |path| profile.element(path.as_str()))
    }
}

/// The logical models and obligations profiles of one FHIR package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatasetModel {
    package: PackageId,
    models: BTreeMap<String, LogicalModel>,
    profiles: BTreeMap<String, ObligationProfile>,
}

impl DatasetModel {
    /// Reads the dataset model from a FHIR package archive, the gzip-compressed
    /// tarball a FHIR package registry serves.
    ///
    /// Every `StructureDefinition` of kind `logical` directly under `package/`
    /// is read: a `specialization` as a logical model, a `constraint` as an
    /// obligations profile over the model its `baseDefinition` names. Every
    /// other resource of the package is left out.
    ///
    /// # Errors
    ///
    /// Returns [`DatasetError`] when the archive cannot be read, carries no
    /// manifest, holds a JSON file that does not parse, or holds a logical
    /// definition that is defective: no snapshot, an element with no
    /// cardinality, a duplicate canonical URL or element path, a profile over
    /// a model the package lacks or whose snapshot drops one of its elements,
    /// or an obligation with no single code or with an empty actor.
    pub fn read(archive: impl Read) -> Result<Self, DatasetError> {
        read::package(archive)
    }

    /// Returns the package the model was read from.
    #[must_use]
    pub const fn package(&self) -> &PackageId {
        &self.package
    }

    /// Returns every logical model, by canonical URL.
    pub fn models(&self) -> impl Iterator<Item = &LogicalModel> {
        self.models.values()
    }

    /// Returns the logical model at `url`, when the package holds one.
    #[must_use]
    pub fn model(&self, url: &str) -> Option<&LogicalModel> {
        self.models.get(url)
    }

    /// Returns every obligations profile, by canonical URL.
    pub fn profiles(&self) -> impl Iterator<Item = &ObligationProfile> {
        self.profiles.values()
    }

    /// Returns the obligations profile at `url`, when the package holds one.
    #[must_use]
    pub fn profile(&self, url: &str) -> Option<&ObligationProfile> {
        self.profiles.get(url)
    }

    /// Returns the dataset `root` names.
    ///
    /// # Errors
    ///
    /// Returns [`DatasetError::MissingRoot`] when the package lacks the model
    /// or the profile, or holds a profile that constrains another model.
    pub fn dataset(&self, root: Root) -> Result<Dataset<'_>, DatasetError> {
        let missing = || DatasetError::MissingRoot {
            model: root.model.to_owned(),
            obligations: root.obligations.to_owned(),
        };
        let model = self.models.get(root.model).ok_or_else(missing)?;
        let obligations = self
            .profiles
            .get(root.obligations)
            .filter(|profile| profile.constrains == root.model)
            .ok_or_else(missing)?;
        Ok(Dataset { model, obligations })
    }
}

/// Why a package does not give a dataset model.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DatasetError {
    /// The archive could not be read as a gzip-compressed tarball.
    #[error("the package archive cannot be read")]
    Archive {
        /// The read failure.
        #[source]
        source: io::Error,
    },
    /// The archive holds no `package/package.json`.
    #[error("the package archive holds no package/package.json")]
    MissingManifest,
    /// A JSON file of the package does not parse as the shape it must have.
    #[error("{file} does not parse")]
    Json {
        /// The file, as its archive path.
        file: String,
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// Two definitions of the package carry one canonical URL.
    #[error("two definitions carry the canonical URL {url}")]
    DuplicateUrl {
        /// The URL.
        url: String,
    },
    /// A logical definition is neither a specialization nor a constraint.
    #[error("{url} has the derivation {derivation:?}, neither specialization nor constraint")]
    Derivation {
        /// The definition's URL.
        url: String,
        /// The derivation it declares, when it declares one.
        derivation: Option<String>,
    },
    /// A logical definition carries no snapshot.
    #[error("{url} carries no snapshot")]
    MissingSnapshot {
        /// The definition's URL.
        url: String,
    },
    /// An element carries no `id`, no `min` or no `max`, or a `max` that is
    /// neither `*` nor a number.
    #[error("{url} has an element at {path} with no id or no readable cardinality")]
    Element {
        /// The definition's URL.
        url: String,
        /// The element's `path`.
        path: String,
    },
    /// Two elements of one definition carry one id.
    #[error("{url} carries the element {path} twice")]
    DuplicateElement {
        /// The definition's URL.
        url: String,
        /// The repeated id.
        path: String,
    },
    /// A constraint profile names no `baseDefinition`.
    #[error("{url} is a constraint with no baseDefinition")]
    MissingBase {
        /// The profile's URL.
        url: String,
    },
    /// A constraint profile names a base the package holds no model for.
    #[error("{url} constrains {base}, which the package holds no model for")]
    UnknownBase {
        /// The profile's URL.
        url: String,
        /// The base it names.
        base: String,
    },
    /// A constraint profile's snapshot drops an element of the model it
    /// constrains.
    #[error("{url} constrains {base} but its snapshot lacks {path}")]
    Uncovered {
        /// The profile's URL.
        url: String,
        /// The model's URL.
        base: String,
        /// The element id the profile lacks.
        path: String,
    },
    /// An obligation extension carries no single `code`.
    #[error("{url} has an obligation on {path} with no single code")]
    Obligation {
        /// The profile's URL.
        url: String,
        /// The element id.
        path: String,
    },
    /// The package lacks the model or the profile a category names.
    #[error("the package holds no model {model} with the profile {obligations} over it")]
    MissingRoot {
        /// The model's URL.
        model: String,
        /// The profile's URL.
        obligations: String,
    },
}
