// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The NVI localizer, `[nl_gf.nvi]`.
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
//! namespaces = ["pseudo-bsn"]
//!
//! [nl_gf.nvi.endpoint]
//! url = "https://nvi.example.org/fhir"
//! credentials = { user = "ferrofed", password_file = "/run/secrets/nvi-password" }
//! tls = { client_identity_file = "/run/secrets/nvi-client.pem", trust_roots_file = "/etc/ferrofed/nvi-roots.pem" }
//!
//! [nl_gf.nvi.custodians]
//! "ura-test-0001" = "node-a"
//! ```
//!
//! The service is sent the pseudonymised BSN, which is personal data like
//! the BSN it stands for (Annex B §B.7), so its URL is held to the
//! protected-payload policy of [`transport`]. No specification governs the
//! shape of the table: our own design.
//!
//! `[nl_gf.nvi.endpoint]` is `ferrohealth-smart`'s `EndpointSection`. The
//! gateway authenticates to the service as a data user: by mutual TLS,
//! basic credentials, or on GF-Authentication with the Nuts grant, the same
//! table and the same implementation a node's onward credentials take
//! ([`super::nuts`]), which the outbound client carries as the product's
//! authorization. The grant's `DPoP`-bound token is sent to the service
//! alone, with a proof of each request (the IG's GFI-004 and GFI-005; Nuts
//! RFC021):
//!
//! ```toml
//! [nl_gf.nvi.nuts]
//! authorization_server = "https://nuts.example.org/oauth2/nvi"
//! scope = "nl-gf-localization"
//! did = "did:web:gateway.example.org"
//! kid = "did:web:gateway.example.org#key-1"
//! key_file = "/run/secrets/nuts-holder.pem"
//! dpop_key_file = "/run/secrets/nvi-dpop.pem"
//!
//! [[nl_gf.nvi.nuts.credential]]
//! input_descriptor = "organization_credential"
//! file = "/run/secrets/organization.jwt"
//! ```

use ferrohealth_config::units;
use ferrohealth_smart::config::credentials::CredentialsSection;
use ferrohealth_smart::config::endpoint::EndpointSection;
use std::collections::BTreeMap;
use std::fmt;

use nl_generic_functions::identification::is_bsn_system;
use serde::Deserialize;
use url::Url;

use super::nuts::{self, NutsOnward};
use crate::config::error::Error;
use crate::config::error::binding::BindingFault;
use crate::config::error::credentials::CredentialsFault;
use crate::config::secrets::resolve_credentials;
use crate::config::settings::OnwardGrant;
use crate::config::settings::Scheme;
use crate::config::transport::ProtectedSite;
use crate::config::{Config, transport};

/// The NVI localizer, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nvi {
    /// The Localization Service's FHIR base, `https`, and how it is reached:
    /// its TLS, the timeout and retries of each call, and basic credentials
    /// when neither the transport nor the Nuts grant authenticates the
    /// gateway.
    pub endpoint: EndpointSection,
    /// The Nuts grant the gateway authenticates to the service with, in place
    /// of basic credentials (`[nl_gf.nvi.nuts]`).
    pub nuts: Option<nuts::Nuts>,
    /// Each care provider, by its URA, mapped to the registry member that
    /// holds its data; optional when the directory publishes each member
    /// organisation's URA, and then equal to the map it gives.
    pub custodians: BTreeMap<String, String>,
    /// The client namespaces that stand for the pseudonymised BSN, beside
    /// `http://fhir.nl/fhir/NamingSystem/pseudo-bsn` itself; a BSN system is
    /// refused.
    pub namespaces: Vec<String>,
}

impl Default for Nvi {
    fn default() -> Self {
        Self {
            endpoint: crate::config::endpoint::tried_once(units::Duration::from_secs(10)),
            nuts: None,
            custodians: BTreeMap::new(),
            namespaces: Vec::new(),
        }
    }
}

/// How the gateway authenticates to the Localization Service, resolved.
#[derive(Debug)]
#[non_exhaustive]
pub enum NviCredentials {
    /// Basic credentials, in a header of every request.
    Header(Scheme),
    /// The Nuts grant of GF-Authentication: a `DPoP`-bound token and a proof
    /// of each request (the IG's GFI-004 and GFI-005).
    Nuts(NutsOnward),
}

