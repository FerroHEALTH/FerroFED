// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter, `[nl_gf.mitz]`: the optional Step-1
//! pre-filter over GF-Consent, the closed authorization question of the
//! Dutch Generic Functions (Annex B §B.6, N27a, §13.2.1).
//!
//! The table names what belongs to the gateway: the Mitz endpoint and how the
//! gateway reaches it, the data categories and the purpose the question asks
//! about, and each member's care provider, the data holder. A holder's URA
//! may be left out where `[nl_gf.nvi.custodians]` or the directory gives it,
//! and must agree with them where both give one. Who asks, the data user, is
//! the verified caller, read from the claims `[auth.issuer.requester]` names,
//! never from this table.
//!
//! ```toml
//! [nl_gf.mitz]
//! url = "https://mitz.example.org/geslotenautorisatievraag"
//! tls = { client_identity_file = "/run/secrets/mitz-client.pem", trust_roots_file = "/etc/ferrofed/mitz-roots.pem" }
//! namespaces = ["urn:oid:2.999.1"]
//! purpose = "TREAT"
//! data_categories = ["GGC002"]
//!
//! [nl_gf.mitz.holders]
//! "node-a" = { type = "V6" }
//! "node-b" = { type = "V6", ura = "ura-test-0002" }
//! ```
//!
//! Beside mutual TLS, the gateway authenticates with basic credentials or
//! with the OAuth 2.0 client credentials grant of `ferrohealth-smart` (RFC
//! 6749 §4.4), whose token each question carries as a bearer token (RFC 6750
//! §2.1):
//!
//! ```toml
//! [nl_gf.mitz.credentials.oauth]
//! token_endpoint = "https://mitz.example.org/oauth2/token"
//! client_id = "ferrofed"
//! client_auth = "client_secret_basic"
//! client_secret_file = "/run/secrets/mitz-client-secret"
//! ```
//!
//! Mitz is sent the BSN, so its URL is held to the protected-payload policy
//! of [`transport`]. No specification governs the shape of the table: our
//! own design.

use ferrohealth_config::removed::Removed;
use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_config::units;
use ferrohealth_server::tls_client::TlsClient;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::nl::mitz::is_purpose;
use ferrohealth_secret::secret_url::SecretUrl;
use nl_generic_functions::identification::PSEUDO_BSN_SYSTEM;
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::error::binding::BindingFault;
use crate::config::resolve::localization_budget_ms;
use crate::config::service_grant::{ServiceContext, resolve_service};
use crate::config::settings::Scheme;
use crate::config::{Config, Credentials, transport};

/// The key of the Mitz table.
pub const MITZ_KEY: &str = "nl_gf.mitz";

/// The Mitz consent pre-filter, as the configuration writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Mitz {
    /// The endpoint of the closed authorization question, `https`.
    pub url: SecretUrl,
    /// How the gateway authenticates to Mitz beside mutual TLS, if at all.
    pub credentials: Option<Credentials>,
    /// The TLS material the service is reached with: the table's `tls`
    /// sub-table.
    pub tls: TlsClientSection,
    /// The client namespaces that stand for the BSN, beside the BSN systems.
    pub namespaces: Vec<String>,
    /// The purpose of use: `TREAT` or `COC`.
    pub purpose: String,
    /// The Mitz data categories asked about.
    pub data_categories: Vec<String>,
    /// How long one round of questions may take.
    pub timeout: units::Duration,
    /// The retired `timeout_ms`, which `timeout` replaced in
    /// 0.0.19: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub timeout_ms: Removed,
    /// Each member's data holder, by member id.
    pub holders: BTreeMap<String, Holder>,
}

impl Default for Mitz {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            tls: TlsClientSection::default(),
            namespaces: Vec::new(),
            purpose: String::new(),
            data_categories: Vec::new(),
            timeout: units::Duration::from_secs(1),
            timeout_ms: Removed::default(),
            holders: BTreeMap::new(),
        }
    }
}

/// One member's data holder, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Holder {
    /// The care provider's URA, when neither the NVI custodians nor the
    /// directory give it.
    pub ura: Option<String>,
    /// The care provider's category.
    #[serde(rename = "type")]
    pub kind: String,
}

/// The Mitz consent pre-filter, with every secret and file read.
pub struct MitzSettings {
    /// The endpoint, already known to parse.
    pub url: SecretUrl,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
    /// The TLS material it is reached with, every file read.
    pub tls: Arc<TlsClient>,
    /// The namespaces that stand for the BSN, as written.
    pub namespaces: Vec<String>,
    /// The purpose, `TREAT` or `COC`.
    pub purpose: String,
    /// The data categories, as written.
    pub data_categories: Vec<String>,
    /// How long one round of questions may take.
    pub timeout: Duration,
    /// The data holders, as written.
    pub holders: BTreeMap<String, Holder>,
}

impl fmt::Debug for MitzSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MitzSettings")
            .field("url", &self.url)
            .field("credentials", &self.credentials.is_some())
            .field("tls", &self.tls)
            .field("namespaces", &self.namespaces)
            .field("purpose", &self.purpose)
            .field("data_categories", &self.data_categories)
            .field("timeout", &self.timeout)
            .field("holders", &self.holders)
            .finish()
    }
}

