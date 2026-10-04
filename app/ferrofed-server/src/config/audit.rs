// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the audit records of the FHIR profiles' transactions go, `[audit]`:
//! PIXm ITI-83 (`[pixm]`), mCSD ITI-90 and ITI-91 (`[registry.mcsd]`), and
//! PMIR ITI-93 and ITI-94 (`[pmir]`).
//!
//! ```toml
//! [audit]
//! destination = "repository"
//!
//! [audit.repository]
//! url = "https://arr.example.org/fhir"
//! hostname = "gateway.example.org"
//! spool_dir = "/var/lib/ferrofed/audit-feed-spool"
//! client_identity_file = "/run/secrets/atna-client.pem"
//! trust_roots_file = "/etc/ferrofed/atna-roots.pem"
//! ```
//!
//! Each profile defines its records as a BALP `AuditEvent` (PIXm
//! §2:3.83.5.1, mCSD §2:3.90.5.1 and §2:3.91.5.1, PMIR §2:3.93.5.1 and
//! §2:3.94.5.1), which BALP sends over the ATX: FHIR Feed Option of ITI-20
//! (BALP §1:52.1.1.1): `destination = "repository"` posts each record to
//! the Audit Record Repository's FHIR base, `log` writes it to the
//! `ferrofed::audit` log target without a patient identifier, and `off`,
//! which only `profile = "development"` admits, records nothing. The
//! repository is reached over `https`; plain `http` is admitted under
//! development alone, through the protected-payload policy of
//! [`transport`](crate::config::transport): every record names the patient.
//! A record is stored in the spool before it is delivered; outside
//! development the spool is a directory, which survives a restart. No
//! specification governs the shape of the table: our own design.

use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::Secret;
use ihe_iti::atna::spool::Bounds;
use ihe_iti::balp::{NetworkAddress, Observer};
use serde::Deserialize;
use url::Url;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::secrets::secret;
use crate::config::xcpd::AuditDestination;

/// `[audit]`, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Audit {
    /// Where the records go: `repository`, `log`, or `off`, which only
    /// `profile = "development"` admits. Outside development it has no
    /// default once a PIXm, mCSD or PMIR binding is configured.
    pub destination: Option<AuditDestination>,
    /// The Audit Record Repository, under `destination = "repository"`.
    pub repository: Option<FeedRepository>,
}

/// `[audit.repository]`, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeedRepository {
    /// The repository's FHIR base, `https`; `http` under development only.
    pub url: String,
    /// The machine name or IP address each record names as the gateway's
    /// network address.
    pub hostname: String,
    /// The name each record gives the gateway (`source.observer` and its
    /// agent's `who`), the hostname when unset.
    pub source_id: Option<String>,
    /// `source.site`, when the deployment names one.
    pub enterprise_site: Option<String>,
    /// The spool directory; required outside development.
    pub spool_dir: Option<PathBuf>,
    /// The most bytes the spool holds.
    pub spool_max_bytes: u64,
    /// The most records the spool holds.
    pub spool_max_events: usize,
    /// How long one delivery may take.
    pub timeout_ms: u64,
    /// The longest wait between two delivery attempts.
    pub retry_max_ms: u64,
    /// The gateway's client certificate chain and private key, PEM, inline
    /// or through `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the repository's certificate chains to,
    /// beside the platform's.
    pub trust_roots_file: Option<PathBuf>,
}

impl Default for FeedRepository {
    fn default() -> Self {
        Self {
            url: String::new(),
            hostname: String::new(),
            source_id: None,
            enterprise_site: None,
            spool_dir: None,
            spool_max_bytes: 64 * 1024 * 1024,
            spool_max_events: 100_000,
            timeout_ms: 5_000,
            retry_max_ms: 60_000,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
        }
    }
}

/// `[audit]`, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditSettings {
    /// Where the records go.
    pub destination: AuditDestination,
    /// The repository, under [`AuditDestination::Repository`].
    pub repository: Option<FeedRepositorySettings>,
}

impl Default for AuditSettings {
    fn default() -> Self {
        Self {
            destination: AuditDestination::Off,
            repository: None,
        }
    }
}

