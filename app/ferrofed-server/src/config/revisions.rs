// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[config]` table: whether the configuration is also kept as
//! versioned revisions an operator saves, applies and rolls back on the
//! admin listener, and where those revisions are kept.
//!
//! `ui = "off"`, the default, leaves the gateway configured by its file and
//! environment alone, and every other key of the table is refused under it.
//! `ui = "managed"` keeps the revisions in a store (`crate::revision`):
//!
//! - `redb`, the default: the embedded store file `path` names, which one
//!   gateway process opens at a time;
//! - `postgres`: one PostgreSQL database every replica shares, reached at the
//!   connection string `url` or `url_file` holds, a secret; only a build with
//!   the `postgres` feature offers it.
//!
//! Under `managed`, `poll_ms` sets how often each gateway reads the running
//! mark, to apply a revision another replica marked running ([`poll`]).
//!
//! The table is part of the bootstrap core, which a revision never holds
//! ([`crate::revision::tree::CORE`]). No specification governs this: our own
//! design.

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_registry::secret::SecretUrl;
use serde::Deserialize;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::secrets::secret;

/// Whether the configuration is managed as revisions, `config.ui`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Ui {
    /// The file and the environment alone configure the gateway, as they
    /// always have.
    #[default]
    Off,
    /// The configuration is also kept as revisions on the admin listener.
    Managed,
}

impl Ui {
    /// The value as `config.ui` spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Managed => "managed",
        }
    }
}

/// The backend the revisions are kept in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// The embedded `redb` store file, for one gateway process.
    #[default]
    Redb,
    /// A PostgreSQL database several gateway replicas share.
    Postgres,
}

impl Backend {
    /// The backend as `config.backend` spells it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Redb => "redb",
            Self::Postgres => "postgres",
        }
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The `[config]` table, as written.
///
/// `Debug` shows the connection string as [`SecretUrl`] redacts it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Revisions {
    /// Whether the configuration is managed as revisions: `off`, the
    /// default, or `managed`.
    pub ui: Ui,
    /// The backend of the revision store; `redb` when the table sets none.
    pub backend: Option<Backend>,
    /// The `redb` store file, created when it does not exist.
    pub path: Option<PathBuf>,
    /// The PostgreSQL connection string of the `postgres` backend, a URL or
    /// libpq key/value pairs; a secret.
    pub url: Option<SecretUrl>,
    /// A file holding [`Revisions::url`], read at boot.
    pub url_file: Option<PathBuf>,
    /// How often, in milliseconds, the gateway reads the store's running
    /// mark, to apply a revision another replica marked running; 5000 when
    /// the table sets none.
    pub poll_ms: Option<u64>,
}

/// The store the revisions are kept in, resolved.
///
/// `Debug` redacts the connection string, because [`SecretUrl`] does.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Store {
    /// The `redb` store file.
    Redb(PathBuf),
    /// The PostgreSQL connection string.
    Postgres(SecretUrl),
}

impl Store {
    /// The backend this store is opened with.
    #[must_use]
    pub const fn backend(&self) -> Backend {
        match self {
            Self::Redb(_) => Backend::Redb,
            Self::Postgres(_) => Backend::Postgres,
        }
    }

    /// Whether `other` opens the same store.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Redb(a), Self::Redb(b)) => a == b,
            (Self::Postgres(a), Self::Postgres(b)) => a == b,
            _ => false,
        }
    }
}

/// Why the `[config]` table is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RevisionsFault {
    /// A store key is set while `config.ui` is `off`.
    #[error(
        "{key} is set while config.ui = \"off\"; the revision store is read only under config.ui = \"managed\""
    )]
    Off {
        /// The key that is set.
        key: &'static str,
    },
    /// A key the chosen backend does not read is set.
    #[error("{key} is not read by config.backend = \"{backend}\"")]
    Key {
        /// The key that is set.
        key: &'static str,
        /// The backend chosen.
        backend: Backend,
    },
    /// The chosen backend is not built into this binary.
    #[error(
        "config.backend = \"{backend}\" is not built into this binary; build ferrofed-server with its {backend} feature"
    )]
    Unavailable {
        /// The backend chosen.
        backend: Backend,
    },
    /// `config.poll_ms` is outside the bounds it is read within.
    #[error(
        "config.poll_ms is {value}; it must be from {POLL_MIN_MS} to {POLL_MAX_MS} milliseconds"
    )]
    Poll {
        /// The value set.
        value: u64,
    },
    /// The PostgreSQL connection string does not parse; the parser's message
    /// is not kept, since it may quote the password.
    #[error("{key} is not a PostgreSQL connection string, a URL or libpq key/value pairs")]
    Url {
        /// The key the string was read from.
        key: &'static str,
    },
    /// `config.ui = "managed"` without the admin listener its revision API
    /// is served on.
    #[error(
        "config.ui = \"managed\" serves its revisions on the admin listener; set metrics.listen"
    )]
    NoAdminListener,
    /// A configuration under `config.ui = "managed"` holds a secret inline,
    /// which a revision never holds.
    #[error(
        "{key} holds a secret inline, and under config.ui = \"managed\" a revision holds a secret only as a reference to a file: set its _file sibling"
    )]
    InlineSecret {
        /// The dotted key of the secret, never its value.
        key: String,
    },
}

