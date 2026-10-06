// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Reading a FHIR package archive into a [`DatasetModel`].
//!
//! A FHIR package is an npm tarball whose `package/` folder holds the
//! manifest (`package.json`) and one JSON file per conformance resource
//! (FHIR NPM Package Specification,
//! <https://confluence.hl7.org/display/FHIR/NPM+Package+Specification>).
//! Only the members this module reads are modelled; every other member of a
//! resource is left out by serde.

use std::collections::BTreeMap;
use std::io;
use std::io::Read;
use std::path::Component;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

use super::Cardinality;
use super::DatasetError;
use super::DatasetModel;
use super::Element;
use super::ElementPath;
use super::Elements;
use super::LogicalModel;
use super::Max;
use super::OBLIGATION_EXTENSION;
use super::Obligation;
use super::ObligationProfile;
use super::PackageId;
use super::ResourceProfile;
use super::constraint::CodingPattern;
use super::constraint::Discriminator;
use super::constraint::DiscriminatorKind;
use super::constraint::Pattern;
use super::constraint::Slicing;
use super::constraint::SlicingRules;

/// The `package.json` members the model keeps.
#[derive(Debug, Deserialize)]
struct Manifest {
    name: String,
    version: String,
}

/// The one member read first, to tell a `StructureDefinition` from the rest.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Head {
    resource_type: Option<String>,
    url: Option<String>,
}

/// The `StructureDefinition` members the model reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructureDefinition {
    url: String,
    name: String,
    version: Option<String>,
    kind: Option<String>,
    derivation: Option<String>,
    base_definition: Option<String>,
    snapshot: Option<Snapshot>,
}

/// The snapshot of a `StructureDefinition`, each element kept as its JSON
/// text so it is read twice: once for its typed members, once for its
/// `fixed[x]` and `pattern[x]` keys.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Snapshot {
    element: Vec<Box<RawValue>>,
}

/// The `ElementDefinition` members the model reads.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ElementDefinition {
    id: Option<String>,
    path: String,
    min: Option<u32>,
    max: Option<String>,
    #[serde(rename = "type")]
    types: Vec<TypeRef>,
    short: Option<String>,
    extension: Vec<Extension>,
    slicing: Option<SlicingDefinition>,
    #[serde(skip)]
    pattern: Option<Pattern>,
}

/// One `ElementDefinition.slicing`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SlicingDefinition {
    discriminator: Vec<DiscriminatorDefinition>,
    ordered: Option<bool>,
    rules: Option<String>,
}

/// One `ElementDefinition.slicing.discriminator`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DiscriminatorDefinition {
    #[serde(rename = "type")]
    kind: String,
    path: String,
}

/// A `Coding`, or one coding of a `CodeableConcept`, as a pattern writes it.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct CodingValue {
    system: Option<String>,
    version: Option<String>,
    code: Option<String>,
    display: Option<String>,
}

/// A `CodeableConcept` as a pattern writes it.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ConceptValue {
    coding: Vec<CodingValue>,
    text: Option<String>,
}

/// The suffixes of the R4 primitive types whose JSON form is a string
/// (<https://hl7.org/fhir/R4/datatypes.html#primitive>,
/// <https://hl7.org/fhir/R4/json.html#primitive>).
const STRING_PRIMITIVES: &[&str] = &[
    "Base64Binary",
    "Canonical",
    "Code",
    "Date",
    "DateTime",
    "Id",
    "Instant",
    "Markdown",
    "Oid",
    "String",
    "Time",
    "Uri",
    "Url",
    "Uuid",
];

/// One `ElementDefinition.type`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct TypeRef {
    code: String,
    target_profile: Vec<String>,
}

/// One extension, with the two value types an obligation carries.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Extension {
    url: String,
    #[serde(rename = "extension")]
    parts: Vec<Self>,
    value_code: Option<String>,
    value_canonical: Option<String>,
}

/// Reads `archive` into a dataset model.
pub(super) fn package(archive: impl Read) -> Result<DatasetModel, DatasetError> {
    let (package, mut definitions) = definitions(archive, |_| true)?;
    definitions.retain(|definition| definition.kind.as_deref() == Some("logical"));
    assemble(package, definitions)
}

