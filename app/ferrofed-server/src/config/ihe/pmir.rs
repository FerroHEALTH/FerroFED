// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PMIR identity feed, `[pmir]` (track 8 of §16.3, Annex A.4).
//!
//! It names the Patient Identity Registry the gateway subscribes to with
//! ITI-94, and the path the Registry sends each ITI-93 message to (PMIR
//! 1.6.0 §2:3.93, §2:3.94).
//!
//! ```toml
//! [pmir]
//! url = "https://pmir.example.org/fhir"
//! callback_url = "https://gateway.example.org/pmir/feed"
//! path = "/pmir/feed"
//! feed_clients = ["pmir-registry"]
//! identifier_system = "urn:oid:2.999.1"
//! on_drain = "keep"
//!
//! [pmir.credentials.oauth]
//! token_endpoint = "https://pmir.example.org/auth/token"
//! client_id = "ferrofed-pmir-subscriber"
//! client_secret_file = "/run/secrets/pmir-subscriber"
//! scope = "*"
//! ```
//!
//! `[pmir.tls]` gives the mutual TLS the
//! Registry asks for, as `[xcpd]` does ([`tls`](crate::config::tls)).
//!
//! The Registry sends the feed with a client-credentials token (RFC 6749
//! §4.4) of a client `feed_clients` names, issued by an `[[auth.issuer]]` the
//! gateway trusts and agreed with its operator out of band: the subscription carries no credential for the feed
//! (§2:3.94.5), and ITI-93 leaves the client authentication to "an
//! appropriate agreement between client and server" (§2:3.93.5). Both the
//! Registry's `url` and the `callback_url` carry patient identifiers, so each
//! is held to the protected-payload policy of [`transport`]. No specification
//! governs the shape of the table: our own design.

use ferrohealth_config::removed::Removed;
use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_config::units;
use ferrohealth_server::tls_client::TlsClient;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::profile::Profile;
use ferrohealth_secret::secret_url::SecretUrl;
use serde::Deserialize;
use url::Url;

use crate::ITS_REST_PREFIX;
use crate::config::error::Error;
use crate::config::error::binding::BindingFault;
use crate::config::public_url::PublicUrl;
use crate::config::service_grant::{ServiceContext, resolve_service};
use crate::config::settings::Scheme;
use crate::config::transport::{self, Encryption, ProtectedSite};
use crate::config::{Config, Credentials};

/// The PMIR identity feed, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pmir {
    /// The Patient Identity Registry's FHIR base URL, with no user name or
    /// password.
    pub url: SecretUrl,
    /// How the gateway authenticates to the Registry, by which the Registry
    /// authorizes the subscription (§2:3.94.5): basic credentials or the
    /// client credentials grant of `oauth`.
    pub credentials: Option<Credentials>,
    /// The TLS material the service is reached with: the table's `tls`
    /// sub-table.
    pub tls: TlsClientSection,
    /// The absolute URL the Registry sends the feed to: the gateway's public
    /// address followed by `{base}` and `path`, the `channel.endpoint` of the
    /// subscription. Empty, it is `path` under `gateway.public_url`, which
    /// must then be set.
    pub callback_url: String,
    /// The path under `{base}` the gateway serves the feed at.
    pub path: String,
    /// The clients the Registry may send the feed as, each with a
    /// client-credentials token of an `[[auth.issuer]]` (RFC 6749 §4.4).
    pub feed_clients: Vec<String>,
    /// The assigning authority the feed is limited to, so the Registry sends
    /// only the Patients with an identifier it issued
    /// (`Patient?identifier=<system>|`, §2:3.94.4.1.2.1.1). Unset, every
    /// Patient change is sent.
    pub identifier_system: Option<String>,
    /// How long one exchange with the Registry may take.
    pub timeout: units::Duration,
    /// The retired `timeout_ms`, which `timeout` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub timeout_ms: Removed,
    /// How often the gateway checks its subscription and subscribes again
    /// when the Registry no longer holds it.
    pub check_interval: units::Duration,
    /// The retired `check_interval_s`, which `check_interval` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub check_interval_s: Removed,
    /// What a drain does with the subscription: `keep` it at the Registry,
    /// the default, or `unsubscribe`.
    pub on_drain: OnDrain,
}

/// The key of the URL the Registry sends the feed to.
const CALLBACK_URL: &str = "pmir.callback_url";

/// What a draining gateway does with its subscription at the Registry.
///
/// Every gateway that shares a `callback_url` shares the one subscription
/// that names it, so a drain deletes it only when the operator says no other
/// gateway relies on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OnDrain {
    /// The subscription stays at the Registry, for the other replicas behind
    /// the same `callback_url` and for this gateway's next start, which
    /// adopts it.
    #[default]
    Keep,
    /// The subscription is deleted (§2:3.94.4.5): for the last gateway of a
    /// `callback_url`, or one whose `callback_url` is its own.
    Unsubscribe,
}