/// Resolves `[config]` of `config`: the revision store when the
/// configuration is managed, and `None` when it is not.
///
/// # Errors
///
/// [`RevisionsFault::Off`] for a store key under `ui = "off"`,
/// [`RevisionsFault::NoAdminListener`] for `managed` without
/// `metrics.listen`, [`RevisionsFault::Key`] for a key the backend does not
/// read, [`Error::Missing`] naming `config.path` or `config.url` when the
/// backend needs it, [`RevisionsFault::Unavailable`] for `postgres` in a
/// build without it, [`RevisionsFault::Url`] for a connection string that
/// does not parse, and the secret errors of a `url_file`.
pub fn resolve(config: &Config) -> Result<Option<Store>, Error> {
    let section = &config.revisions;
    let keys = [
        ("config.backend", section.backend.is_some()),
        ("config.path", section.path.is_some()),
        ("config.url", section.url.is_some()),
        ("config.url_file", section.url_file.is_some()),
    ];
    if section.ui == Ui::Off {
        // NOTE: no specification governs this: our own design; with the switch off
        // nothing of the store is read, so a key that would configure it is refused.
        return match keys.iter().find(|(_, set)| *set) {
            Some((key, _)) => Err(RevisionsFault::Off { key }.into()),
            None if section.poll_ms.is_some() => Err(RevisionsFault::Off {
                key: "config.poll_ms",
            }
            .into()),
            None => Ok(None),
        };
    }
    if config.metrics.listen.is_none() {
        return Err(RevisionsFault::NoAdminListener.into());
    }
    let backend = section.backend.unwrap_or_default();
    let store = match backend {
        Backend::Redb => {
            refuse_keys(&keys[2..], backend)?;
            match &section.path {
                Some(path) if !path.as_os_str().is_empty() => Store::Redb(path.clone()),
                Some(_) | None => {
                    return Err(Error::Missing {
                        key: String::from("config.path"),
                    });
                }
            }
        }
        Backend::Postgres => {
            refuse_keys(&keys[1..2], backend)?;
            Store::Postgres(url(section)?)
        }
    };
    Ok(Some(store))
}

/// The shortest `config.poll_ms` read, in milliseconds.
pub const POLL_MIN_MS: u64 = 100;

/// The longest `config.poll_ms` read, in milliseconds: ten minutes.
pub const POLL_MAX_MS: u64 = 600_000;

/// How often the gateway reads the store's running mark, `config.poll_ms`:
/// 5000 ms when the table sets none.
///
/// # Errors
///
/// [`RevisionsFault::Poll`] for a value under [`POLL_MIN_MS`] or over
/// [`POLL_MAX_MS`].
pub fn poll(section: &Revisions) -> Result<Duration, Error> {
    // NOTE: no specification governs this: our own design; five seconds keeps a
    // replica out of step briefly at one small read of the mark per replica.
    let value = section.poll_ms.unwrap_or(5_000);
    if !(POLL_MIN_MS..=POLL_MAX_MS).contains(&value) {
        return Err(RevisionsFault::Poll { value }.into());
    }
    Ok(Duration::from_millis(value))
}

/// Refuses the first of `keys` that is set, which `backend` does not read.
fn refuse_keys(keys: &[(&'static str, bool)], backend: Backend) -> Result<(), Error> {
    match keys.iter().find(|(_, set)| *set) {
        Some((key, _)) => Err(RevisionsFault::Key { key, backend }.into()),
        None => Ok(()),
    }
}

/// The connection string of the `postgres` backend, checked to parse.
fn url(section: &Revisions) -> Result<SecretUrl, Error> {
    if !cfg!(feature = "postgres") {
        return Err(RevisionsFault::Unavailable {
            backend: Backend::Postgres,
        }
        .into());
    }
    let key = "config.url";
    let url = secret(key, section.url.as_ref(), section.url_file.as_deref())?
        .filter(|url: &SecretUrl| !url.expose().trim().is_empty())
        .ok_or_else(|| Error::Missing {
            key: String::from(key),
        })?;
    #[cfg(feature = "postgres")]
    if !crate::stored::postgres::parses(&url) {
        let key = if section.url_file.is_some() {
            "config.url_file"
        } else {
            key
        };
        return Err(RevisionsFault::Url { key }.into());
    }
    Ok(url)
}

/// Refuses `tree`, the merged configuration, when the configuration is
/// managed and a part a revision holds carries a secret inline
/// ([`crate::revision::tree::inline_secrets`]).
///
/// # Errors
///
/// [`RevisionsFault::InlineSecret`] naming the first such key.
pub fn check_tree(store: Option<&Store>, tree: &toml::Table) -> Result<(), Error> {
    if store.is_none() {
        return Ok(());
    }
    let (_core, revision) = crate::revision::tree::split(tree);
    match crate::revision::tree::inline_secrets(&revision)
        .into_iter()
        .next()
    {
        Some(key) => Err(RevisionsFault::InlineSecret { key }.into()),
        None => Ok(()),
    }
}
