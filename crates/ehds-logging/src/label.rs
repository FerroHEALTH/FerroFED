// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The security labels of an access record, by the categories and the
//! origins of the data accessed (Regulation (EU) 2025/327 Annex II 3.4).
//!
//! Annex II 3.4 asks the components that store personal electronic health
//! data to "support different retention periods and access rights that take
//! into account the origins and categories" of the data. The store of the
//! access records decides who reads one: an ATNA Audit Record Repository
//! returns only the records "which the requester is authorized to view" (IHE
//! `RESTful` ATNA Rev. 3.6 §3.81.4.1.3), and "access controls should
//! appropriately restrict access to audit records" (the same supplement's
//! ITI TF-1 §9.5). So each
//! record carries a [`Label`] per category of the data it reached and per
//! origin of the data, set when the record is written, which a store that
//! limits access by label can decide on.
//!
//! A [`LabelPolicy`] says which labels each category and each origin carries:
//!
//! - a category the policy declares nothing for is labelled with its own
//!   code, the coding [`Category`] writes;
//! - an origin the policy declares nothing for is labelled with its endpoint
//!   as the code in the policy's origin system, when it names one;
//! - an access the map could not classify, in whole or in part, carries the
//!   labels of every category the map can name, so a store that admits a
//!   reader to a record only when the reader is cleared for each of its
//!   labels shows it to a reader cleared for all of them alone.
//!
//! No adopted act fixes a label vocabulary for a category or an origin, so
//! the deployment declares it (no specification governs the labels: our own
//! design).

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::category::{Category, is_absolute_uri, is_fhir_code};
use crate::classify::Classification;
use crate::map::CategoryMap;

/// A security label: a coded value in a code system, as a FHIR R4 `Coding`
/// is, with the system's version where it is known.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Label {
    system: String,
    version: Option<String>,
    code: String,
}

impl Label {
    /// The label `code` in `system`.
    ///
    /// # Errors
    ///
    /// [`LabelCodeError::System`] for a system that is no absolute URI or
    /// holds a `|`, and [`LabelCodeError::Code`] for a code that is no FHIR
    /// `code`.
    pub fn new(system: &str, code: &str) -> Result<Self, LabelCodeError> {
        if !is_absolute_uri(system) {
            return Err(LabelCodeError::System);
        }
        if !is_fhir_code(code) {
            return Err(LabelCodeError::Code);
        }
        Ok(Self {
            system: system.to_owned(),
            version: None,
            code: code.to_owned(),
        })
    }

    /// The label of `category`: its own code, in its system, at the
    /// system's version where it is known.
    #[must_use]
    pub fn of(category: &Category) -> Self {
        Self {
            system: category.system().to_owned(),
            version: category.version().map(str::to_owned),
            code: category.code().to_owned(),
        }
    }

    /// The system.
    #[must_use]
    pub fn system(&self) -> &str {
        &self.system
    }

    /// The version of the system, when it is known.
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    /// The code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// The label as one string, `<system>|<code>`, the FHIR R4 search token
    /// form.
    #[must_use]
    pub fn token(&self) -> String {
        format!("{}|{}", self.system, self.code)
    }
}

/// One label as a configuration writes it: a code and its system.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredLabel {
    /// The code system, an absolute URI.
    pub system: String,
    /// The code.
    pub code: String,
}

/// Why a label is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LabelCodeError {
    /// The system is no absolute URI, or it holds a `|`.
    #[error("the system of a label is an absolute URI without `|`")]
    System,
    /// The code is empty, has leading or trailing whitespace, holds
    /// whitespace other than single spaces, or holds a character below U+0020.
    #[error(
        "a label code is a FHIR code: no leading, trailing or repeated whitespace, and no character below U+0020"
    )]
    Code,
}

/// Why a label policy is refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LabelError {
    /// The origin system is no absolute URI, or it holds a `|`.
    #[error("origin_system is no absolute URI without `|`")]
    OriginSystem,
    /// A category key is neither a priority category nor a national one the
    /// category map declares.
    #[error("categories key {code:?} is neither a priority category nor a declared national one")]
    UnknownCategory {
        /// The code.
        code: String,
    },
    /// Two category keys name the same category.
    #[error("categories key {code:?} names a category another key names too")]
    CategoryTwice {
        /// The second key.
        code: String,
    },
    /// An origin key is empty or holds a control character.
    #[error("an origins key is empty or holds a control character")]
    OriginKey,
    /// A key declares no label.
    #[error("{key} declares no label; name at least one")]
    Empty {
        /// The key: `categories.<code>` or `origins.<endpoint>`.
        key: String,
    },
    /// A key declares a label that is refused.
    #[error("{key} declares a label that is refused")]
    Label {
        /// The key: `categories.<code>` or `origins.<endpoint>`.
        key: String,
        /// Why.
        #[source]
        source: LabelCodeError,
    },
}

/// The labels a deployment gives its records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelPolicy {
    origin_system: Option<String>,
    categories: BTreeMap<Category, Vec<Label>>,
    origins: BTreeMap<String, Vec<Label>>,
    known: Vec<Category>,
}

