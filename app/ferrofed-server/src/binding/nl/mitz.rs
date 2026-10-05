// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! client_identity_file = "/run/secrets/mitz-client.pem"
//! trust_roots_file = "/etc/ferrofed/mitz-roots.pem"
//! namespaces = ["urn:oid:2.999.1"]
//! purpose = "TREAT"
//! data_categories = ["GGC002"]
//!
//! [nl_gf.mitz.holders]
//! "node-a" = { type = "V6" }
//! "node-b" = { type = "V6", ura = "ura-test-0002" }
//! ```
//!
//! Mitz is sent the BSN, so its URL is held to the protected-payload policy
//! of [`transport`]. No specification governs the shape of the table: our
//! own design.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::dev::Profile;
use ferrofed_identity::nl::mitz::{HolderConfig, MitzConfig, MitzPrefilter, is_purpose};
use ferrofed_identity::role::consent::ConsentPrefilter;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::{Secret, SecretUrl};
use ferrofed_registry::snapshot::RegistrySnapshot;
use nl_generic_functions::identification::PSEUDO_BSN_SYSTEM;
use serde::Deserialize;

use crate::binding::nl::NlGfSettings;
use crate::config::error::Error;
use crate::config::resolve::localization_budget_ms;
use crate::config::secrets::{resolve_credentials, secret};
use crate::config::settings::{Scheme, Settings};
use crate::config::{Config, Credentials, transport};
use crate::federation::error::FederationError;
use crate::service;

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
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS, inline or through `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots Mitz's certificate chains to.
    pub trust_roots_file: Option<PathBuf>,
    /// The client namespaces that stand for the BSN, beside the BSN systems.
    pub namespaces: Vec<String>,
    /// The purpose of use: `TREAT` or `COC`.
    pub purpose: String,
    /// The Mitz data categories asked about.
    pub data_categories: Vec<String>,
    /// How long one round of questions may take, in milliseconds.
    pub timeout_ms: u64,
    /// Each member's data holder, by member id.
    pub holders: BTreeMap<String, Holder>,
}

impl Default for Mitz {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
            namespaces: Vec::new(),
            purpose: String::new(),
            data_categories: Vec::new(),
            timeout_ms: 1_000,
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
    /// The client certificate chain and key.
    pub client_identity: Option<Secret>,
    /// The PEM trust roots.
    pub trust_roots: Option<String>,
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
            .field("client_identity", &self.client_identity.is_some())
            .field("trust_roots", &self.trust_roots.is_some())
            .field("namespaces", &self.namespaces)
            .field("purpose", &self.purpose)
            .field("data_categories", &self.data_categories)
            .field("timeout", &self.timeout)
            .field("holders", &self.holders)
            .finish()
    }
}

/// Resolves `[nl_gf.mitz]`: a URL that parses, carries no userinfo and is
/// `https` outside development, a bearer token or basic credentials, a
/// purpose and data categories, a positive timeout that, with the
/// demographics step's and the localizer's, ends before the overall budget
/// (§11.5), a data user, and every secret and file read.
///
/// # Errors
/// [`Error::Missing`] for no registry, no `url`, no `purpose`, no
/// `data_categories`, a data user key left empty or a zero timeout,
/// [`Error::Mitz`] for a purpose other than `TREAT` or `COC` or the
/// pseudonymised BSN listed in `namespaces`, [`Error::Url`],
/// [`Error::UrlCredentials`] and [`Error::Cleartext`] for the URL,
/// [`Error::GrantNotHere`] for an OAuth 2.0 grant,
/// [`Error::PrefilterBudget`] for a budget that, with the demographics
/// step's and the localizer's, leaves no time to resolve, and the errors of a
/// secret or a file that cannot be read.
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
    let credentials = mitz
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if credentials.as_ref().is_some_and(Scheme::is_grant) {
        return Err(Error::GrantNotHere { section });
    }
    // NOTE: Implementatiehandleiding §3.3: the question travels over TLS, and it carries
    // the BSN, so the URL is a protected-payload site; one admitted under development is reported.
    transport::protected_payload(
        config.profile,
        mitz.url.expose(),
        transport::identity_site(MITZ_KEY, credentials.is_some().then_some(section.as_str())),
    )?;
    let client_identity = secret(
        "nl_gf.mitz.client_identity",
        mitz.client_identity.as_ref(),
        mitz.client_identity_file.as_deref(),
    )?;
    let trust_roots = mitz
        .trust_roots_file
        .as_ref()
        .map(|path| {
            std::fs::read_to_string(path).map_err(|source| Error::Secret {
                key: String::from("nl_gf.mitz.trust_roots_file"),
                path: path.clone(),
                source,
            })
        })
        .transpose()?;
    Ok(MitzSettings {
        url: mitz.url.clone(),
        credentials,
        client_identity,
        trust_roots,
        namespaces: mitz.namespaces.clone(),
        purpose: mitz.purpose.clone(),
        data_categories: mitz.data_categories.clone(),
        timeout: Duration::from_millis(mitz.timeout_ms),
        holders: mitz.holders.clone(),
    })
}

/// Holds the pre-filter's budget, with the demographics step's and the
/// localizer's, below the overall budget, of which each is a part (§11.5).
fn budget(config: &Config, mitz: &Mitz) -> Result<(), Error> {
    let demographics_ms = crate::binding::budgets(config).demographics_ms;
    let localization_ms = localization_budget_ms(config);
    let overall_ms = config.federation.overall_timeout_ms;
    if mitz
        .timeout_ms
        .saturating_add(demographics_ms)
        .saturating_add(localization_ms)
        >= overall_ms
    {
        return Err(Error::PrefilterBudget {
            timeout_ms: mitz.timeout_ms,
            demographics_ms,
            localization_ms,
            overall_ms,
        });
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
        return Err(Error::Mitz {
            key: format!("{MITZ_KEY}.purpose"),
            fault: "is neither TREAT nor COC (Implementatiehandleiding §3.2.4.2)",
        });
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
        return Err(Error::Mitz {
            key: format!("{MITZ_KEY}.namespaces"),
            fault: "lists the pseudonymised BSN, which cannot stand for the BSN Mitz is asked by",
        });
    }
    if mitz.timeout_ms == 0 {
        return Err(Error::Zero {
            key: format!("{MITZ_KEY}.timeout_ms"),
        });
    }
    Ok(())
}

/// The Mitz pre-filter `[nl_gf.mitz]` describes over the members of
/// `snapshot`, with each holder's URA also read from `[nl_gf.nvi.custodians]`
/// when that table is set; `None` when `[nl_gf.mitz]` is not.
pub(super) fn prefilter(
    settings: &Settings,
    nl_gf: &NlGfSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<Arc<dyn ConsentPrefilter>>, FederationError> {
    let Some(mitz) = &nl_gf.mitz else {
        return Ok(None);
    };
    let mut custodians = BTreeMap::new();
    for (ura, member) in nl_gf
        .nvi
        .as_ref()
        .map(|nvi| &nvi.custodians)
        .into_iter()
        .flatten()
    {
        custodians.insert(
            ura.clone(),
            node(&format!("nl_gf.nvi.custodians.{ura:?}"), member)?,
        );
    }
    let config = MitzConfig {
        endpoint: mitz.url.clone(),
        development: settings.profile == Profile::Development,
        auth: service::authentication(
            &format!("{MITZ_KEY}.credentials"),
            mitz.credentials.as_ref(),
        )?,
        tls: service::tls(
            MITZ_KEY,
            mitz.client_identity.as_ref(),
            mitz.trust_roots.as_deref(),
        )
        .map_err(FederationError::Tls)?,
        namespaces: namespaces(mitz)?,
        categories: mitz.data_categories.clone(),
        purpose: mitz.purpose.clone(),
        holders: holders(mitz)?,
        custodians,
        timeout: mitz.timeout,
    };
    let prefilter = MitzPrefilter::from_config(config, snapshot).map_err(FederationError::Mitz)?;
    Ok(Some(Arc::new(prefilter)))
}

/// The member `value` names, read at `key`.
fn node(key: &str, value: &str) -> Result<NodeId, FederationError> {
    NodeId::new(value).map_err(|source| FederationError::MitzMember {
        key: key.to_owned(),
        source,
    })
}

/// The namespaces that stand for the BSN.
fn namespaces(mitz: &MitzSettings) -> Result<BTreeSet<IdentifierNamespace>, FederationError> {
    mitz.namespaces
        .iter()
        .map(|namespace| IdentifierNamespace::new(namespace.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(FederationError::MitzNamespace)
}

/// Each member's data holder, by member id.
fn holders(mitz: &MitzSettings) -> Result<BTreeMap<NodeId, HolderConfig>, FederationError> {
    let mut holders = BTreeMap::new();
    for (member, holder) in &mitz.holders {
        let key = format!("{MITZ_KEY}.holders.{member:?}");
        holders.insert(
            node(&key, member)?,
            HolderConfig {
                ura: holder.ura.clone(),
                kind: holder.kind.clone(),
            },
        );
    }
    Ok(holders)
}
