// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PDQm demographics step, `[pdqm]` (Annex A §A.2 and §A.7).
//!
//! The table names the Patient Demographics Supplier the gateway asks for the
//! master identity of a patient identifier issued in a namespace the
//! cross-reference does not map, ahead of localization and resolution.
//!
//! ```toml
//! [pdqm]
//! transaction = "iti-78"
//! master = "urn:oid:2.999.1"
//!
//! [pdqm.endpoint]
//! url = "https://pdq.example.org/fhir/"
//! timeout = "1s"
//!
//! [pdqm.namespaces]
//! "2.999.7" = "urn:oid:2.999.7"
//!
//! [pdqm.endpoint.credentials.oauth]
//! token_endpoint = "https://pdq.example.org/auth/token"
//! client_id = "ferrofed-pdq-consumer"
//! client_secret_file = "/run/secrets/pdq-consumer"
//! scope = "*"
//! ```
//!
//! `[pdqm.endpoint]` is `ferrohealth-smart`'s `EndpointSection`: its
//! `timeout` bounds one exchange with the Supplier, every retry included,
//! and its `tls` gives the mutual TLS the Supplier asks for.
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

use ferrohealth_config::removed::Removed;
use ferrohealth_config::units;
use ferrohealth_smart::config::endpoint::EndpointSection;
use std::collections::BTreeMap;
use std::time::Duration;

use ferrofed_identity::ihe::pdqm::Transaction;
use serde::Deserialize;
use url::Url;

use crate::config::error::Error;
use crate::config::error::binding::BindingFault;
use crate::config::resolve::localization_budget_ms;
use crate::config::service_grant::ServiceContext;
use crate::config::settings::Scheme;
use crate::config::{Config, transport};

/// The key of the table.
pub const PDQM_KEY: &str = "pdqm";

/// The key of its endpoint table.
pub const PDQM_ENDPOINT: &str = "pdqm.endpoint";

/// The PDQm demographics step, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Pdqm {
    /// The Supplier's FHIR base and how it is reached: `https`, `http` under
    /// development only. Its `timeout` bounds one exchange.
    pub endpoint: EndpointSection,
    /// The transaction the Supplier is asked with: `iti-78` or `iti-119`.
    pub transaction: Transaction,
    /// The identifier system of the master domain.
    pub master: String,
    /// Each client namespace taken to the Supplier, mapped to the identifier
    /// system the identifier is sent in.
    pub namespaces: BTreeMap<String, String>,
    /// The retired `timeout_ms`, which `endpoint.timeout` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub timeout_ms: Removed,
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

impl Default for Pdqm {
    fn default() -> Self {
        Self {
            // NOTE: no specification governs this: our own design; the step runs ahead of
            // localization within the overall budget, so one exchange takes a second by default.
            endpoint: crate::config::endpoint::tried_once(units::Duration::from_secs(1)),
            transaction: Transaction::Search,
            master: String::new(),
            namespaces: BTreeMap::new(),
            timeout_ms: Removed::default(),
            url: Removed::default(),
            credentials: Removed::default(),
            tls: Removed::default(),
        }
    }
}

/// The PDQm demographics step, with every secret read.
#[derive(Debug)]
pub struct PdqmSettings {
    /// The Supplier's FHIR base URL, read from `endpoint`.
    pub url: Url,
    /// How the gateway authenticates to it: basic credentials or the client
    /// credentials grant of `oauth`.
    pub credentials: Option<Scheme>,
    /// How the Supplier is reached, as `ferrohealth-smart`'s outbound client
    /// reads it.
    pub endpoint: Box<EndpointSection>,
    /// The transaction the Supplier is asked with.
    pub transaction: Transaction,
    /// The identifier system of the master domain, as written.
    pub master: String,
    /// The client namespaces taken to the Supplier, as written.
    pub namespaces: BTreeMap<String, String>,
    /// How long one exchange may take.
    pub timeout: Duration,
}

/// Resolves `[pdqm]`: an endpoint table whose URL carries no userinfo and is
/// `https` outside development, with basic credentials or an `oauth` grant,
/// a master domain, at least one namespace and none the cross-reference
/// maps, and a positive timeout that, with the localizer's, ends before the
/// overall budget (§11.5).
///
/// # Errors
/// [`Error::Missing`] for no `endpoint.url`, `master` or `namespaces`,
/// [`Error::Zero`] for a zero timeout, [`BindingFault::DemographicsBudget`] for a budget that
/// leaves no time to resolve, [`Error::Url`], [`Error::UrlCredentials`] and
/// [`Error::Cleartext`] for the URL, [`CredentialsFault::GrantNotHere`] for an
/// OAuth 2.0 or Nuts grant, [`BindingFault::Pdqm`] for a namespace
/// `[pixm.namespaces]` maps, and the errors of a secret that cannot be read.
///
/// [`CredentialsFault::GrantNotHere`]: crate::config::error::credentials::CredentialsFault::GrantNotHere
pub(crate) fn resolve(
    config: &Config,
    context: ServiceContext,
) -> Result<Option<PdqmSettings>, Error> {
    let Some(pdqm) = &config.pdqm else {
        return Ok(None);
    };
    let missing = |key: &str| Error::Missing {
        key: format!("{PDQM_KEY}.{key}"),
    };
    let endpoint = crate::config::endpoint::service(PDQM_ENDPOINT, &pdqm.endpoint, context)?;
    if pdqm.master.is_empty() {
        return Err(missing("master"));
    }
    if pdqm.namespaces.is_empty() {
        return Err(missing("namespaces"));
    }
    let timeout = pdqm.endpoint.timeout;
    let localization_ms = localization_budget_ms(config);
    let overall_ms = config.federation.overall_timeout.as_millis();
    if timeout.as_millis().saturating_add(localization_ms) >= overall_ms {
        return Err(Error::Binding(BindingFault::DemographicsBudget {
            timeout_ms: timeout.as_millis(),
            localization_ms,
            overall_ms,
        }));
    }
    // NOTE: Annex A §A.2: the step serves an identifier the cross-reference does
    // not map, so a namespace mapped in both would have two meanings.
    if let Some(mapped) = config.pixm.as_ref().and_then(|pixm| {
        pdqm.namespaces
            .keys()
            .find(|namespace| pixm.namespaces.contains_key(*namespace))
    }) {
        return Err(Error::Binding(BindingFault::Pdqm {
            key: format!("{PDQM_KEY}.namespaces.\"{mapped}\""),
            fault: "is a namespace [pixm.namespaces] maps, so the cross-reference resolves it as it is",
        }));
    }
    let section = format!("{PDQM_ENDPOINT}.credentials");
    // NOTE: no specification governs this: our own design; the Supplier is sent
    // patient identifiers, held to https at load as every identity service is.
    transport::protected_payload(
        config.profile,
        endpoint.url.as_str(),
        transport::identity_site(
            PDQM_ENDPOINT,
            endpoint.credentials.is_some().then_some(section.as_str()),
        ),
    )?;
    Ok(Some(PdqmSettings {
        url: endpoint.url,
        credentials: endpoint.credentials,
        endpoint: Box::new(pdqm.endpoint.clone()),
        transaction: pdqm.transaction,
        master: pdqm.master.clone(),
        namespaces: pdqm.namespaces.clone(),
        timeout: timeout.get(),
    }))
}