/// Reads the one `StructureDefinition` of `archive` at the canonical `url`.
pub(super) fn resource_profile(
    archive: impl Read,
    url: &str,
) -> Result<ResourceProfile, DatasetError> {
    let (package, definitions) = definitions(archive, |head| head.url.as_deref() == Some(url))?;
    let mut definitions = definitions.into_iter();
    let definition = match (definitions.next(), definitions.next()) {
        (Some(definition), None) => definition,
        (Some(_), Some(_)) => {
            return Err(DatasetError::DuplicateUrl {
                url: url.to_owned(),
            });
        }
        (None, _) => {
            return Err(DatasetError::MissingProfile {
                url: url.to_owned(),
            });
        }
    };
    let snapshot = definition
        .snapshot
        .ok_or_else(|| DatasetError::MissingSnapshot {
            url: definition.url.clone(),
        })?;
    let elements = elements(&definition.url, snapshot.element, |_| Ok(()))?;
    Ok(ResourceProfile {
        package,
        url: definition.url,
        name: definition.name,
        version: definition.version,
        elements,
    })
}

/// Reads the manifest of `archive` and every `StructureDefinition` whose
/// head `keep` admits, in archive order.
fn definitions(
    archive: impl Read,
    mut keep: impl FnMut(&Head) -> bool,
) -> Result<(PackageId, Vec<StructureDefinition>), DatasetError> {
    let mut tarball = tar::Archive::new(GzDecoder::new(archive));
    let mut manifest = None;
    let mut definitions = Vec::new();
    for entry in tarball.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path().map_err(unreadable)?.into_owned();
        let Some(file) = resource_file(&path) else {
            continue;
        };
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(unreadable)?;
        let archived = path.to_string_lossy().into_owned();
        if file == "package.json" {
            manifest = Some(parse::<Manifest>(&archived, &bytes)?);
            continue;
        }
        let head = parse::<Head>(&archived, &bytes)?;
        if head.resource_type.as_deref() != Some("StructureDefinition") || !keep(&head) {
            continue;
        }
        definitions.push(parse::<StructureDefinition>(&archived, &bytes)?);
    }
    let manifest = manifest.ok_or(DatasetError::MissingManifest)?;
    Ok((
        PackageId {
            name: manifest.name,
            version: manifest.version,
        },
        definitions,
    ))
}

/// Builds the model from every logical definition, in any order.
fn assemble(
    package: PackageId,
    logical: Vec<StructureDefinition>,
) -> Result<DatasetModel, DatasetError> {
    let mut models = BTreeMap::new();
    let mut constraints = Vec::new();
    for definition in logical {
        match definition.derivation.as_deref() {
            Some("specialization") => {
                let model = model(definition)?;
                if models.contains_key(&model.url) {
                    return Err(DatasetError::DuplicateUrl { url: model.url });
                }
                models.insert(model.url.clone(), model);
            }
            Some("constraint") => constraints.push(definition),
            _ => {
                return Err(DatasetError::Derivation {
                    url: definition.url,
                    derivation: definition.derivation,
                });
            }
        }
    }
    let mut profiles = BTreeMap::new();
    for definition in constraints {
        let profile = profile(definition, &models)?;
        if models.contains_key(&profile.url) || profiles.contains_key(&profile.url) {
            return Err(DatasetError::DuplicateUrl { url: profile.url });
        }
        profiles.insert(profile.url.clone(), profile);
    }
    Ok(DatasetModel {
        package,
        models,
        profiles,
    })
}

/// Builds one logical model from its snapshot.
fn model(definition: StructureDefinition) -> Result<LogicalModel, DatasetError> {
    let url = definition.url;
    let snapshot = definition
        .snapshot
        .ok_or_else(|| DatasetError::MissingSnapshot { url: url.clone() })?;
    let elements = elements(&url, snapshot.element, |_| Ok(()))?;
    Ok(LogicalModel {
        name: definition.name,
        base: definition.base_definition,
        url,
        elements,
    })
}

/// Builds the elements of one snapshot, handing each raw element to `visit`
/// first, and refusing an element given twice.
fn elements(
    url: &str,
    snapshot: Vec<Box<RawValue>>,
    mut visit: impl FnMut(&ElementDefinition) -> Result<(), DatasetError>,
) -> Result<Elements, DatasetError> {
    let mut list = Vec::with_capacity(snapshot.len());
    let mut index = BTreeMap::new();
    for text in snapshot {
        let raw = definition(url, &text)?;
        visit(&raw)?;
        let element = element(url, raw)?;
        if index.contains_key(&element.path) {
            return Err(DatasetError::DuplicateElement {
                url: url.to_owned(),
                path: element.path.0,
            });
        }
        index.insert(element.path.clone(), list.len());
        list.push(element);
    }
    Ok(Elements { list, index })
}

