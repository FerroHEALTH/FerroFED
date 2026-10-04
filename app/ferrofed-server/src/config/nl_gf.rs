// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch Generic Functions, `[nl_gf]`: the regional binding of Annex B,
//! one table per function a deployment uses.
//!
//! `[nl_gf.nvi]` is the localizer over GF-Localization, the NVI
//! Localization Service (Annex B §B.1, N4, §14.1): its FHIR base, how the
//! gateway authenticates to it, and the registry member that holds each care
//! provider's data, by URA. The `custodians` table is optional when the
//! registry comes from a directory whose organisations publish their URAs
//! (Annex B §B.2); given beside them, it must agree.
//!
//! ```toml
//! [nl_gf.nvi]
//! url = "https://nvi.example.org/fhir"
//! credentials = { bearer_token_file = "/run/secrets/nvi-token" }
//! client_identity_file = "/run/secrets/nvi-client.pem"
//! trust_roots_file = "/etc/ferrofed/nvi-roots.pem"
//! namespaces = ["pseudo-bsn"]
//!
//! [nl_gf.nvi.custodians]
//! "ura-test-0001" = "node-a"
//! ```
//!
//! The service is sent the pseudonymised BSN, which is personal data like
//! the BSN it stands for (Annex B §B.7), so its URL is held to the
//! protected-payload policy of [`transport`]. No specification governs the
//! shape of the table: our own design.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use ferrofed_identity::nvi::is_bsn_system;
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::secrets::{resolve_credentials, secret};
use crate::config::settings::Scheme;
use crate::config::{Config, Credentials, transport};

/// The Dutch Generic Functions, as the configuration writes them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct NlGf {
    /// The NVI localizer (`[nl_gf.nvi]`).
    pub nvi: Option<Nvi>,
    /// The Mitz consent pre-filter (`[nl_gf.mitz]`).
    pub mitz: Option<crate::config::mitz::Mitz>,
}

/// The NVI localizer, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nvi {
    /// The Localization Service's FHIR base URL, `https`; no rendering shows
    /// its userinfo.
    pub url: SecretUrl,
    /// How the gateway authenticates to the service, when the transport does
    /// not: a bearer token or basic credentials.
    pub credentials: Option<Credentials>,
    /// Each care provider, by its URA, mapped to the registry member that
    /// holds its data; optional when the directory publishes each member
    /// organisation's URA, and then equal to the map it gives.
    pub custodians: BTreeMap<String, String>,
    /// The client namespaces that stand for the pseudonymised BSN, beside
    /// `http://fhir.nl/fhir/NamingSystem/pseudo-bsn` itself; a BSN system is
    /// refused.
    pub namespaces: Vec<String>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS, inline or through `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the service's certificate chains to, beside
    /// the platform's.
    pub trust_roots_file: Option<PathBuf>,
}

/// The Dutch Generic Functions, resolved.
#[derive(Debug)]
pub struct NlGfSettings {
    /// The NVI localizer, when `[nl_gf.nvi]` is set.
    pub nvi: Option<NviSettings>,
    /// The Mitz consent pre-filter, when `[nl_gf.mitz]` is set.
    pub mitz: Option<crate::config::mitz::MitzSettings>,
}

/// The NVI localizer, with every secret and file read.
pub struct NviSettings {
    /// The service's FHIR base URL, already known to parse.
    pub url: SecretUrl,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
    /// The custodian map, as written.
    pub custodians: BTreeMap<String, String>,
    /// The namespaces that stand for the pseudonymised BSN, as written.
    pub namespaces: Vec<String>,
    /// The client certificate chain and key.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots.
    pub trust_roots: Option<String>,
}

impl fmt::Debug for NviSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NviSettings")
            .field("url", &self.url)
            .field("credentials", &self.credentials.is_some())
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .finish()
    }
}

/// The key of the NVI table, which names its URL and its credentials.
pub const NVI_KEY: &str = "nl_gf.nvi";

/// Resolves `[nl_gf]`: an NVI URL that parses, carries no userinfo and is
/// `https` outside development, a bearer token or basic credentials, and
/// every secret and file read.
///
/// # Errors
/// [`Error::BsnAsPseudonym`] for a BSN system listed in `namespaces`,
/// [`Error::Missing`] for no registry or no `url`, [`Error::Url`] for a URL
/// that does not parse, [`Error::UrlCredentials`] for one that carries a
/// user name or a password, [`Error::Cleartext`] for one that is not
/// `https` outside the development profile, [`Error::GrantNotHere`] for an
/// OAuth 2.0 grant, and the errors of a secret or a file that cannot be read.
pub(super) fn resolve(config: &Config) -> Result<Option<NlGfSettings>, Error> {
    let Some(nl_gf) = &config.nl_gf else {
        return Ok(None);
    };
    let nvi = nl_gf
        .nvi
        .as_ref()
        .map(|nvi| resolve_nvi(config, nvi))
        .transpose()?;
    let mitz = nl_gf
        .mitz
        .as_ref()
        .map(|mitz| crate::config::mitz::resolve(config, mitz))
        .transpose()?;
    Ok(Some(NlGfSettings { nvi, mitz }))
}

/// Resolves `[nl_gf.nvi]`.
fn resolve_nvi(config: &Config, nvi: &Nvi) -> Result<NviSettings, Error> {
    // NOTE: no specification governs this: our own design; the custodian map
    // names registry members, so it means nothing without a registry.
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    let url_key = format!("{NVI_KEY}.url");
    if nvi.url.expose().is_empty() {
        return Err(Error::Missing { key: url_key });
    }
    let url = url::Url::parse(nvi.url.expose()).map_err(|source| Error::Url {
        key: url_key.clone(),
        source,
    })?;
    // NOTE: no specification governs this: our own design; as the registry
    // refuses it on an endpoint URL, a credential goes in its own section.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::UrlCredentials {
            key: url_key,
            section: format!("{NVI_KEY}.credentials"),
        });
    }
    // NOTE: Annex B §B.1, N33: the NVI is keyed on the pseudonymised BSN, so a BSN
    // system listed as its alias would send a BSN there under the pseudonym's label.
    if let Some(bsn) = nvi
        .namespaces
        .iter()
        .find(|namespace| is_bsn_system(namespace))
    {
        return Err(Error::BsnAsPseudonym {
            namespace: bsn.clone(),
        });
    }
    let section = format!("{NVI_KEY}.credentials");
    let credentials = nvi
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if matches!(
        credentials,
        Some(Scheme::OAuth2(_) | Scheme::Nuts(_) | Scheme::Fapi2(_))
    ) {
        return Err(Error::GrantNotHere { section });
    }
    // NOTE: Annex B §B.7: the pseudonym is personal data like the BSN, so the
    // service URL is a protected-payload site; one admitted under development is reported by check.
    transport::protected_payload(
        config.profile,
        nvi.url.expose(),
        transport::identity_site(NVI_KEY, credentials.is_some().then_some(section.as_str())),
    )?;
    let client_identity = secret(
        "nl_gf.nvi.client_identity",
        nvi.client_identity.as_ref(),
        nvi.client_identity_file.as_deref(),
    )?;
    let trust_roots = nvi
        .trust_roots_file
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: String::from("nl_gf.nvi.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(NviSettings {
        url: nvi.url.clone(),
        credentials,
        custodians: nvi.custodians.clone(),
        namespaces: nvi.namespaces.clone(),
        client_identity,
        trust_roots,
    })
}
