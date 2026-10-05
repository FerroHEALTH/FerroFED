// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//!
//! The gateway authenticates to the service as a data user: by mutual TLS, a
//! bearer token or basic credentials, or on GF-Authentication with the Nuts
//! grant, the same table and the same implementation a node's onward
//! credentials take ([`super::nuts`]). The grant's `DPoP`-bound token is sent
//! to the service alone, with a proof of each request (the IG's GFI-004 and
//! GFI-005; Nuts RFC021):
//!
//! ```toml
//! [nl_gf.nvi.credentials.nuts]
//! authorization_server = "https://nuts.example.org/oauth2/nvi"
//! scope = "nl-gf-localization"
//! did = "did:web:gateway.example.org"
//! kid = "did:web:gateway.example.org#key-1"
//! key_file = "/run/secrets/nuts-holder.pem"
//! dpop_key_file = "/run/secrets/nvi-dpop.pem"
//!
//! [[nl_gf.nvi.credentials.nuts.credential]]
//! input_descriptor = "organization_credential"
//! file = "/run/secrets/organization.jwt"
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::nuts::NutsAuthorizer;
use ferrofed_identity::fhir::Authentication;
use ferrofed_identity::nl::nvi::{NviConfig, NviLocalizer};
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::{Secret, SecretUrl};
use ferrofed_registry::snapshot::RegistrySnapshot;
use nl_generic_functions::identification::is_bsn_system;
use nl_generic_functions::nvi::authorizer::Authorizer;
use serde::Deserialize;

use super::nuts::{self, NutsOnward};
use crate::binding::seam::{LocalizerSeam, OnwardGrant};
use crate::config::error::Error;
use crate::config::secrets::{resolve_credentials, secret};
use crate::config::settings::Scheme;
use crate::config::transport::ProtectedSite;
use crate::config::{Config, Credentials, transport};
use crate::localization::LocalizationError;
use crate::service;

/// The `localization.mode` of the NVI localizer of the Dutch Generic
/// Functions (Annex B §B.1).
pub const NL_GF_NVI: &str = "nl-gf-nvi";

/// The NVI localizer, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nvi {
    /// The Localization Service's FHIR base URL, `https`; no rendering shows
    /// its userinfo.
    pub url: SecretUrl,
    /// How the gateway authenticates to the service, when the transport does
    /// not: a bearer token, basic credentials or the Nuts grant.
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

/// How the gateway authenticates to the Localization Service, resolved.
#[derive(Debug)]
#[non_exhaustive]
pub enum NviCredentials {
    /// A bearer token or basic credentials, in a header of every request.
    Header(Scheme),
    /// The Nuts grant of GF-Authentication: a `DPoP`-bound token and a proof
    /// of each request (the IG's GFI-004 and GFI-005).
    Nuts(NutsOnward),
}

/// The NVI localizer, with every secret and file read.
pub struct NviSettings {
    /// The service's FHIR base URL, already known to parse.
    pub url: SecretUrl,
    /// How the gateway authenticates to it.
    pub credentials: Option<NviCredentials>,
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
            .field("credentials", &self.credentials)
            .field("custodians", &self.custodians)
            .field("namespaces", &self.namespaces)
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .finish()
    }
}

/// The key of the NVI table, which names its URL and its credentials.
pub const NVI_KEY: &str = "nl_gf.nvi";

/// The key of the NVI's credentials section.
const CREDENTIALS_KEY: &str = "nl_gf.nvi.credentials";

impl NviSettings {
    /// Returns the site the Nuts grant sends the gateway's credentials and
    /// presentation to, its authorization server, when the service is
    /// reached with one.
    #[must_use]
    pub fn grant_site(&self) -> Option<(String, ProtectedSite)> {
        match &self.credentials {
            Some(NviCredentials::Nuts(grant)) => Some(grant.site(CREDENTIALS_KEY)),
            _ => None,
        }
    }
}

