// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where the access records and the IHE binding's BALP records go, `[audit]`.
//!
//! The access log of every federated query, stored-query execution, routed
//! read and routed write writes its records here (`crate::access`;
//! Regulation (EU) 2025/327 Annex II 3.2) in every build, so a gateway with
//! a registry needs a destination outside development whatever its
//! bindings. Under the IHE binding the PIXm ITI-83 (`[pixm]`), PDQm ITI-78
//! and ITI-119 (`[pdqm]`), mCSD ITI-90 and ITI-91 (`[registry.mcsd]`), and
//! PMIR ITI-93 and ITI-94 (`[pmir]`) transactions are recorded here too.
//!
//! ```toml
//! [audit]
//! destination = "repository"
//!
//! [audit.repository]
//! url = "https://arr.example.org/fhir"
//! hostname = "gateway.example.org"
//! spool_dir = "/var/lib/ferrofed/audit-feed-spool"
//!
//! [audit.repository.tls]
//! client_identity_file = "/run/secrets/atna-client.pem"
//! trust_roots_file = "/etc/ferrofed/atna-roots.pem"
//! ```
//!
//! Each profile defines its records as a BALP `AuditEvent` (PIXm
//! §2:3.83.5.1, PDQm §2:3.78.5.1 and §2:3.119.5.1, mCSD §2:3.90.5.1 and
//! §2:3.91.5.1, PMIR §2:3.93.5.1 and §2:3.94.5.1), which BALP sends over the ATX: FHIR Feed Option of ITI-20
//! (BALP §1:52.1.1.1): `destination = "repository"` posts each record to
//! the Audit Record Repository's FHIR base, `log` writes it to the
//! `ferrofed::audit` log target without a patient identifier or a caller,
//! which outside development a gateway with a registry refuses, and `off`,
//! which only `profile = "development"` admits, records nothing. The
//! repository is reached over `https`; plain `http` is admitted under
//! development alone, through the protected-payload policy of
//! [`transport`]: every record names the patient.
//! A record is stored in the spool before it is delivered; outside
//! development the spool is a directory, which survives a restart. No
//! specification governs the shape of the table: our own design.

use ferrohealth_config::removed::Removed;
use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_config::units;
use ferrohealth_server::tls_client::TlsClient;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::profile::Profile;
use ihe_atna::atna::spool::Bounds;
use ihe_atna::balp::{NetworkAddress, Observer};
use serde::Deserialize;
use url::Url;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::error::access_log::AccessLogFault;
use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, ProtectedSite};

/// Where audit records go: the `destination` of `[audit]`, and the `audit`
/// of the IHE binding's `[xcpd]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditDestination {
    /// The Audit Record Repository the table's own repository section
    /// names.
    Repository,
    /// A structured event at the `ferrofed::audit` log target, without a
    /// patient identifier, for a deployment that routes its log to its audit
    /// repository.
    Log,
    /// No record: development only.
    Off,
}

impl AuditDestination {
    /// The value as the configuration and `OPTIONS {base}/` spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Log => "log",
            Self::Off => "off",
        }
    }
}

/// `[audit]`, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Audit {
    /// Where the records go: `repository`, `log`, or `off`, which only
    /// `profile = "development"` admits. Outside development it has no
    /// default once a registry, or a binding section that records here, is
    /// configured, and a registry needs `repository`.
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
    pub spool_max_size: units::ByteSize,
    /// The retired `spool_max_bytes`, which `spool_max_size` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub spool_max_bytes: Removed,
    /// The most records the spool holds.
    pub spool_max_events: usize,
    /// The longest storing one record in the spool may take; a record not
    /// stored by then, or by the end of its exchange's time if that comes
    /// first, is an audit failure.
    pub spool_write_timeout: units::Duration,
    /// The retired `spool_write_timeout_ms`, which `spool_write_timeout` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub spool_write_timeout_ms: Removed,
    /// How long one delivery may take.
    pub timeout: units::Duration,
    /// The retired `timeout_ms`, which `timeout` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub timeout_ms: Removed,
    /// The longest wait between two delivery attempts.
    pub retry_max: units::Duration,
    /// The retired `retry_max_ms`, which `retry_max` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub retry_max_ms: Removed,
    /// The TLS material the service is reached with: the table's `tls`
    /// sub-table.
    pub tls: TlsClientSection,
}