impl Default for Pmir {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            tls: TlsClientSection::default(),
            callback_url: String::new(),
            path: String::from("/pmir/feed"),
            feed_clients: Vec::new(),
            identifier_system: None,
            timeout: units::Duration::from_secs(5),
            timeout_ms: Removed::default(),
            check_interval: units::Duration::from_mins(1),
            check_interval_s: Removed::default(),
            on_drain: OnDrain::Keep,
        }
    }
}

/// The PMIR identity feed, with every secret read.
#[derive(Debug)]
pub struct PmirSettings {
    /// The Registry's FHIR base URL, already known to parse as an `http` or
    /// `https` URL with no user name or password.
    pub url: SecretUrl,
    /// How the gateway authenticates to the Registry.
    pub credentials: Option<Scheme>,
    /// The TLS material the Registry is reached with.
    pub tls: Arc<TlsClient>,
    /// Where the Registry sends the feed.
    pub callback_url: Url,
    /// The path under `{base}` the feed is served at.
    pub path: String,
    /// The clients the Registry may send the feed as.
    pub feed_clients: Vec<String>,
    /// The assigning authority the feed is limited to.
    pub identifier_system: Option<String>,
    /// How long one exchange with the Registry may take.
    pub timeout: Duration,
    /// How often the subscription is checked.
    pub check_interval: Duration,
    /// What a drain does with the subscription.
    pub on_drain: OnDrain,
}

impl PmirSettings {
    /// Whether `other` names the same Registry, credentials, TLS material,
    /// callback, path, feed users, criteria, timings and drain.
    #[must_use]
    pub fn same_as(&self, other: &Self) -> bool {
        let credentials = match (&self.credentials, &other.credentials) {
            (None, None) => true,
            (Some(was), Some(now)) => was.same_as(now),
            _ => false,
        };
        credentials
            && self.url.expose() == other.url.expose()
            && self.tls == other.tls
            && self.callback_url == other.callback_url
            && self.path == other.path
            && self.feed_clients == other.feed_clients
            && self.identifier_system == other.identifier_system
            && self.timeout == other.timeout
            && self.check_interval == other.check_interval
            && self.on_drain == other.on_drain
    }
}

/// The site of the Registry: its `url`, sent the subscription and the
/// gateway's credentials, and answering with the identities it holds.
fn registry_site() -> ProtectedSite {
    ProtectedSite {
        url_key: String::from("pmir.url"),
        payload: String::from("pmir.credentials and the subscription to patient identities"),
        requires: Encryption::Https,
    }
}

/// The site of the callback: the ITI-93 messages, which carry patient
/// identifiers, and the feed's Basic credentials travel to it.
fn callback_site() -> ProtectedSite {
    ProtectedSite {
        url_key: String::from("pmir.callback_url"),
        payload: String::from(
            "the feed's Basic credentials and the patient identifiers of the ITI-93 messages",
        ),
        requires: Encryption::Https,
    }
}

/// Holds the two PMIR sites of `pmir`, when it is set, to the protected-payload policy under
/// `profile`, and returns the ones that travel in cleartext under the
/// development profile.
///
/// # Errors
/// The [`transport::CleartextError`] of a site that is not `https` outside the
/// development profile.
pub fn sites(
    profile: Profile,
    pmir: Option<&PmirSettings>,
) -> Result<Vec<ProtectedSite>, transport::CleartextError> {
    let mut cleartext = Vec::new();
    let Some(pmir) = pmir else {
        return Ok(cleartext);
    };
    cleartext.extend(transport::protected_payload(
        profile,
        pmir.url.expose(),
        registry_site(),
    )?);
    cleartext.extend(transport::protected_payload(
        profile,
        pmir.callback_url.as_str(),
        callback_site(),
    )?);
    Ok(cleartext)
}