/// Builds one element, refusing one with no id or no readable cardinality.
fn element(url: &str, raw: ElementDefinition) -> Result<Element, DatasetError> {
    let max = match raw.max.as_deref() {
        Some("*") => Some(Max::Unbounded),
        Some(text) => match text.parse::<u32>() {
            Ok(bound) => Some(Max::Bounded(bound)),
            Err(_) => None,
        },
        None => None,
    };
    let (Some(id), Some(min), Some(max)) = (raw.id, raw.min, max) else {
        return Err(DatasetError::Element {
            url: url.to_owned(),
            path: raw.path,
        });
    };
    let slicing = raw
        .slicing
        .map(|slicing| {
            slicing_of(slicing).ok_or_else(|| DatasetError::Slicing {
                url: url.to_owned(),
                path: id.clone(),
            })
        })
        .transpose()?;
    Ok(Element {
        pattern: raw.pattern,
        slicing,
        path: ElementPath(id),
        cardinality: Cardinality { min, max },
        profiles: raw
            .types
            .iter()
            .flat_map(|reference| reference.target_profile.iter().cloned())
            .collect(),
        types: raw
            .types
            .into_iter()
            .map(|reference| reference.code)
            .collect(),
        short: raw.short,
    })
}

/// Parses one snapshot element, its `fixed[x]` or `pattern[x]` included.
fn definition(url: &str, text: &RawValue) -> Result<ElementDefinition, DatasetError> {
    let unparsed = |source| DatasetError::ElementJson {
        url: url.to_owned(),
        source,
    };
    let mut definition: ElementDefinition = serde_json::from_str(text.get()).map_err(unparsed)?;
    let members: BTreeMap<String, Box<RawValue>> =
        serde_json::from_str(text.get()).map_err(unparsed)?;
    let mut keys = members
        .iter()
        .filter(|(key, _)| choice_suffix(key).is_some());
    definition.pattern = match (keys.next(), keys.next()) {
        (None, _) => None,
        (Some((key, value)), None) => Some(pattern_of(key, value)),
        (Some((key, _)), Some((other, _))) => Some(Pattern::Unread {
            key: format!("{key}, {other}"),
        }),
    };
    Ok(definition)
}

/// Returns the type suffix of a `fixed[x]` or `pattern[x]` key, or `None`
/// for any other key.
fn choice_suffix(key: &str) -> Option<&str> {
    let suffix = key
        .strip_prefix("fixed")
        .or_else(|| key.strip_prefix("pattern"))?;
    suffix
        .chars()
        .next()
        .is_some_and(char::is_uppercase)
        .then_some(suffix)
}

/// Reads one `fixed[x]` or `pattern[x]` value under its key.
// NOTE: a form this model does not read, or a value that does not parse as
// its form, is legitimately unread rather than defective: the profile is
// valid FHIR and the check reports the constraint as not evaluated.
fn pattern_of(key: &str, value: &RawValue) -> Pattern {
    let unread = || Pattern::Unread {
        key: key.to_owned(),
    };
    let Some(suffix) = choice_suffix(key) else {
        return unread();
    };
    match (key.starts_with("pattern"), suffix) {
        (true, "CodeableConcept") => serde_json::from_str::<ConceptValue>(value.get()).map_or_else(
            |_| unread(),
            |concept| Pattern::Concept {
                codings: concept.coding.into_iter().map(coding_of).collect(),
                text: concept.text,
            },
        ),
        (true, "Coding") => serde_json::from_str::<CodingValue>(value.get())
            .map_or_else(|_| unread(), |coding| Pattern::Coding(coding_of(coding))),
        (_, primitive) if STRING_PRIMITIVES.contains(&primitive) => {
            serde_json::from_str::<String>(value.get())
                .map_or_else(|_| unread(), Pattern::Primitive)
        }
        _ => unread(),
    }
}

/// Moves a pattern's coding into the model.
fn coding_of(coding: CodingValue) -> CodingPattern {
    CodingPattern {
        system: coding.system,
        version: coding.version,
        code: coding.code,
        display: coding.display,
    }
}