/// `[audit.repository]`, with every value checked and every file read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedRepositorySettings {
    /// The repository's FHIR base.
    pub url: Url,
    /// Whether the base is plain `http`, which development alone admits.
    pub cleartext: bool,
    /// The gateway as each record names it.
    pub observer: Observer,
    /// The spool directory, or `None` for a spool in memory.
    pub spool_dir: Option<PathBuf>,
    /// The spool's bounds.
    pub bounds: Bounds,
    /// How long one delivery may take.
    pub timeout: Duration,
    /// The longest wait between two delivery attempts.
    pub retry_max: Duration,
    /// The client certificate chain and key.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots.
    pub trust_roots: Option<String>,
}

/// The key of the table.
const KEY: &str = "audit";

/// Resolves `[audit]` under the configuration `config`.
///
/// # Errors
///
/// [`Error::Missing`] for no `destination` outside development while a
/// PIXm, mCSD or PMIR binding is configured, for no `[audit.repository]`
/// under `repository`, and for no `url`, `hostname` or, outside
/// development, `spool_dir`; [`Error::FeedAuditOff`] for `off` outside
/// development; [`Error::FeedAuditRepositoryUnused`] for a repository under
/// another destination; [`Error::Url`] for a `url` that does not parse;
/// [`Error::Zero`] for a zero bound or timeout; and the errors of a secret or
/// a file that cannot be read.
pub(super) fn resolve(config: &Config) -> Result<AuditSettings, Error> {
    let profile = config.profile;
    let table = &config.audit;
    let audited = config.pixm.is_some() || config.registry.mcsd.is_some() || config.pmir.is_some();
    // NOTE: PIXm §2:3.83.5.1, mCSD §2:3.90.5.1, PMIR §2:3.93.5.1 have each actor
    // record its transactions, so no audit at all is a development-only choice.
    let destination = match table.destination {
        Some(AuditDestination::Off) if profile != Profile::Development => {
            return Err(Error::FeedAuditOff {
                key: format!("{KEY}.destination"),
            });
        }
        Some(destination) => destination,
        None if audited && profile != Profile::Development => {
            return Err(Error::Missing {
                key: format!("{KEY}.destination"),
            });
        }
        None => AuditDestination::Off,
    };
    let repository = match (destination, &table.repository) {
        (AuditDestination::Repository, Some(repository)) => {
            Some(resolve_repository(profile, repository)?)
        }
        (AuditDestination::Repository, None) => {
            return Err(Error::Missing {
                key: format!("{KEY}.repository"),
            });
        }
        (_, Some(_)) => return Err(Error::FeedAuditRepositoryUnused),
        (_, None) => None,
    };
    Ok(AuditSettings {
        destination,
        repository,
    })
}

/// Resolves `[audit.repository]` under `profile`.
fn resolve_repository(
    profile: Profile,
    table: &FeedRepository,
) -> Result<FeedRepositorySettings, Error> {
    let key = |field: &str| format!("{KEY}.repository.{field}");
    for (field, empty) in [
        ("url", table.url.is_empty()),
        ("hostname", table.hostname.is_empty()),
    ] {
        if empty {
            return Err(Error::Missing { key: key(field) });
        }
    }
    let url = Url::parse(&table.url).map_err(|source| Error::Url {
        key: key("url"),
        source,
    })?;
    if table.spool_dir.is_none() && profile != Profile::Development {
        return Err(Error::Missing {
            key: key("spool_dir"),
        });
    }
    for (field, zero) in [
        ("spool_max_bytes", table.spool_max_bytes == 0),
        ("spool_max_events", table.spool_max_events == 0),
        ("timeout_ms", table.timeout_ms == 0),
        ("retry_max_ms", table.retry_max_ms == 0),
    ] {
        if zero {
            return Err(Error::Zero { key: key(field) });
        }
    }
    let client_identity = secret(
        &key("client_identity"),
        table.client_identity.as_ref(),
        table.client_identity_file.as_deref(),
    )?;
    let trust_roots = table
        .trust_roots_file
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: key("trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(FeedRepositorySettings {
        cleartext: url.scheme() != "https",
        url,
        observer: Observer {
            source_id: table
                .source_id
                .clone()
                .unwrap_or_else(|| table.hostname.clone()),
            site: table.enterprise_site.clone(),
            host: NetworkAddress::host(&table.hostname),
        },
        spool_dir: table.spool_dir.clone(),
        bounds: Bounds {
            max_messages: table.spool_max_events,
            max_bytes: table.spool_max_bytes,
        },
        timeout: Duration::from_millis(table.timeout_ms),
        retry_max: Duration::from_millis(table.retry_max_ms),
        client_identity,
        trust_roots,
    })
}
