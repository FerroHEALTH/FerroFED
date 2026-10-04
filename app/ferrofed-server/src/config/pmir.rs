// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! feed_token_file = "/run/secrets/pmir-feed-token"
//! identifier_system = "urn:oid:2.999.1"
//!
//! [pmir.credentials]
//! bearer_token_file = "/run/secrets/pmir-registry-token"
//! ```
//!
//! The Registry sends the feed with the bearer token `feed_token`, agreed
//! with its operator out of band: the subscription carries no credential for
//! the feed (§2:3.94.5), and ITI-93 leaves the client authentication to "an
//! appropriate agreement between client and server" (§2:3.93.5). Both the
//! Registry's `url` and the `callback_url` carry patient identifiers, so each
//! is held to the protected-payload policy of [`transport`]. No specification
//! governs the shape of the table: our own design.

use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::dev::Profile;
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;
use url::Url;

use crate::ITS_REST_PREFIX;
use crate::config::error::Error;
use crate::config::secrets::{resolve_credentials, secret};
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
    /// authorizes the subscription (§2:3.94.5): a bearer token or basic
    /// credentials.
    pub credentials: Option<Credentials>,
    /// The absolute URL the Registry sends the feed to: the gateway's public
    /// address followed by `{base}` and `path`, the `channel.endpoint` of the
    /// subscription.
    pub callback_url: String,
    /// The path under `{base}` the gateway serves the feed at.
    pub path: String,
    /// The bearer token the Registry sends the feed with, inline or through
    /// `feed_token_file`.
    pub feed_token: Option<Secret>,
    /// A file holding the feed token, read at boot.
    pub feed_token_file: Option<PathBuf>,
    /// The assigning authority the feed is limited to, so the Registry sends
    /// only the Patients with an identifier it issued
    /// (`Patient?identifier=<system>|`, §2:3.94.4.1.2.1.1). Unset, every
    /// Patient change is sent.
    pub identifier_system: Option<String>,
    /// How long one exchange with the Registry may take, in milliseconds.
    pub timeout_ms: u64,
    /// How often the gateway checks its subscription and subscribes again
    /// when the Registry no longer holds it, in seconds.
    pub check_interval_s: u64,
}

impl Default for Pmir {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            callback_url: String::new(),
            path: String::from("/pmir/feed"),
            feed_token: None,
            feed_token_file: None,
            identifier_system: None,
            timeout_ms: 5_000,
            check_interval_s: 60,
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
    /// Where the Registry sends the feed.
    pub callback_url: Url,
    /// The path under `{base}` the feed is served at.
    pub path: String,
    /// The bearer token the Registry sends the feed with.
    pub feed_token: Secret,
    /// The assigning authority the feed is limited to.
    pub identifier_system: Option<String>,
    /// How long one exchange with the Registry may take.
    pub timeout: Duration,
    /// How often the subscription is checked.
    pub check_interval: Duration,
}

impl PmirSettings {
    /// Whether `other` names the same Registry, credentials, callback, path,
    /// feed token, criteria and timings.
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
            && self.callback_url == other.callback_url
            && self.path == other.path
            && self.feed_token == other.feed_token
            && self.identifier_system == other.identifier_system
            && self.timeout == other.timeout
            && self.check_interval == other.check_interval
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
/// identifiers, and the feed token travel to it.
fn callback_site() -> ProtectedSite {
    ProtectedSite {
        url_key: String::from("pmir.callback_url"),
        payload: String::from("pmir.feed_token and the patient identifiers of the ITI-93 messages"),
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
/// timings.
///
/// # Errors
/// [`Error::Missing`] for no registry, no `url`, no `callback_url` or no feed
/// token; [`Error::Url`] or [`Error::HttpUrl`] for a URL that does not parse
/// or is no `http(s)` URL without credentials; [`Error::Cleartext`] for one
/// that is not `https` outside the development profile;
/// [`Error::Authorization`] for a feed token that is no RFC 6750 `b64token`;
/// [`Error::FeedPath`]; [`Error::GrantNotHere`] for OAuth 2.0 credentials;
/// [`Error::Zero`]; and the errors of a secret that cannot be read.
pub(super) fn resolve(config: &Config) -> Result<Option<PmirSettings>, Error> {
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
    if pmir.callback_url.is_empty() {
        return Err(Error::Missing {
            key: String::from("pmir.callback_url"),
        });
    }
    let callback_url = http_url("pmir.callback_url", &pmir.callback_url)?;
    if callback_url.fragment().is_some() {
        return Err(Error::HttpUrl {
            key: String::from("pmir.callback_url"),
        });
    }
    let path = feed_path(&pmir.path)?;
    let feed_token = secret(
        "pmir.feed_token",
        pmir.feed_token.as_ref(),
        pmir.feed_token_file.as_deref(),
    )?
    .ok_or_else(|| Error::Missing {
        key: String::from("pmir.feed_token"),
    })?;
    let token_key = if pmir.feed_token_file.is_some() {
        "pmir.feed_token_file"
    } else {
        "pmir.feed_token"
    };
    openehr_its::rest::client::Credentials::bearer(feed_token.to_secret_string())
        .header_value()
        .map_err(|source| Error::Authorization {
            key: token_key.to_owned(),
            source,
        })?;
    let section = String::from("pmir.credentials");
    let credentials = pmir
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if matches!(credentials, Some(Scheme::OAuth2(_) | Scheme::Nuts(_))) {
        return Err(Error::GrantNotHere { section });
    }
    if let Some(system) = &pmir.identifier_system {
        Url::parse(system).map_err(|source| Error::Url {
            key: String::from("pmir.identifier_system"),
            source,
        })?;
    }
    let settings = PmirSettings {
        url: pmir.url.clone(),
        credentials,
        callback_url,
        path,
        feed_token,
        identifier_system: pmir.identifier_system.clone(),
        timeout: positive("pmir.timeout_ms", Duration::from_millis(pmir.timeout_ms))?,
        check_interval: positive(
            "pmir.check_interval_s",
            Duration::from_secs(pmir.check_interval_s),
        )?,
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
        Err(Error::FeedPath {
            key: String::from("pmir.path"),
        })
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
