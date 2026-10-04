// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step, `[pdqm]` (Annex A §A.2 and §A.7).
//!
//! The table names the Patient Demographics Supplier the gateway asks for the
//! master identity of a patient identifier issued in a namespace the
//! cross-reference does not map, ahead of localization and resolution.
//!
//! ```toml
//! [pdqm]
//! url = "https://pdq.example.org/fhir/"
//! transaction = "iti-78"
//! master = "urn:oid:2.999.1"
//! timeout_ms = 1000
//!
//! [pdqm.namespaces]
//! "2.999.7" = "urn:oid:2.999.7"
//!
//! [pdqm.credentials]
//! bearer_token_file = "/run/secrets/pdq-token"
//! ```
//!
//! `client_identity_file` and `trust_roots_file` give the mutual TLS the
//! Supplier asks for, as `[xcpd]` does ([`tls`](crate::config::tls)).
//!
//! `transaction` is `iti-78`, the Mobile Patient Demographics Query, or
//! `iti-119`, the Patient Demographics Match, where the deployment declares
//! it. `master` is the identifier system of the master domain, whose
//! identifier the cross-reference resolves; each `namespaces` key is a client
//! namespace taken to the Supplier, mapped to the identifier system the
//! identifier is sent in. The Supplier is sent the patient identifier, so its
//! URL is held to the protected-payload policy of [`transport`]. A reload
//! applies a change, as it does to `[pixm]`. No specification governs the
//! shape of the table: our own design.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_identity::pdqm::{PdqmConfig, PdqmDemographics, Transaction};
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::binding::ihe::audit::config::AuditSettings;
use crate::config::error::Error;
use crate::config::resolve::localization_budget_ms;
use crate::config::secrets::resolve_credentials;
use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;
use crate::config::{Config, Credentials, transport};
use crate::federation::DemographicsStep;
use crate::federation::error::FederationError;
use crate::service;

/// The key of the table.
pub const PDQM_KEY: &str = "pdqm";

/// The PDQm demographics step, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pdqm {
    /// The Supplier's FHIR base URL, `https`; `http` under development only.
    pub url: SecretUrl,
    /// How the gateway authenticates to the Supplier, when the transport
    /// does not.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS with the Supplier, inline or through
    /// `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the Supplier's certificate chains to,
    /// beside the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// The transaction the Supplier is asked with: `iti-78` or `iti-119`.
    pub transaction: Transaction,
    /// The identifier system of the master domain.
    pub master: String,
    /// Each client namespace taken to the Supplier, mapped to the identifier
    /// system the identifier is sent in.
    pub namespaces: BTreeMap<String, String>,
    /// How long one exchange with the Supplier may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for Pdqm {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
            transaction: Transaction::Search,
            master: String::new(),
            namespaces: BTreeMap::new(),
            timeout_ms: 1_000,
        }
    }
}

/// The PDQm demographics step, with every secret read.
#[derive(Debug)]
pub struct PdqmSettings {
    /// The Supplier's FHIR base URL, already known to parse.
    pub url: SecretUrl,
    /// How the gateway authenticates to it: a bearer token or basic
    /// credentials.
    pub credentials: Option<Scheme>,
    /// The TLS material it is reached with.
    pub tls: TlsSettings,
    /// The transaction the Supplier is asked with.
    pub transaction: Transaction,
    /// The identifier system of the master domain, as written.
    pub master: String,
    /// The client namespaces taken to the Supplier, as written.
    pub namespaces: BTreeMap<String, String>,
    /// How long one exchange may take.
    pub timeout: Duration,
}

