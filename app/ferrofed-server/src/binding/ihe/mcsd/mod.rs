// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mCSD care services directory, `[registry.mcsd]` (§15.1, Annex A.5):
//! the table, its resolution, and the registry the gateway keeps in step
//! with the directory ([`registry`]).

pub mod registry;

use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::binding::ihe::audit::config::AuditSettings;
use crate::config::Credentials;
use crate::config::error::Error;
use crate::config::resolve::{positive, positive_ms};
use crate::config::secrets::resolve_credentials;
use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;
use crate::config::transport;

/// The key of the directory's URL.
const URL_KEY: &str = "registry.mcsd.url";

/// The mCSD care services directory the registry is read from.
///
/// Its `Organization`s and `Endpoint`s are read with ITI-90 and checked as
/// the registry document in FHIR form is; every `refresh_interval_s` the
/// changes since the last read are asked for with ITI-91 and checked again
/// before they replace the running registry. Each read or refresh ends at
/// its deadline and its caps on pages, bytes and entries, so a faulty
/// directory can neither hold it nor fill the gateway's memory (no
/// specification governs these limits: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McsdDirectory {
    /// The directory's FHIR base URL, `http` or `https`, with no user name
    /// or password.
    pub url: SecretUrl,
    /// How the gateway authenticates to the directory, when the transport
    /// does not: a bearer token or basic credentials.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS with the directory, inline or through
    /// `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the directory's certificate chains to,
    /// beside the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// How often the changes are asked for, in seconds.
    pub refresh_interval_s: u64,
    /// How long one whole read or refresh may take, every page of both
    /// resource types included, in milliseconds.
    pub deadline_ms: u64,
    /// The most pages one read or refresh may read.
    pub max_pages: usize,
    /// The most bytes of answer bodies one read or refresh may read.
    pub max_bytes: usize,
    /// The most Bundle entries one read or refresh may read.
    pub max_entries: usize,
}

impl Default for McsdDirectory {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
            refresh_interval_s: 300,
            deadline_ms: 30_000,
            max_pages: 200,
            max_bytes: 64 << 20,
            max_entries: 50_000,
        }
    }
}

/// The mCSD care services directory the registry is read from, resolved.
#[derive(Debug)]
pub struct DirectorySettings {
    /// The directory's FHIR base URL, already known to parse as an `http` or
    /// `https` URL with no user name or password.
    pub url: SecretUrl,
    /// How the gateway authenticates to it: a bearer token or basic
    /// credentials.
    pub credentials: Option<Scheme>,
    /// The TLS material the directory is reached with.
    pub tls: TlsSettings,
    /// How often the changes are asked for.
    pub refresh_interval: Duration,
    /// How long one whole read or refresh may take.
    pub deadline: Duration,
    /// The most pages one read or refresh may read.
    pub max_pages: usize,
    /// The most bytes of answer bodies one read or refresh may read.
    pub max_bytes: usize,
    /// The most Bundle entries one read or refresh may read.
    pub max_entries: usize,
    /// Where the audit records of its ITI-90 searches and ITI-91 histories
    /// go: `[audit]`, as the whole configuration resolves it.
    pub audit: AuditSettings,
}

impl DirectorySettings {
    /// Whether `other` names the same directory, credentials, TLS material,
    /// interval, deadline and caps.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        let credentials = match (&self.credentials, &other.credentials) {
            (None, None) => true,
            (Some(Scheme::Bearer(was)), Some(Scheme::Bearer(now))) => was == now,
            (
                Some(Scheme::Basic { user, password }),
                Some(Scheme::Basic {
                    user: now_user,
                    password: now_password,
                }),
            ) => user == now_user && password == now_password,
            _ => false,
        };
        credentials
            && self.url.expose() == other.url.expose()
            && self.tls == other.tls
            && self.refresh_interval == other.refresh_interval
            && self.deadline == other.deadline
            && self.max_pages == other.max_pages
            && self.max_bytes == other.max_bytes
            && self.max_entries == other.max_entries
    }
}

/// The site of the directory: its `url`, sent the credentials of its own
/// section.
#[must_use]
pub fn site() -> transport::ProtectedSite {
    transport::ProtectedSite {
        url_key: String::from(URL_KEY),
        payload: String::from("registry.mcsd.credentials"),
        requires: transport::Encryption::Https,
    }
}

/// Resolves `[registry.mcsd]`: an `http` or `https` base URL with no user name
/// or password, a bearer token or basic credentials, and a positive interval,
/// deadline and caps, audited as `audit` says.
pub(super) fn resolve(
    directory: &McsdDirectory,
    profile: Profile,
    audit: AuditSettings,
) -> Result<DirectorySettings, Error> {
    if directory.url.is_empty() {
        return Err(Error::Missing {
            key: URL_KEY.to_owned(),
        });
    }
    let url = url::Url::parse(directory.url.expose()).map_err(|source| Error::Url {
        key: URL_KEY.to_owned(),
        source,
    })?;
    // NOTE: no specification governs this: our own design; as on a PIX Manager
    // URL, a credential goes in its own section and never in the URL.
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::HttpUrl {
            key: URL_KEY.to_owned(),
        });
    }
    let section = String::from("registry.mcsd.credentials");
    let credentials = directory
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if credentials.as_ref().is_some_and(Scheme::is_grant) {
        return Err(Error::GrantNotHere { section });
    }
    // NOTE: no specification governs this: our own design; the credential is
    // held to https before anything is sent, and the binding's sites report it.
    if credentials.is_some() {
        transport::protected_payload(profile, directory.url.expose(), site())?;
    }
    let refresh_interval = Duration::from_secs(directory.refresh_interval_s);
    if refresh_interval.is_zero() {
        return Err(Error::Zero {
            key: String::from("registry.mcsd.refresh_interval_s"),
        });
    }
    let tls = crate::config::tls::resolve(
        "registry.mcsd",
        directory.client_identity.as_ref(),
        directory.client_identity_file.as_deref(),
        directory.trust_roots_file.as_ref(),
    )?;
    Ok(DirectorySettings {
        url: directory.url.clone(),
        credentials,
        tls,
        refresh_interval,
        deadline: positive_ms("registry.mcsd.deadline_ms", directory.deadline_ms)?,
        max_pages: positive("registry.mcsd.max_pages", directory.max_pages)?,
        max_bytes: positive("registry.mcsd.max_bytes", directory.max_bytes)?,
        max_entries: positive("registry.mcsd.max_entries", directory.max_entries)?,
        audit,
    })
}