/// Resolves `[pmir]`: a registry to keep in step, a Registry and a callback
/// URL with no user name or password, each `https` outside development, a
/// feed token that fits a bearer header, a path of its own, and positive
/// timings. With `public`, the public base URL, set, an unset `callback_url`
/// is the feed route under it, and a set one must name that route.
///
/// # Errors
/// [`Error::Missing`] for no registry, no `url`, no `callback_url` or no feed
/// client; [`BindingFault::FeedIssuer`] when `issuers` is false, so no issuer
/// signs a feed client's token; [`Error::Url`] or [`Error::HttpUrl`] for a URL that does not parse
/// or is no `http(s)` URL without credentials;
/// [`ServerFault::PublicUrlDisagrees`] for a `callback_url` that names another
/// route than the feed's under `public`; [`Error::Cleartext`] for one
/// that is not `https` outside the development profile;
/// [`BindingFault::FeedPath`]; [`CredentialsFault::ServiceGrantNotHere`](crate::config::error::credentials::CredentialsFault::ServiceGrantNotHere)
/// for a Nuts grant; [`Error::Zero`]; and the errors of a secret that
/// cannot be read.
///
/// [`ServerFault::PublicUrlDisagrees`]: crate::config::error::server::ServerFault::PublicUrlDisagrees
pub(crate) fn resolve(
    config: &Config,
    context: ServiceContext,
    public: Option<&PublicUrl>,
    issuers: bool,
) -> Result<Option<PmirSettings>, Error> {
    let Some(pmir) = &config.pmir else {
        return Ok(None);
    };
    // NOTE: no specification governs this: our own design; the feed changes the
    // resolution bindings of a federation, so it means nothing without a registry.
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    if pmir.url.is_empty() {
        return Err(Error::Missing {
            key: String::from("pmir.url"),
        });
    }
    http_url("pmir.url", pmir.url.expose())?;
    let path = feed_path(&pmir.path)?;
    let callback_url = match (pmir.callback_url.is_empty(), public) {
        (false, _) => http_url(CALLBACK_URL, &pmir.callback_url)?,
        (true, Some(public)) => public.route(CALLBACK_URL, &path)?,
        (true, None) => {
            return Err(Error::Missing {
                key: String::from(CALLBACK_URL),
            });
        }
    };
    if callback_url.fragment().is_some() {
        return Err(Error::HttpUrl {
            key: String::from(CALLBACK_URL),
        });
    }
    if let Some(public) = public {
        public.agrees(CALLBACK_URL, &callback_url, &path)?;
    }
    if pmir.feed_clients.is_empty() {
        return Err(Error::Missing {
            key: String::from("pmir.feed_clients"),
        });
    }
    if !issuers {
        return Err(Error::Binding(BindingFault::FeedIssuer));
    }
    let section = String::from("pmir.credentials");
    let credentials = pmir
        .credentials
        .as_ref()
        .map(|credentials| resolve_service(&section, credentials, context))
        .transpose()?;
    if let Some(system) = &pmir.identifier_system {
        Url::parse(system).map_err(|source| Error::Url {
            key: String::from("pmir.identifier_system"),
            source,
        })?;
    }
    let tls = crate::config::tls::read("pmir.tls", &pmir.tls)?;
    let settings = PmirSettings {
        url: pmir.url.clone(),
        credentials,
        tls,
        callback_url,
        path,
        feed_clients: pmir.feed_clients.clone(),
        identifier_system: pmir.identifier_system.clone(),
        timeout: positive(
            "pmir.timeout",
            Duration::from_millis(pmir.timeout.as_millis()),
        )?,
        check_interval: positive(
            "pmir.check_interval",
            Duration::from_secs(pmir.check_interval.as_secs()),
        )?,
        on_drain: pmir.on_drain,
    };
    // NOTE: PMIR §2:3.93.5: the feed and the subscription carry patient identities,
    // so both URLs are held to https before anything is sent.
    sites(config.profile, Some(&settings))?;
    Ok(Some(settings))
}

/// `text` at `key` as an `http` or `https` URL with no user name or password.
fn http_url(key: &str, text: &str) -> Result<Url, Error> {
    let url = Url::parse(text).map_err(|source| Error::Url {
        key: key.to_owned(),
        source,
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(Error::HttpUrl {
            key: key.to_owned(),
        });
    }
    Ok(url)
}

/// `path` as the path of the feed: from `/`, with no query or fragment, and
/// off the ITS-REST surface, the health family and the well-known documents.
fn feed_path(path: &str) -> Result<String, Error> {
    let its_rest = ITS_REST_PREFIX.trim_end_matches('/');
    let reserved = [its_rest, "/health", "/.well-known"];
    let under = |prefix: &str| path == prefix || path.starts_with(&format!("{prefix}/"));
    let own = path.len() > 1
        && path.starts_with('/')
        && !path.ends_with('/')
        && !path.contains(['?', '#', ' ', '{', '}', '*', ':'])
        && !reserved.iter().any(|prefix| under(prefix));
    if own {
        Ok(path.to_owned())
    } else {
        Err(Error::Binding(BindingFault::FeedPath {
            key: String::from("pmir.path"),
        }))
    }
}

/// `duration` at `key`, refused when zero.
fn positive(key: &str, duration: Duration) -> Result<Duration, Error> {
    if duration.is_zero() {
        return Err(Error::Zero {
            key: key.to_owned(),
        });
    }
    Ok(duration)
}

#[cfg(test)]
mod tests {
    use super::feed_path;

    #[test]
    fn a_feed_path_is_its_own() {
        for good in ["/pmir/feed", "/identity-feed", "/v1x"] {
            assert_eq!(Some(good), feed_path(good).ok().as_deref(), "{good}");
        }
        for bad in [
            "",
            "/",
            "pmir",
            "/v1",
            "/v1/query",
            "/health",
            "/health/x",
            "/.well-known/x",
            "/pmir/",
            "/pmir?x",
            "/pmir#x",
            "/pmir/{id}",
            "/pmir/*rest",
            "/pmir/:id",
        ] {
            assert!(feed_path(bad).is_err(), "{bad}");
        }
    }
}