/// Reads one slicing, or `None` when it names a discriminator type or rules
/// outside the R4 value sets.
fn slicing_of(slicing: SlicingDefinition) -> Option<Slicing> {
    let discriminators = slicing
        .discriminator
        .into_iter()
        .map(|discriminator| {
            DiscriminatorKind::from_code(&discriminator.kind).map(|kind| Discriminator {
                kind,
                path: discriminator.path,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Slicing {
        discriminators,
        ordered: slicing.ordered.unwrap_or(false),
        rules: SlicingRules::from_code(slicing.rules.as_deref()?)?,
    })
}

/// Builds one obligations profile over a model the package holds.
///
/// The profile's own snapshot is its element list, and it must hold every
/// element of the model it constrains.
fn profile(
    definition: StructureDefinition,
    models: &BTreeMap<String, LogicalModel>,
) -> Result<ObligationProfile, DatasetError> {
    let url = definition.url;
    let Some(base) = definition.base_definition else {
        return Err(DatasetError::MissingBase { url });
    };
    let Some(model) = models.get(&base) else {
        return Err(DatasetError::UnknownBase { url, base });
    };
    let snapshot = definition
        .snapshot
        .ok_or_else(|| DatasetError::MissingSnapshot { url: url.clone() })?;
    let mut obligations = BTreeMap::new();
    let elements = elements(&url, snapshot.element, |raw| {
        let list = obligations_of(&url, raw)?;
        if let (false, Some(id)) = (list.is_empty(), raw.id.as_ref()) {
            obligations.insert(ElementPath(id.clone()), list);
        }
        Ok(())
    })?;
    if let Some(dropped) = model
        .elements()
        .iter()
        .find(|element| elements.get(element.path.as_str()).is_none())
    {
        return Err(DatasetError::Uncovered {
            url,
            base,
            path: dropped.path.0.clone(),
        });
    }
    Ok(ObligationProfile {
        name: definition.name,
        url,
        constrains: base,
        elements,
        obligations,
    })
}

/// Reads the obligation extensions of one element.
///
/// An obligation carries exactly one `code` and any number of `actor`s; its
/// other parts (`documentation`, `usage`, `filter`) are left out.
fn obligations_of(url: &str, raw: &ElementDefinition) -> Result<Vec<Obligation>, DatasetError> {
    let mut list = Vec::new();
    for extension in raw
        .extension
        .iter()
        .filter(|extension| extension.url == OBLIGATION_EXTENSION)
    {
        let defective = || DatasetError::Obligation {
            url: url.to_owned(),
            path: raw.id.clone().unwrap_or_else(|| raw.path.clone()),
        };
        let mut codes = extension
            .parts
            .iter()
            .filter(|part| part.url == "code")
            .map(|part| part.value_code.clone());
        let (Some(Some(code)), None) = (codes.next(), codes.next()) else {
            return Err(defective());
        };
        let actors = extension
            .parts
            .iter()
            .filter(|part| part.url == "actor")
            .map(|part| part.value_canonical.clone())
            .collect::<Option<Vec<String>>>()
            .ok_or_else(defective)?;
        list.push(Obligation { code, actors });
    }
    list.sort();
    Ok(list)
}

/// Returns the file name of an archive member the model may read: a JSON
/// file directly under `package/` that is not an index.
// NOTE: FHIR NPM Package Specification: resources sit directly under `package/`,
// so a member elsewhere, an index such as `.index.json` or a non-JSON file is
// legitimately not one and is not read.
fn resource_file(path: &Path) -> Option<&str> {
    let mut parts = path.components();
    let (Some(Component::Normal(folder)), Some(Component::Normal(file)), None) =
        (parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    let file = file.to_str()?;
    let readable = folder == "package"
        && !file.starts_with('.')
        && Path::new(file)
            .extension()
            .is_some_and(|extension| extension == "json");
    readable.then_some(file)
}

/// Parses `bytes` as `T`, naming `file` on failure.
fn parse<T: DeserializeOwned>(file: &str, bytes: &[u8]) -> Result<T, DatasetError> {
    serde_json::from_slice(bytes).map_err(|source| DatasetError::Json {
        file: file.to_owned(),
        source,
    })
}

/// Wraps an archive read failure.
fn unreadable(source: io::Error) -> DatasetError {
    DatasetError::Archive { source }
}