impl Default for LabelPolicy {
    fn default() -> Self {
        Self {
            origin_system: None,
            categories: BTreeMap::new(),
            origins: BTreeMap::new(),
            known: Category::PRIORITY.to_vec(),
        }
    }
}

impl LabelPolicy {
    /// The policy that labels each category of `categories` and each origin
    /// of `origins` as they say, each category read as
    /// [`CategoryMap::category`] reads it, every other category with its own
    /// code, and every other origin with its endpoint in `origin_system`,
    /// when given.
    ///
    /// # Errors
    ///
    /// A [`LabelError`] for an origin system that is no absolute URI, a key
    /// that names no category or names one twice, an empty or malformed
    /// endpoint, an empty list, and a label that is refused.
    pub fn declare(
        origin_system: Option<&str>,
        categories: &BTreeMap<String, Vec<DeclaredLabel>>,
        origins: &BTreeMap<String, Vec<DeclaredLabel>>,
        map: &CategoryMap,
    ) -> Result<Self, LabelError> {
        if origin_system.is_some_and(|system| !is_absolute_uri(system)) {
            return Err(LabelError::OriginSystem);
        }
        let mut declared = BTreeMap::new();
        for (code, labels) in categories {
            let category = map
                .category(code)
                .ok_or_else(|| LabelError::UnknownCategory { code: code.clone() })?;
            let labels = read(&format!("categories.{code}"), labels)?;
            if declared.insert(category, labels).is_some() {
                return Err(LabelError::CategoryTwice { code: code.clone() });
            }
        }
        let origins = origins
            .iter()
            .map(|(endpoint, labels)| {
                if endpoint.is_empty() || endpoint.chars().any(char::is_control) {
                    return Err(LabelError::OriginKey);
                }
                Ok((
                    endpoint.clone(),
                    read(&format!("origins.{endpoint}"), labels)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, LabelError>>()?;
        Ok(Self {
            origin_system: origin_system.map(str::to_owned),
            categories: declared,
            origins,
            known: map.categories().collect(),
        })
    }

    /// The endpoints the policy names.
    pub fn origins(&self) -> impl Iterator<Item = &str> {
        self.origins.keys().map(String::as_str)
    }

    /// The labels of the record of an access classified as `classified`,
    /// whose data came from the endpoints `origins`.
    #[must_use]
    pub fn label<'a>(
        &self,
        classified: &Classification,
        origins: impl IntoIterator<Item = &'a str>,
    ) -> Labels {
        let mut categories = Vec::new();
        let reached: Vec<&Category> = if classified.unclassified().is_some() {
            self.known.iter().collect()
        } else {
            classified.categories().keys().collect()
        };
        for category in reached {
            match self.categories.get(category) {
                Some(labels) => extend(&mut categories, labels.iter().cloned()),
                None => extend(&mut categories, [Label::of(category)]),
            }
        }
        let mut labelled: Vec<(String, Vec<Label>)> = Vec::new();
        for endpoint in origins {
            if labelled.iter().any(|(seen, _)| seen == endpoint) {
                continue;
            }
            let labels = match (self.origins.get(endpoint), &self.origin_system) {
                (Some(labels), _) => labels.clone(),
                (None, Some(system)) => vec![Label {
                    system: system.clone(),
                    version: None,
                    code: endpoint.to_owned(),
                }],
                (None, None) => Vec::new(),
            };
            labelled.push((endpoint.to_owned(), labels));
        }
        Labels {
            categories,
            origins: labelled,
        }
    }
}

/// The labels one record carries: those of its categories and those of
/// each of its origins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Labels {
    categories: Vec<Label>,
    origins: Vec<(String, Vec<Label>)>,
}

impl Labels {
    /// The labels of the categories of the data accessed, each once.
    #[must_use]
    pub fn categories(&self) -> &[Label] {
        &self.categories
    }

    /// The labels of the origin `endpoint`, none when the record names no
    /// such origin.
    #[must_use]
    pub fn origin(&self, endpoint: &str) -> &[Label] {
        self.origins
            .iter()
            .find(|(named, _)| named == endpoint)
            .map_or(&[], |(_, labels)| labels.as_slice())
    }

    /// Every label of the record, each once: the categories' first, then
    /// each origin's in the order the record names the origins.
    #[must_use]
    pub fn all(&self) -> Vec<Label> {
        let mut all = self.categories.clone();
        for (_, labels) in &self.origins {
            extend(&mut all, labels.iter().cloned());
        }
        all
    }
}

/// The labels `declared` under `key`, at least one.
fn read(key: &str, declared: &[DeclaredLabel]) -> Result<Vec<Label>, LabelError> {
    if declared.is_empty() {
        return Err(LabelError::Empty {
            key: key.to_owned(),
        });
    }
    let mut labels = Vec::new();
    for label in declared {
        let label = Label::new(&label.system, &label.code).map_err(|source| LabelError::Label {
            key: key.to_owned(),
            source,
        })?;
        extend(&mut labels, [label]);
    }
    Ok(labels)
}

/// Appends each of `labels` to `kept` that it does not hold yet.
fn extend(kept: &mut Vec<Label>, labels: impl IntoIterator<Item = Label>) {
    for label in labels {
        if !kept.contains(&label) {
            kept.push(label);
        }
    }
}