/// The NVI localizer, with every secret and file read.
pub struct NviSettings {
    /// The service's FHIR base URL, read from `endpoint`.
    pub url: Url,
    /// How the gateway authenticates to it.
    pub credentials: Option<NviCredentials>,
    /// How the service is reached, as `ferrohealth-smart`'s outbound client
    /// reads it.
    pub endpoint: Box<EndpointSection>,
    /// The custodian map, as written.
    pub custodians: BTreeMap<String, String>,
    /// The namespaces that stand for the pseudonymised BSN, as written.
    pub namespaces: Vec<String>,
}

impl fmt::Debug for NviSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NviSettings")
            .field("url", &self.url)
            .field("credentials", &self.credentials)
            .field("endpoint", &self.endpoint)
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .finish()
    }
}

/// The key of the NVI table.
pub const NVI_KEY: &str = "nl_gf.nvi";

/// The key of the NVI's endpoint table, which names its URL and its
/// credentials.
pub const NVI_ENDPOINT: &str = "nl_gf.nvi.endpoint";

/// The key of the NVI's credentials section.
pub(crate) const CREDENTIALS_KEY: &str = "nl_gf.nvi.endpoint.credentials";

impl NviSettings {
    /// Returns the site the Nuts grant sends the gateway's credentials and
    /// presentation to, its authorization server, when the service is
    /// reached with one.
    #[must_use]
    pub fn grant_site(&self) -> Option<(String, ProtectedSite)> {
        match &self.credentials {
            Some(NviCredentials::Nuts(grant)) => Some(grant.site(NVI_KEY)),
            _ => None,
        }
    }
}

/// Resolves how the gateway authenticates to the NVI `nvi` names: basic
/// credentials or the Nuts grant, one of them, or none.
fn credentials(nvi: &Nvi) -> Result<Option<NviCredentials>, Error> {
    // NOTE: the IG's Localization page (Authentication and Authorization) and GF-Authentication
    // define the Nuts profile for the data user and no other grant, so every other grant is refused.
    if nvi.endpoint.credentials.oauth.is_some() {
        return Err(Error::Binding(BindingFault::NviGrant {
            key: format!("{CREDENTIALS_KEY}.oauth"),
        }));
    }
    let table = &nvi.endpoint.credentials;
    let basic = (*table != CredentialsSection::default()).then_some(table);
    match (&nvi.nuts, basic) {
        (Some(_), Some(_)) => Err(Error::Credentials(CredentialsFault::Scheme {
            section: NVI_KEY.to_owned(),
        })),
        (Some(grant), None) => nuts::resolve(&format!("{NVI_KEY}.{}", nuts::KEY), grant)
            .map(|grant| Some(NviCredentials::Nuts(grant))),
        (None, Some(table)) => {
            let scheme = resolve_credentials(CREDENTIALS_KEY, table)?;
            if scheme.is_grant() {
                return Err(Error::Credentials(CredentialsFault::GrantNotHere {
                    section: CREDENTIALS_KEY.to_owned(),
                }));
            }
            Ok(Some(NviCredentials::Header(scheme)))
        }
        (None, None) => Ok(None),
    }
}

/// Resolves `[nl_gf.nvi]`.
pub(super) fn resolve(config: &Config, nvi: &Nvi) -> Result<NviSettings, Error> {
    // NOTE: no specification governs this: our own design; the custodian map
    // names registry members, so it means nothing without a registry.
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    let url = crate::config::endpoint::checked(NVI_ENDPOINT, &nvi.endpoint)?;
    // NOTE: Annex B §B.1, N33: the NVI is keyed on the pseudonymised BSN, so a BSN
    // system listed as its alias would send a BSN there under the pseudonym's label.
    if let Some(bsn) = nvi
        .namespaces
        .iter()
        .find(|namespace| is_bsn_system(namespace))
    {
        return Err(Error::Binding(BindingFault::BsnAsPseudonym {
            namespace: bsn.clone(),
        }));
    }
    let credentials = credentials(nvi)?;
    // NOTE: Annex B §B.7: the pseudonym is personal data like the BSN, so the
    // service URL is a protected-payload site; one admitted under development is reported by check.
    transport::protected_payload(
        config.profile,
        url.as_str(),
        transport::identity_site(
            NVI_ENDPOINT,
            credentials.is_some().then_some(CREDENTIALS_KEY),
        ),
    )?;
    crate::config::tls::read("nl_gf.nvi.endpoint.tls", &nvi.endpoint.tls)?;
    Ok(NviSettings {
        url,
        credentials,
        endpoint: Box::new(nvi.endpoint.clone()),
        custodians: nvi.custodians.clone(),
        namespaces: nvi.namespaces.clone(),
    })
}