/// Resolves `[nl_gf.mitz]`: a URL that parses, carries no userinfo and is
/// `https` outside development, basic credentials or an `oauth` grant, a
/// purpose and data categories, a positive timeout that, with the
/// demographics step's and the localizer's, ends before the overall budget
/// (§11.5), a data user, and every secret and file read.
///
/// # Errors
/// [`Error::Missing`] for no registry, no `url`, no `purpose`, no
/// `data_categories`, a data user key left empty or a zero timeout,
/// [`BindingFault::Mitz`] for a purpose other than `TREAT` or `COC` or the
/// pseudonymised BSN listed in `namespaces`, [`Error::Url`],
/// [`Error::UrlCredentials`] and [`Error::Cleartext`] for the URL and the
/// token endpoint, [`CredentialsFault::ServiceGrantNotHere`] for a Nuts
/// grant,
/// [`BindingFault::PrefilterBudget`] for a budget that, with the demographics
/// step's and the localizer's, leaves no time to resolve, and the errors of a
/// secret or a file that cannot be read.
///
/// [`CredentialsFault::ServiceGrantNotHere`]: crate::config::error::credentials::CredentialsFault::ServiceGrantNotHere
pub(super) fn resolve(config: &Config, mitz: &Mitz) -> Result<MitzSettings, Error> {
    let missing = |key: &str| Error::Missing {
        key: format!("{MITZ_KEY}.{key}"),
    };
    // NOTE: no specification governs this: our own design; the holders name
    // registry members, so the table means nothing without a registry.
    if !config.registry.configured() {
        return Err(Error::Missing {
            key: String::from("registry.document"),
        });
    }
    let url_key = format!("{MITZ_KEY}.url");
    if mitz.url.expose().is_empty() {
        return Err(missing("url"));
    }
    let url = url::Url::parse(mitz.url.expose()).map_err(|source| Error::Url {
        key: url_key.clone(),
        source,
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::UrlCredentials {
            key: url_key,
            section: format!("{MITZ_KEY}.credentials"),
        });
    }
    question(mitz)?;
    budget(config, mitz)?;
    let section = format!("{MITZ_KEY}.credentials");
    let context = ServiceContext {
        profile: config.profile,
    };
    let credentials = mitz
        .credentials
        .as_ref()
        .map(|credentials| resolve_service(&section, credentials, context))
        .transpose()?;
    // NOTE: Implementatiehandleiding §3.3: the question travels over TLS, and it carries
    // the BSN, so the URL is a protected-payload site; one admitted under development is reported.
    transport::protected_payload(
        config.profile,
        mitz.url.expose(),
        transport::identity_site(MITZ_KEY, credentials.is_some().then_some(section.as_str())),
    )?;
    let tls = crate::config::tls::read("nl_gf.mitz.tls", &mitz.tls)?;
    Ok(MitzSettings {
        url: mitz.url.clone(),
        credentials,
        tls,
        namespaces: mitz.namespaces.clone(),
        purpose: mitz.purpose.clone(),
        data_categories: mitz.data_categories.clone(),
        timeout: Duration::from_millis(mitz.timeout.as_millis()),
        holders: mitz.holders.clone(),
    })
}

/// Holds the pre-filter's budget, with the demographics step's and the
/// localizer's, below the overall budget, of which each is a part (§11.5).
fn budget(config: &Config, mitz: &Mitz) -> Result<(), Error> {
    let demographics_ms = config
        .pdqm
        .as_ref()
        .map_or(0, |pdqm| pdqm.timeout.as_millis());
    let localization_ms = localization_budget_ms(config);
    let overall_ms = config.federation.overall_timeout.as_millis();
    if mitz
        .timeout
        .as_millis()
        .saturating_add(demographics_ms)
        .saturating_add(localization_ms)
        >= overall_ms
    {
        return Err(Error::Binding(BindingFault::PrefilterBudget {
            timeout_ms: mitz.timeout.as_millis(),
            demographics_ms,
            localization_ms,
            overall_ms,
        }));
    }
    Ok(())
}

/// Holds the question's own keys to what the closed authorization question
/// takes: a purpose, data categories, no pseudonym as the BSN and a
/// timeout.
fn question(mitz: &Mitz) -> Result<(), Error> {
    let missing = |key: &str| Error::Missing {
        key: format!("{MITZ_KEY}.{key}"),
    };
    if mitz.purpose.is_empty() {
        return Err(missing("purpose"));
    }
    // NOTE: Implementatiehandleiding Open en gesloten autorisatievraag 3.8.2 §3.2.4.2:
    // the consultation situation is TREAT or COC, and nothing else.
    if !is_purpose(&mitz.purpose) {
        return Err(Error::Binding(BindingFault::Mitz {
            key: format!("{MITZ_KEY}.purpose"),
            fault: "is neither TREAT nor COC (Implementatiehandleiding §3.2.4.2)",
        }));
    }
    if mitz.data_categories.is_empty() {
        return Err(missing("data_categories"));
    }
    // NOTE: Annex B §B.1, §B.6: Mitz is asked by BSN, so a pseudonym listed as
    // the BSN would reach Mitz labelled as one.
    if mitz
        .namespaces
        .iter()
        .any(|namespace| namespace.as_str() == PSEUDO_BSN_SYSTEM)
    {
        return Err(Error::Binding(BindingFault::Mitz {
            key: format!("{MITZ_KEY}.namespaces"),
            fault: "lists the pseudonymised BSN, which cannot stand for the BSN Mitz is asked by",
        }));
    }
    if mitz.timeout.as_millis() == 0 {
        return Err(Error::Zero {
            key: format!("{MITZ_KEY}.timeout"),
        });
    }
    Ok(())
}
