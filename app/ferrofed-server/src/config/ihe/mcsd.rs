// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The mCSD care services directory, `[registry.mcsd]` (§15.1, Annex A.5).
//!
//! The table and its resolution, from which the IHE binding keeps the
//! registry in step with the directory. `[registry.mcsd.endpoint]` is
//! `ferrohealth-smart`'s `EndpointSection`: the directory's FHIR base, its
//! TLS, the timeout and retries of each call, and its credentials.

use ferrohealth_config::removed::Removed;
use ferrohealth_config::units;
use ferrohealth_server::tls_client::TlsClient;
use ferrohealth_smart::config::endpoint::EndpointSection;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use url::Url;

use crate::config::audit::AuditSettings;
use crate::config::error::Error;
use crate::config::resolve::{positive, positive_ms};
use crate::config::service_grant::ServiceContext;
use crate::config::settings::Scheme;
use crate::config::transport;

/// The key of the directory's endpoint table.
pub const ENDPOINT_KEY: &str = "registry.mcsd.endpoint";

/// The key of the directory's URL.
const URL_KEY: &str = "registry.mcsd.endpoint.url";

/// The mCSD care services directory the registry is read from.
///
/// Its `Organization`s and `Endpoint`s are read with ITI-90 and checked as
/// the registry document in FHIR form is; every `refresh_interval` the
/// changes since the last read are asked for with ITI-91 and checked again
/// before they replace the running registry. Each read or refresh ends at
/// its deadline and its caps on pages, bytes and entries, so a faulty
/// directory can neither hold it nor fill the gateway's memory (no
/// specification governs these limits: our own design).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct McsdDirectory {
    /// The directory's FHIR base, `http` or `https` with no user name or
    /// password, and how it is reached: its TLS, the timeout and retries of
    /// each call, and basic credentials or the client credentials grant of
    /// `oauth`.
    pub endpoint: EndpointSection,
    /// How often the changes are asked for.
    pub refresh_interval: units::Duration,
    /// The retired `refresh_interval_s`, which `refresh_interval` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub refresh_interval_s: Removed,
    /// How long one whole read or refresh may take, every page of both
    /// resource types included.
    pub deadline: units::Duration,
    /// The retired `deadline_ms`, which `deadline` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub deadline_ms: Removed,
    /// The most pages one read or refresh may read.
    pub max_pages: usize,
    /// The most bytes of answer bodies one read or refresh may read.
    pub max_size: units::ByteSize,
    /// The retired `max_bytes`, which `max_size` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub max_bytes: Removed,
    /// The most Bundle entries one read or refresh may read.
    pub max_entries: usize,
    /// The retired `url`, which `endpoint.url` replaced in 0.0.19: set, it
    /// refuses the configuration.
    #[serde(skip_serializing)]
    pub url: Removed,
    /// The retired `credentials`, which `endpoint.credentials` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub credentials: Removed,
    /// The retired `tls`, which `endpoint.tls` replaced in 0.0.19: set, it
    /// refuses the configuration.
    #[serde(skip_serializing)]
    pub tls: Removed,
}

impl Default for McsdDirectory {
    fn default() -> Self {
        Self {
            endpoint: crate::config::endpoint::tried_once(units::Duration::from_secs(10)),
            refresh_interval: units::Duration::from_mins(5),
            refresh_interval_s: Removed::default(),
            deadline: units::Duration::from_secs(30),
            deadline_ms: Removed::default(),
            max_pages: 200,
            max_size: units::ByteSize::from_mib(64),
            max_bytes: Removed::default(),
            max_entries: 50_000,
            url: Removed::default(),
            credentials: Removed::default(),
            tls: Removed::default(),
        }
    }
}

/// The mCSD care services directory the registry is read from, resolved.
#[derive(Debug)]
pub struct DirectorySettings {
    /// The directory's FHIR base URL, already known to parse as an `http` or
    /// `https` URL with no user name or password.
    pub url: Url,
    /// How the gateway authenticates to it: basic credentials or the client
    /// credentials grant of `oauth`.
    pub credentials: Option<Scheme>,
    /// How the directory is reached, as `ferrohealth-smart`'s outbound
    /// client reads it.
    pub endpoint: Box<EndpointSection>,
    /// The TLS material `endpoint.tls` names, read at load, which a reload
    /// compares.
    pub tls: Arc<TlsClient>,
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
    /// Whether `other` names the same endpoint table, credentials, interval,
    /// deadline and caps.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        let credentials = match (&self.credentials, &other.credentials) {
            (None, None) => true,
            (Some(was), Some(now)) => was.same_as(now),
            _ => false,
        };
        credentials
            && self.endpoint == other.endpoint
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
        payload: String::from("registry.mcsd.endpoint.credentials"),
        requires: transport::Encryption::Https,
    }
}

/// Resolves `[registry.mcsd]`: an endpoint table with an `http` or `https`
/// base URL with no user name or password, basic credentials or an `oauth`
/// grant, and a positive interval,
/// deadline and caps, audited as `audit` says.
pub(crate) fn resolve(
    directory: &McsdDirectory,
    context: ServiceContext,
    audit: AuditSettings,
) -> Result<DirectorySettings, Error> {
    let profile = context.profile;
    let endpoint = crate::config::endpoint::service(ENDPOINT_KEY, &directory.endpoint, context)?;
    // NOTE: no specification governs this: our own design; the directory is a FHIR
    // server, so its base is an http(s) URL (FHIR R4 RESTful API §3.1.0.1.2).
    if !matches!(endpoint.url.scheme(), "http" | "https") {
        return Err(Error::HttpUrl {
            key: URL_KEY.to_owned(),
        });
    }
    // NOTE: no specification governs this: our own design; the credential is
    // held to https before anything is sent, and the binding's sites report it.
    if endpoint.credentials.is_some() {
        transport::protected_payload(profile, endpoint.url.as_str(), site())?;
    }
    let refresh_interval = Duration::from_secs(directory.refresh_interval.as_secs());
    if refresh_interval.is_zero() {
        return Err(Error::Zero {
            key: String::from("registry.mcsd.refresh_interval"),
        });
    }
    Ok(DirectorySettings {
        url: endpoint.url,
        credentials: endpoint.credentials,
        tls: endpoint.tls,
        endpoint: Box::new(directory.endpoint.clone()),
        refresh_interval,
        deadline: positive_ms("registry.mcsd.deadline", directory.deadline.as_millis())?,
        max_pages: positive("registry.mcsd.max_pages", directory.max_pages)?,
        max_bytes: positive(
            "registry.mcsd.max_size",
            usize::try_from(directory.max_size.bytes()).unwrap_or(usize::MAX),
        )?,
        max_entries: positive("registry.mcsd.max_entries", directory.max_entries)?,
        audit,
    })
}
