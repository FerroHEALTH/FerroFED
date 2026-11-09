// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[stored_queries]` table: where the federated stored-query registry
//! keeps its definitions (§12.7, N44).
//!
//! Setting the table offers the registry, which `OPTIONS {base}/` declares
//! (§7a.2), over one of three backends:
//!
//! - `redb`, the default: the embedded store file `path` names, which one
//!   gateway process opens at a time;
//! - `postgres`: FerroFED's own PostgreSQL database, which every replica
//!   shares, configured by `[store]` ([`crate::config::store`]); the table's
//!   former `url` and `url_file` are refused, naming `[store]`;
//! - `files`: read-only, one file per definition under the directory `path`
//!   names, loaded at start, so a `PUT` is refused.
//!
//! No specification governs the store or its configuration: our own design.

use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;
use serde::de::IgnoredAny;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::error::federation::FederationFault;
use crate::config::store::StoreFault;

/// The backend of the stored-query registry.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// The embedded `redb` store file, for one gateway process.
    #[default]
    Redb,
    /// A PostgreSQL database several gateway replicas share.
    Postgres,
    /// Read-only definition files loaded at start.
    Files,
}

impl Backend {
    /// The backend as `stored_queries.backend` spells it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Redb => "redb",
            Self::Postgres => "postgres",
            Self::Files => "files",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The federated stored-query registry, as written.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StoredQueries {
    /// The backend; `redb` when the table sets none.
    pub backend: Option<Backend>,
    /// The `redb` store file, created when it does not exist, or the
    /// directory the `files` backend reads.
    pub path: Option<PathBuf>,
    /// The removed connection string of the `postgres` backend, read only
    /// so that a configuration still setting it is refused naming `[store]`
    /// ([`StoreFault::UrlMoved`]); its value is never kept.
    pub url: Option<IgnoredAny>,
    /// The removed file of the connection string, refused as
    /// [`StoredQueries::url`] is.
    pub url_file: Option<IgnoredAny>,
}

/// The store the stored-query registry is opened over, resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Store {
    /// The `redb` store file.
    Redb(PathBuf),
    /// FerroFED's own PostgreSQL store, which `[store]` configures.
    Postgres,
    /// The directory of read-only definition files.
    Files(PathBuf),
}

impl Store {
    /// The backend this store is opened with.
    #[must_use]
    pub fn backend(&self) -> Backend {
        match self {
            Self::Redb(_) => Backend::Redb,
            Self::Postgres => Backend::Postgres,
            Self::Files(_) => Backend::Files,
        }
    }

    /// Whether `other` opens the same store, so a reload that reads it
    /// needs no restart; a change of `[store]` is a restart key of its own.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Redb(a), Self::Redb(b)) | (Self::Files(a), Self::Files(b)) => a == b,
            (Self::Postgres, Self::Postgres) => true,
            _ => false,
        }
    }
}

/// Resolves `[stored_queries]` of `config`: the store, when the registry is
/// offered.
///
/// # Errors
///
/// [`StoreFault::UrlMoved`] for the removed `stored_queries.url` or
/// `stored_queries.url_file`; [`Error::Missing`] naming `stored_queries.path`
/// when the backend needs it and it is unset or empty, and naming
/// `registry.document` when the registry is offered with neither a registry
/// document nor a directory, and so without the federation that executes
/// its queries; and [`FederationFault::StoreKey`] for a key the backend does
/// not read. The `postgres` backend without `[store]` is refused by
/// [`crate::config::store::resolve`].
pub fn resolve(config: &Config) -> Result<Option<Store>, Error> {
    let section = &config.stored_queries;
    for (key, set) in [
        ("stored_queries.url", section.url.is_some()),
        ("stored_queries.url_file", section.url_file.is_some()),
    ] {
        if set {
            return Err(StoreFault::UrlMoved {
                key: String::from(key),
            }
            .into());
        }
    }
    if section.backend.is_none() && section.path.is_none() {
        return Ok(None);
    }
    let backend = section.backend.unwrap_or_default();
    let store = match backend {
        Backend::Redb => Store::Redb(path(section)?),
        Backend::Files => Store::Files(path(section)?),
        Backend::Postgres => {
            if section.path.is_some() {
                return Err(Error::Federation(FederationFault::StoreKey {
                    key: String::from("stored_queries.path"),
                    backend,
                }));
            }
            Store::Postgres
        }
    };
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    Ok(Some(store))
}

/// The `path` a path backend reads.
fn path(section: &StoredQueries) -> Result<PathBuf, Error> {
    match &section.path {
        Some(path) if !path.as_os_str().is_empty() => Ok(path.clone()),
        Some(_) | None => Err(Error::Missing {
            key: String::from("stored_queries.path"),
        }),
    }
}
