// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ATNA Audit Record Repository the XCPD localizer's audit messages are
//! sent to, `[xcpd.audit_repository]`, under `[xcpd] audit = "repository"`
//! (ITI TF-2 §3.20, §3.55.5.1.1).
//!
//! ```toml
//! [xcpd]
//! audit = "repository"
//!
//! [xcpd.audit_repository]
//! url = "tls://arr.example.org:6514"
//! hostname = "gateway.example.org"
//! spool_dir = "/var/lib/ferrofed/audit-spool"
//! client_identity_file = "/run/secrets/atna-client.pem"
//! trust_roots_file = "/etc/ferrofed/atna-roots.pem"
//! ```
//!
//! The repository is reached over syslog with TLS (RFC 5425), `tls://`.
//! Plain TCP, `tcp://`, is admitted under `profile = "development"` alone,
//! through the protected-payload policy of [`transport`]: every message names
//! the patient. A message is stored in the spool before it is delivered;
//! outside development the spool is a directory, which survives a restart,
//! and under development without `spool_dir` it is held in memory. No
//! specification governs the shape of the table: our own design.

use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::Secret;
use ihe_iti::atna::message::AuditSource;
use ihe_iti::atna::repository::Timeouts;
use ihe_iti::atna::spool::Bounds;
use ihe_iti::atna::syslog::Sender;
use serde::Deserialize;
use url::Url;

use crate::config::error::Error;
use crate::config::secrets::secret;
use crate::config::transport;

/// The repository, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AuditRepository {
    /// `tls://host[:port]`, the port 6514 by default; `tcp://host:port`
    /// under development only.
    pub url: String,
    /// The syslog `HOSTNAME`, and the machine name or IP address the audit
    /// message names as the gateway's network access point.
    pub hostname: String,
    /// The syslog `APP-NAME`.
    pub app_name: String,
    /// The `AuditSourceID`, the hostname when unset.
    pub source_id: Option<String>,
    /// The `AuditEnterpriseSiteID`, when the deployment names one.
    pub enterprise_site: Option<String>,
    /// The spool directory; required outside development.
    pub spool_dir: Option<PathBuf>,
    /// The most bytes the spool holds.
    pub spool_max_bytes: u64,
    /// The most messages the spool holds.
    pub spool_max_events: usize,
    /// The longest storing one message in the spool may take; a message not
    /// stored by then, or by the end of its discovery's time if that comes
    /// first, is an audit failure.
    pub spool_write_timeout_ms: u64,
    /// How long the repository may take to accept a connection.
    pub connect_timeout_ms: u64,
    /// How long the TLS handshake, and each write and flush of a message,
    /// may take.
    pub send_timeout_ms: u64,
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

impl Default for AuditRepository {
    fn default() -> Self {
        Self {
            url: String::new(),
            hostname: String::new(),
            app_name: String::from("ferrofed"),
            source_id: None,
            enterprise_site: None,
            spool_dir: None,
            spool_max_bytes: 64 * 1024 * 1024,
            spool_max_events: 100_000,
            spool_write_timeout_ms: 2_000,
            connect_timeout_ms: 5_000,
            send_timeout_ms: 5_000,
            retry_max_ms: 60_000,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
        }
    }
}

/// The repository, with every value checked and every file read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRepositorySettings {
    /// The repository address.
    pub url: Url,
    /// Whether the address is plain TCP, which development alone admits.
    pub unencrypted: bool,
    /// The syslog header fields the gateway writes as.
    pub sender: Sender,
    /// The audit source every message names.
    pub source: AuditSource,
    /// The spool directory, or `None` for a spool in memory.
    pub spool_dir: Option<PathBuf>,
    /// The spool's bounds.
    pub bounds: Bounds,
    /// How long each step of sending a message may take.
    pub timeouts: Timeouts,
    /// The longest wait between two delivery attempts.
    pub retry_max: Duration,
    /// The client certificate chain and key.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots.
    pub trust_roots: Option<String>,
}

/// Resolves `[xcpd.audit_repository]` under `profile`.
///
/// # Errors
///
/// [`Error::Missing`] for no `url` or `hostname`, and for no `spool_dir`
/// outside development; [`Error::Url`] for a `url` that does not parse;
/// [`Error::Cleartext`] for a `url` that is not `tls://` outside development;
/// [`Error::SyslogHeader`] for a `hostname` or `app_name` syslog cannot
/// carry; [`Error::Zero`] for a zero bound or timeout; and the errors of a
/// secret or a file that cannot be read.
pub(super) fn resolve(
    profile: Profile,
    table: &AuditRepository,
) -> Result<AuditRepositorySettings, Error> {
    const KEY: &str = "xcpd.audit_repository";
    let missing = |key: &str| Error::Missing {
        key: format!("{KEY}.{key}"),
    };
    if table.url.is_empty() {
        return Err(missing("url"));
    }
    if table.hostname.is_empty() {
        return Err(missing("hostname"));
    }
    let url = Url::parse(&table.url).map_err(|source| Error::Url {
        key: format!("{KEY}.url"),
        source,
    })?;
    // NOTE: ITI TF-2 §3.20.4.1.2.1.1: the message names the patient and travels
    // over TLS; plain TCP is admitted, and reported, under development alone.
    transport::encrypted_syslog(
        profile,
        &url,
        transport::ProtectedSite {
            url_key: format!("{KEY}.url"),
            payload: String::from("the ITI-55 audit messages, which name the patient"),
            requires: transport::Encryption::SyslogTls,
        },
    )?;
    let sender = Sender::new(
        &table.hostname,
        &table.app_name,
        &std::process::id().to_string(),
    )
    .map_err(|source| Error::SyslogHeader {
        key: format!(
            "{KEY}.{}",
            if source.field == "APP-NAME" {
                "app_name"
            } else {
                "hostname"
            }
        ),
        source,
    })?;
    if table.spool_dir.is_none() && profile != Profile::Development {
        return Err(missing("spool_dir"));
    }
    for (key, zero) in [
        ("spool_max_bytes", table.spool_max_bytes == 0),
        ("spool_max_events", table.spool_max_events == 0),
        ("spool_write_timeout_ms", table.spool_write_timeout_ms == 0),
        ("connect_timeout_ms", table.connect_timeout_ms == 0),
        ("send_timeout_ms", table.send_timeout_ms == 0),
        ("retry_max_ms", table.retry_max_ms == 0),
    ] {
        if zero {
            return Err(Error::Zero {
                key: format!("{KEY}.{key}"),
            });
        }
    }
    let client_identity = secret(
        "xcpd.audit_repository.client_identity",
        table.client_identity.as_ref(),
        table.client_identity_file.as_deref(),
    )?;
    let trust_roots = table
        .trust_roots_file
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: format!("{KEY}.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(AuditRepositorySettings {
        unencrypted: url.scheme() != "tls",
        url,
        source: AuditSource {
            id: table
                .source_id
                .clone()
                .unwrap_or_else(|| table.hostname.clone()),
            enterprise_site: table.enterprise_site.clone(),
        },
        sender,
        spool_dir: table.spool_dir.clone(),
        bounds: Bounds {
            max_messages: table.spool_max_events,
            max_bytes: table.spool_max_bytes,
            write_timeout: Duration::from_millis(table.spool_write_timeout_ms),
        },
        timeouts: Timeouts {
            connect: Duration::from_millis(table.connect_timeout_ms),
            send: Duration::from_millis(table.send_timeout_ms),
        },
        retry_max: Duration::from_millis(table.retry_max_ms),
        client_identity,
        trust_roots,
    })
}
