// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[store]` table: FerroFED's own PostgreSQL database, shared by every
//! replica, for the state that must both survive a restart and be one on
//! every replica.
//!
//! The database and its login role belong to FerroFED alone, never to a
//! member's CDR. The table names the connection string as `url` or
//! `url_file`, a secret; the CA certificates the database's certificate is
//! verified by, `ca_file`, the platform's roots when it is unset; how many
//! connections the gateway opens, `max_connections`; how long one statement
//! or connection may take, `timeout_ms`; and how often rows past their
//! window are deleted, `sweep_interval_s`. The connection always requires
//! TLS: a connection string that does not set `sslmode=require` is refused
//! under every profile.
//!
//! The store is needed in two cases, and a configuration that needs it and
//! sets no `[store]` is refused naming the key that needs it ([`required`]):
//! an issuer declared a national contact point
//! (`[trust.issuer.national_contact_point]`), and the stored-query registry
//! on `backend = "postgres"`. Otherwise the gateway runs with no PostgreSQL
//! database. No specification governs the store or its configuration: our
//! own design.

use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

use ferrohealth_secret::file::read_secret;
use ferrohealth_secret::secret_url::SecretUrl;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use serde::Deserialize;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::stored_queries::Backend;

/// The most connections `max_connections` may open.
// NOTE: no specification governs this: our own design; a gateway's store calls are
// short, and a bound keeps a typo from exhausting the database's connection slots.
pub const MAX_CONNECTIONS: usize = 64;

/// The key the connection string is read from.
const URL: &str = "store.url";

/// The key of the file the connection string is read from.
const URL_FILE: &str = "store.url_file";

/// `[store]` as written.
///
/// `Debug` shows the connection string as [`SecretUrl`] redacts it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Store {
    /// The PostgreSQL connection string, a URL or libpq key/value pairs; a
    /// secret. It sets `sslmode=require`.
    pub url: Option<SecretUrl>,
    /// A file holding [`Store::url`], read at boot.
    pub url_file: Option<PathBuf>,
    /// A PEM file of the CA certificates the database's certificate is
    /// verified by, in place of the platform's roots; unset, the platform's
    /// roots verify it.
    pub ca_file: Option<PathBuf>,
    /// How many connections the gateway opens to the database, 1 to
    /// [`MAX_CONNECTIONS`].
    pub max_connections: usize,
    /// How long one connection, or one statement, may take, in
    /// milliseconds; zero is refused.
    pub timeout_ms: u64,
    /// How often the rows past their window are deleted, in seconds; zero
    /// is refused.
    pub sweep_interval_s: u64,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            url: None,
            url_file: None,
            ca_file: None,
            max_connections: 4,
            timeout_ms: 10_000,
            sweep_interval_s: 60,
        }
    }
}

/// `[store]`, resolved: the connection string read and checked, the CA
/// certificates read.
///
/// `Debug` redacts the connection string, because [`SecretUrl`] does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreSettings {
    /// The connection string; it requires TLS.
    pub url: SecretUrl,
    /// The PEM CA certificates `ca_file` holds, when it is set.
    pub ca: Option<String>,
    /// How many connections the gateway opens.
    pub max_connections: NonZeroUsize,
    /// How long one connection, or one statement, may take.
    pub timeout: Duration,
    /// How often the rows past their window are deleted.
    pub sweep_interval: Duration,
}

/// A `[store]` the gateway cannot use, or a configuration that needs one.
///
/// No variant carries any part of the connection string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StoreFault {
    /// The configuration needs FerroFED's own store and sets no `[store]`.
    #[error(
        "{key} needs FerroFED's own PostgreSQL store, and the configuration sets no [store]; add [store] with url or url_file"
    )]
    Required {
        /// The key that needs the store.
        key: String,
    },
    /// `[stored_queries]` still sets its own connection string, which the
    /// `postgres` backend no longer reads.
    #[error(
        "{key} is no longer read: the postgres backend of the stored-query registry uses FerroFED's own store; move the connection string to [store] url or url_file"
    )]
    UrlMoved {
        /// The removed key that is set.
        key: String,
    },
    /// The connection string does not require TLS.
    #[error(
        "{key} does not require TLS: FerroFED's store is reached over TLS under every profile; set sslmode=require"
    )]
    TlsOff {
        /// The key the string was read from.
        key: String,
    },
    /// The connection string does not parse.
    ///
    /// The parser's message is not kept: it may quote a character of the
    /// string, which holds a password.
    #[error("{key} is not a PostgreSQL connection string, a URL or libpq key/value pairs")]
    Url {
        /// The key the string was read from.
        key: String,
    },
    /// `ca_file` holds no PEM certificate.
    #[error("store.ca_file names {}, which holds no PEM certificate", path.display())]
    Ca {
        /// The file it names.
        path: PathBuf,
    },
    /// `max_connections` is outside 1 to [`MAX_CONNECTIONS`].
    #[error("store.max_connections is {value}, outside 1 to {MAX_CONNECTIONS}")]
    Connections {
        /// The value written.
        value: usize,
    },
}

