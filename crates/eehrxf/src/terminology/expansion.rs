// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value set expansions a FHIR package carries.
//!
//! A `ValueSet` may carry its `expansion`, the list of codes it holds at the
//! time it was expanded (<https://hl7.org/fhir/R4/valueset.html#expansion>).
//! [`Expansions::read`] keeps every expansion of a package, keyed on the
//! value set's canonical URL, with every code of its `contains`, nested
//! entries included. A value set the package carries without an expansion is
//! not kept: the check of a binding to it names it as not evaluated.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io;
use std::io::Read;
use std::path::Component;
use std::path::Path;

use flate2::read::GzDecoder;
use serde::Deserialize;

/// The codes of every expanded value set of one or more packages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansions {
    by_url: BTreeMap<String, BTreeSet<Member>>,
}

/// One code of an expansion, with its system.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Member {
    system: Option<String>,
    code: String,
}

impl Expansions {
    /// Reads every expanded `ValueSet` of a FHIR package archive, the
    /// gzip-compressed tarball a FHIR package registry serves.
    ///
    /// # Errors
    ///
    /// [`ExpansionError::Archive`] when the archive cannot be read,
    /// [`ExpansionError::Json`] for a resource file that does not parse, and
    /// [`ExpansionError::Twice`] for two value sets that carry one URL.
    pub fn read(archive: impl Read) -> Result<Self, ExpansionError> {
        let mut expansions = Self::default();
        expansions.add(archive)?;
        Ok(expansions)
    }

    /// Adds every expanded `ValueSet` of another package archive.
    ///
    /// # Errors
    ///
    /// The errors of [`Expansions::read`], and [`ExpansionError::Twice`] for
    /// a value set this set already holds.
    pub fn add(&mut self, archive: impl Read) -> Result<(), ExpansionError> {
        let mut tarball = tar::Archive::new(GzDecoder::new(archive));
        for entry in tarball.entries().map_err(unreadable)? {
            let mut entry = entry.map_err(unreadable)?;
            if !entry.header().entry_type().is_file() {
                continue;
            }
            let path = entry.path().map_err(unreadable)?.into_owned();
            if !resource_file(&path) {
                continue;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).map_err(unreadable)?;
            let file = path.to_string_lossy().into_owned();
            let resource: ValueSet = serde_json::from_slice(&bytes)
                .map_err(|source| ExpansionError::Json { file, source })?;
            if resource.resource_type.as_deref() != Some("ValueSet") {
                continue;
            }
            let (Some(url), Some(expansion)) = (resource.url, resource.expansion) else {
                continue;
            };
            let mut members = BTreeSet::new();
            collect(&expansion.contains, &mut members);
            if self.by_url.insert(url.clone(), members).is_some() {
                return Err(ExpansionError::Twice { url });
            }
        }
        Ok(())
    }

    /// Returns whether the value set `url` has an expansion here.
    #[must_use]
    pub fn has(&self, url: &str) -> bool {
        self.by_url.contains_key(url)
    }

    /// Returns whether the expansion of `url` holds `code` in `system`, or
    /// any system when `system` is `None`; `None` when there is no
    /// expansion of `url`.
    #[must_use]
    pub fn contains(&self, url: &str, system: Option<&str>, code: &str) -> Option<bool> {
        let members = self.by_url.get(url)?;
        Some(members.iter().any(|member| {
            member.code == code
                && system.is_none_or(|system| member.system.as_deref() == Some(system))
        }))
    }

    /// Returns the URL of every expanded value set, in order.
    pub fn urls(&self) -> impl Iterator<Item = &str> {
        self.by_url.keys().map(String::as_str)
    }
}

/// Adds every code of `contains`, nested entries included.
fn collect(contains: &[Entry], members: &mut BTreeSet<Member>) {
    for entry in contains {
        if let Some(code) = &entry.code {
            members.insert(Member {
                system: entry.system.clone(),
                code: code.clone(),
            });
        }
        collect(&entry.contains, members);
    }
}

/// The `ValueSet` members the expansion is read from.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ValueSet {
    resource_type: Option<String>,
    url: Option<String>,
    expansion: Option<Expansion>,
}

/// A `ValueSet.expansion`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Expansion {
    contains: Vec<Entry>,
}

/// One `ValueSet.expansion.contains`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Entry {
    system: Option<String>,
    code: Option<String>,
    contains: Vec<Self>,
}

/// Why a package gives no expansions.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ExpansionError {
    /// The archive could not be read as a gzip-compressed tarball.
    #[error("the package archive cannot be read")]
    Archive {
        /// The read failure.
        #[source]
        source: io::Error,
    },
    /// A resource file of the package does not parse.
    #[error("{file} does not parse")]
    Json {
        /// The file, as its archive path.
        file: String,
        /// The parse failure.
        #[source]
        source: serde_json::Error,
    },
    /// Two expanded value sets carry one canonical URL.
    #[error("two expanded value sets carry the canonical URL {url}")]
    Twice {
        /// The URL.
        url: String,
    },
}

/// Returns whether an archive member is a resource file: a JSON file
/// directly under `package/` that is not an index.
// NOTE: FHIR NPM Package Specification: resources sit directly under `package/`,
// so a member elsewhere, an index such as `.index.json` or a non-JSON file is
// legitimately not one and is not read.
fn resource_file(path: &Path) -> bool {
    let mut parts = path.components();
    let (Some(Component::Normal(folder)), Some(Component::Normal(file)), None) =
        (parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some(file) = file.to_str() else {
        return false;
    };
    folder == "package"
        && file != "package.json"
        && !file.starts_with('.')
        && Path::new(file)
            .extension()
            .is_some_and(|extension| extension == "json")
}

/// Wraps an archive read failure.
fn unreadable(source: io::Error) -> ExpansionError {
    ExpansionError::Archive { source }
}