/// Resolves the credentials section `table` of the NVI: a bearer token,
/// basic credentials or the Nuts grant, one of them.
fn credentials(table: &Credentials) -> Result<NviCredentials, Error> {
    // NOTE: the IG's Localization page (Authentication and Authorization) and GF-Authentication
    // define the Nuts profile for the data user and no other grant, so oauth2 and fapi2 are refused.
    let other_grant = [
        ("oauth2", table.oauth2.is_some()),
        ("fapi2", table.fapi2.is_some()),
    ];
    if let Some((grant, _)) = other_grant.into_iter().find(|(_, set)| *set) {
        return Err(Error::NviGrant {
            key: format!("{CREDENTIALS_KEY}.{grant}"),
        });
    }
    if let Some(grant) = &table.nuts {
        let header = table.bearer_token.is_some()
            || table.bearer_token_file.is_some()
            || table.user.is_some()
            || table.password.is_some()
            || table.password_file.is_some();
        if header {
            return Err(Error::Scheme {
                section: CREDENTIALS_KEY.to_owned(),
            });
        }
        return nuts::resolve(&format!("{CREDENTIALS_KEY}.{}", nuts::KEY), grant)
            .map(NviCredentials::Nuts);
    }
    let scheme = resolve_credentials(CREDENTIALS_KEY, table)?;
    if scheme.is_grant() {
        return Err(Error::GrantNotHere {
            section: CREDENTIALS_KEY.to_owned(),
        });
    }
    Ok(NviCredentials::Header(scheme))
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
    let credentials = nvi.credentials.as_ref().map(credentials).transpose()?;
    // NOTE: Annex B §B.7: the pseudonym is personal data like the BSN, so the
    // service URL is a protected-payload site; one admitted under development is reported by check.
    transport::protected_payload(
        config.profile,
        nvi.url.expose(),
        transport::identity_site(NVI_KEY, credentials.is_some().then_some(CREDENTIALS_KEY)),
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

/// The NVI localizer `nvi` describes over the members of `snapshot`
/// (Annex B §B.1), a Nuts grant's token requests bounded by `timeout`.
pub(super) fn localizer(
    nvi: &NviSettings,
    snapshot: &RegistrySnapshot,
    timeout: Duration,
) -> Result<LocalizerSeam, LocalizationError> {
    let mut custodians = BTreeMap::new();
    for (ura, member) in &nvi.custodians {
        let member =
            NodeId::new(member.as_str()).map_err(|source| LocalizationError::NviMember {
                ura: ura.clone(),
                source,
            })?;
        custodians.insert(ura.clone(), member);
    }
    let namespaces = nvi
        .namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(LocalizationError::NviNamespace)?;
    let (auth, authorizer) = match &nvi.credentials {
        None => (Authentication::None, None),
        Some(NviCredentials::Header(scheme)) => (
            service::authentication(CREDENTIALS_KEY, Some(scheme))
                .map_err(LocalizationError::Grant)?,
            None,
        ),
        Some(NviCredentials::Nuts(grant)) => {
            let client = nuts::http_client().map_err(LocalizationError::NviNutsClient)?;
            let authorizer: Arc<dyn Authorizer> = Arc::new(NutsAuthorizer::new(
                NVI_KEY,
                grant.grant().clone(),
                client,
                timeout,
                Arc::new(SystemClock),
            ));
            (Authentication::None, Some(authorizer))
        }
    };
    let tls = service::tls(
        NVI_KEY,
        nvi.client_identity.as_ref(),
        nvi.trust_roots.as_deref(),
    )
    .map_err(LocalizationError::Tls)?;
    let config = NviConfig {
        base: nvi.url.clone(),
        auth,
        authorizer,
        custodians,
        namespaces,
        tls,
    };
    let localizer = NviLocalizer::from_config(config, snapshot).map_err(LocalizationError::Nvi)?;
    Ok(LocalizerSeam {
        localizer: Arc::new(localizer),
        mode: NL_GF_NVI,
        audit: None,
        indicators: Vec::new(),
    })
}