/// Resolves `[store]` of `config`, refusing a configuration that needs the
/// store and sets none ([`required`]).
///
/// # Errors
///
/// [`StoreFault::Required`] naming the key that needs the store when
/// `[store]` is unset; [`Error::Missing`] naming `store.url` when neither it
/// nor `store.url_file` is set; [`StoreFault::Url`] for a connection string
/// that does not parse and [`StoreFault::TlsOff`] for one that does not set
/// `sslmode=require`, each naming the key it was read from;
/// [`StoreFault::Ca`] and [`Error::Secret`] for a `ca_file` that holds no
/// certificate or cannot be read; [`StoreFault::Connections`] and
/// [`Error::Zero`] for a bound out of range; and the secret errors of a
/// `url_file`.
pub fn resolve(config: &Config) -> Result<Option<StoreSettings>, Error> {
    let Some(section) = &config.store else {
        return match required(config) {
            Some(key) => Err(StoreFault::Required { key }.into()),
            None => Ok(None),
        };
    };
    let url = read_secret(URL, section.url.clone(), section.url_file.as_deref())?
        .filter(|url: &SecretUrl| !url.expose().trim().is_empty())
        .ok_or_else(|| Error::Missing {
            key: String::from(URL),
        })?;
    let key = String::from(if section.url_file.is_some() {
        URL_FILE
    } else {
        URL
    });
    if !crate::config::postgres::parses(&url) {
        return Err(StoreFault::Url { key }.into());
    }
    if !crate::config::postgres::requires_tls(&url) {
        return Err(StoreFault::TlsOff { key }.into());
    }
    let ca = section.ca_file.as_ref().map(ca).transpose()?;
    let max_connections = NonZeroUsize::new(section.max_connections)
        .filter(|count| count.get() <= MAX_CONNECTIONS)
        .ok_or(StoreFault::Connections {
            value: section.max_connections,
        })?;
    Ok(Some(StoreSettings {
        url,
        ca,
        max_connections,
        timeout: Duration::from_millis(positive("store.timeout_ms", section.timeout_ms)?),
        sweep_interval: Duration::from_secs(positive(
            "store.sweep_interval_s",
            section.sweep_interval_s,
        )?),
    }))
}

/// The key of the first setting of `config` that needs FerroFED's own
/// store: an issuer declared a national contact point, then the
/// stored-query registry on the `postgres` backend.
#[must_use]
pub fn required(config: &Config) -> Option<String> {
    // NOTE: no specification governs this: our own design; a contact point's limits must
    // hold across replicas and restarts, so its issuer is never served without the store.
    let contact_point = config
        .trust
        .issuer
        .iter()
        .position(|issuer| issuer.national_contact_point.is_some())
        .map(|index| format!("trust.issuer[{index}].national_contact_point"));
    contact_point.or_else(|| {
        (config.stored_queries.backend == Some(Backend::Postgres))
            .then(|| String::from("stored_queries.backend"))
    })
}

/// `value` of `key`, refused at zero.
fn positive(key: &str, value: u64) -> Result<u64, Error> {
    if value == 0 {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(value)
}

/// The PEM CA certificates `path` holds.
fn ca(path: &PathBuf) -> Result<String, Error> {
    let pem = std::fs::read_to_string(path).map_err(|source| Error::Secret {
        key: String::from("store.ca_file"),
        path: path.clone(),
        source,
    })?;
    let certificates: Result<Vec<CertificateDer<'_>>, _> =
        CertificateDer::pem_slice_iter(pem.as_bytes()).collect();
    match certificates {
        Ok(certificates) if !certificates.is_empty() => Ok(pem),
        Ok(_) | Err(_) => Err(StoreFault::Ca { path: path.clone() }.into()),
    }
}