/// Resolves `[pdqm]`: a URL that parses, carries no userinfo and is `https`
/// outside development, a bearer token or basic credentials, a master
/// domain, at least one namespace and none the cross-reference maps, and a
/// positive timeout that, with the localizer's, ends before the overall
/// budget (§11.5).
///
/// # Errors
/// [`Error::Missing`] for no `url`, `master` or `namespaces`, [`Error::Zero`]
/// for a zero timeout, [`Error::DemographicsBudget`] for a budget that leaves no
/// time to resolve, [`Error::Url`], [`Error::UrlCredentials`] and
/// [`Error::Cleartext`] for the URL, [`Error::GrantNotHere`] for an OAuth 2.0
/// or Nuts grant, [`Error::Pdqm`] for a namespace `[pixm.namespaces]` maps,
/// and the errors of a secret that cannot be read.
pub(super) fn resolve(config: &Config) -> Result<Option<PdqmSettings>, Error> {
    let Some(pdqm) = &config.pdqm else {
        return Ok(None);
    };
    let missing = |key: &str| Error::Missing {
        key: format!("{PDQM_KEY}.{key}"),
    };
    let url_key = format!("{PDQM_KEY}.url");
    if pdqm.url.expose().is_empty() {
        return Err(missing("url"));
    }
    let url = url::Url::parse(pdqm.url.expose()).map_err(|source| Error::Url {
        key: url_key.clone(),
        source,
    })?;
    // NOTE: no specification governs this: our own design; as the registry
    // refuses it on an endpoint URL, a credential goes in its own section.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::UrlCredentials {
            key: url_key,
            section: format!("{PDQM_KEY}.credentials"),
        });
    }
    if pdqm.master.is_empty() {
        return Err(missing("master"));
    }
    if pdqm.namespaces.is_empty() {
        return Err(missing("namespaces"));
    }
    if pdqm.timeout_ms == 0 {
        return Err(Error::Zero {
            key: format!("{PDQM_KEY}.timeout_ms"),
        });
    }
    let localization_ms = localization_budget_ms(config);
    let overall_ms = config.federation.overall_timeout_ms;
    if pdqm.timeout_ms.saturating_add(localization_ms) >= overall_ms {
        return Err(Error::DemographicsBudget {
            timeout_ms: pdqm.timeout_ms,
            localization_ms,
            overall_ms,
        });
    }
    // NOTE: Annex A §A.2: the step serves an identifier the cross-reference does
    // not map, so a namespace mapped in both would have two meanings.
    if let Some(mapped) = config.pixm.as_ref().and_then(|pixm| {
        pdqm.namespaces
            .keys()
            .find(|namespace| pixm.namespaces.contains_key(*namespace))
    }) {
        return Err(Error::Pdqm {
            key: format!("{PDQM_KEY}.namespaces.\"{mapped}\""),
            fault: "is a namespace [pixm.namespaces] maps, so the cross-reference resolves it as it is",
        });
    }
    let section = format!("{PDQM_KEY}.credentials");
    let credentials = pdqm
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if credentials.as_ref().is_some_and(Scheme::is_grant) {
        return Err(Error::GrantNotHere { section });
    }
    // NOTE: no specification governs this: our own design; the Supplier is sent
    // patient identifiers, held to https at load as every identity service is.
    transport::protected_payload(
        config.profile,
        pdqm.url.expose(),
        transport::identity_site(PDQM_KEY, credentials.is_some().then_some(section.as_str())),
    )?;
    let tls = crate::config::tls::resolve(
        PDQM_KEY,
        pdqm.client_identity.as_ref(),
        pdqm.client_identity_file.as_deref(),
        pdqm.trust_roots_file.as_ref(),
    )?;
    Ok(Some(PdqmSettings {
        url: pdqm.url.clone(),
        credentials,
        tls,
        transaction: pdqm.transaction,
        master: pdqm.master.clone(),
        namespaces: pdqm.namespaces.clone(),
        timeout: Duration::from_millis(pdqm.timeout_ms),
    }))
}

/// The demographics step `pdqm` describes, audited through `[audit]`.
pub(super) fn step(
    pdqm: &PdqmSettings,
    audit: &AuditSettings,
) -> Result<DemographicsStep, FederationError> {
    let auth = service::authentication("pdqm.credentials", pdqm.credentials.as_ref())?;
    let tls = service::tls_of("pdqm", &pdqm.tls).map_err(FederationError::Tls)?;
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pdqm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(FederationError::PdqmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let step = PdqmDemographics::from_config(PdqmConfig {
        base: pdqm.url.clone(),
        auth,
        tls,
        transaction: pdqm.transaction,
        master: pdqm.master.clone(),
        namespaces,
    })
    .map_err(FederationError::Pdqm)?;
    // NOTE: PDQm §2:3.78.5.1 and §2:3.119.5.1.1: each exchange is audited, and one
    // whose record is refused fails, so the patient's resolution fails closed.
    let step = match super::audit::recorder(audit).map_err(FederationError::Audit)? {
        Some(recorder) => step.audited(&recorder),
        None => step,
    };
    Ok(DemographicsStep::new(Arc::new(step), pdqm.timeout))
}