impl Default for FeedRepository {
    fn default() -> Self {
        Self {
            url: String::new(),
            hostname: String::new(),
            source_id: None,
            enterprise_site: None,
            spool_dir: None,
            spool_max_size: units::ByteSize::from_mib(64),
            spool_max_bytes: Removed::default(),
            spool_max_events: 100_000,
            spool_write_timeout: units::Duration::from_secs(2),
            spool_write_timeout_ms: Removed::default(),
            timeout: units::Duration::from_secs(5),
            timeout_ms: Removed::default(),
            retry_max: units::Duration::from_mins(1),
            retry_max_ms: Removed::default(),
            tls: TlsClientSection::default(),
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
    /// The TLS material it is reached with, every file read.
    pub tls: Arc<TlsClient>,
}

/// The key of the table.
const KEY: &str = "audit";

/// Resolves `[audit]` under the configuration `config`.
///
/// # Errors
///
/// [`Error::Missing`] for no `destination` outside development while a
/// registry or a binding section that records here is configured, for no `[audit.repository]`
/// under `repository`, and for no `url`, `hostname` or, outside
/// development, `spool_dir`; [`AccessLogFault::FeedAuditOff`] for `off` outside
/// development; [`AccessLogFault::AccessAuditLog`] for `log` outside development
/// while a registry is configured; [`AccessLogFault::FeedAuditRepositoryUnused`] for a repository under
/// another destination; [`Error::Url`] for a `url` that does not parse;
/// [`Error::Zero`] for a zero bound or timeout; and the errors of a secret or
/// a file that cannot be read.
pub(crate) fn resolve(config: &Config) -> Result<AuditSettings, Error> {
    let profile = config.profile;
    let table = &config.audit;
    #[cfg(feature = "binding-ihe")]
    let audited = config.registry.configured() || crate::config::ihe::audits(config);
    #[cfg(not(feature = "binding-ihe"))]
    let audited = config.registry.configured();
    // NOTE: PIXm §2:3.83.5.1, PDQm §2:3.78.5.1, mCSD §2:3.90.5.1, PMIR §2:3.93.5.1 and
    // Regulation (EU) 2025/327 Annex II 3.2 have every exchange and access recorded.
    let destination = match table.destination {
        Some(AuditDestination::Off) if profile != Profile::Development => {
            return Err(Error::AccessLog(AccessLogFault::FeedAuditOff {
                key: format!("{KEY}.destination"),
            }));
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
        (_, Some(_)) => return Err(Error::AccessLog(AccessLogFault::FeedAuditRepositoryUnused)),
        (_, None) => None,
    };
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2 has the access log name who accessed and
    // whose data, which `log` never names (§5.4, N33), so a registry needs `repository`.
    if destination == AuditDestination::Log
        && config.registry.configured()
        && profile != Profile::Development
    {
        return Err(Error::AccessLog(AccessLogFault::AccessAuditLog {
            key: format!("{KEY}.destination"),
        }));
    }
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
        ("spool_max_size", table.spool_max_size.is_zero()),
        ("spool_max_events", table.spool_max_events == 0),
        ("spool_write_timeout", table.spool_write_timeout.is_zero()),
        ("timeout", table.timeout.is_zero()),
        ("retry_max", table.retry_max.is_zero()),
    ] {
        if zero {
            return Err(Error::Zero { key: key(field) });
        }
    }
    let tls = crate::config::tls::read(&key("tls"), &table.tls)?;
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
            source_types: Vec::new(),
        },
        spool_dir: table.spool_dir.clone(),
        bounds: Bounds {
            max_messages: table.spool_max_events,
            max_bytes: table.spool_max_size.bytes(),
            write_timeout: table.spool_write_timeout.get(),
        },
        timeout: table.timeout.get(),
        retry_max: table.retry_max.get(),
        tls,
    })
}

/// Holds the repository `settings` send to to the transport policy, and
/// returns it when it is reached in cleartext under the development profile.
///
/// # Errors
///
/// The [`CleartextError`] of a repository the policy refuses.
pub fn sites(settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
    let Some(repository) = &settings.audit.repository else {
        return Ok(Vec::new());
    };
    Ok(transport::protected_payload(
        settings.profile,
        repository.url.as_str(),
        ProtectedSite {
            url_key: String::from("audit.repository.url"),
            payload: String::from(
                "the access records and the IHE transactions' audit records, which name the patient",
            ),
            requires: transport::Encryption::Https,
        },
    )?
    .into_iter()
    .collect())
}
