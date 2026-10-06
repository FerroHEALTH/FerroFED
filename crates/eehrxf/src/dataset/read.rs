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
}

/// The `StructureDefinition` members the model reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StructureDefinition {
    url: String,
    name: String,
    kind: Option<String>,
    derivation: Option<String>,
    base_definition: Option<String>,
    snapshot: Option<Snapshot>,
}

/// The snapshot of a `StructureDefinition`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Snapshot {
    element: Vec<ElementDefinition>,
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
}

/// One `ElementDefinition.type`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct TypeRef {
    code: String,
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
    let mut tarball = tar::Archive::new(GzDecoder::new(archive));
    let mut manifest = None;
    let mut logical = Vec::new();
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
        if parse::<Head>(&archived, &bytes)?.resource_type.as_deref() != Some("StructureDefinition")
        {
            continue;
        }
        let definition = parse::<StructureDefinition>(&archived, &bytes)?;
        if definition.kind.as_deref() == Some("logical") {
            logical.push(definition);
        }
    }
    let manifest = manifest.ok_or(DatasetError::MissingManifest)?;
    assemble(
        PackageId {
            name: manifest.name,
            version: manifest.version,
        },
        logical,
    )
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
    snapshot: Vec<ElementDefinition>,
    mut visit: impl FnMut(&ElementDefinition) -> Result<(), DatasetError>,
) -> Result<Elements, DatasetError> {
    let mut list = Vec::with_capacity(snapshot.len());
    let mut index = BTreeMap::new();
    for raw in snapshot {
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
    Ok(Element {
        path: ElementPath(id),
        cardinality: Cardinality { min, max },
        types: raw
            .types
            .into_iter()
            .map(|reference| reference.code)
            .collect(),
        short: raw.short,
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
