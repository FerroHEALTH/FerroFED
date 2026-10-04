// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Mitz consent pre-filter, `[nl_gf.mitz]`: the optional Step-1
//! pre-filter over GF-Consent, the closed authorization question of the
//! Dutch Generic Functions (Annex B §B.6, N27a, §13.2.1).
//!
//! The table names the Mitz endpoint and how the gateway reaches it, the
//! data user (the deployment's own organisation, with the responsible
//! professional and their role), the data categories and the purpose the
//! question asks about, and each member's care provider, the data holder.
//! A holder's URA may be left out where `[nl_gf.nvi.custodians]` or the
//! directory gives it, and must agree with them where both give one.
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
//! [nl_gf.mitz.data_user]
//! ura = "ura-test-0100"
//! type = "V6"
//! responsible_root = "2.999.10"
//! responsible = "professional0001"
//! role = "01.015"
//!
//! [nl_gf.mitz.holders]
//! "node-a" = { type = "V6" }
//! "node-b" = { type = "V6", ura = "ura-test-0002" }
//! ```
//!
//! Mitz is sent the BSN, so its URL is held to the protected-payload policy
//! of [`transport`]. No specification governs the shape of the table: our
//! own design.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_identity::mitz::{is_pseudonym_system, is_purpose};
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::secrets::{resolve_credentials, secret};
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
    /// The data user.
    pub data_user: DataUser,
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
            data_user: DataUser::default(),
            holders: BTreeMap::new(),
        }
    }
}

/// The data user, as the configuration writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DataUser {
    /// The organisation's URA.
    pub ura: String,
    /// The organisation's care provider category.
    #[serde(rename = "type")]
    pub kind: String,
    /// The OID the responsible professional's number is issued under.
    pub responsible_root: String,
    /// The responsible professional's identification number.
    pub responsible: String,
    /// The responsible professional's UZI role code.
    pub role: String,
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
    /// The data user, as written.
    pub data_user: DataUser,
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
            .field("data_user", &self.data_user)
            .field("holders", &self.holders)
            .finish()
    }
}

/// Resolves `[nl_gf.mitz]`: a URL that parses, carries no userinfo and is
/// `https` outside development, a bearer token or basic credentials, a
/// purpose and data categories, a data user, and every secret and file read.
///
/// # Errors
/// [`Error::Missing`] for no registry, no `url`, no `purpose`, no
/// `data_categories`, a data user key left empty or a zero timeout,
/// [`Error::Mitz`] for a purpose other than `TREAT` or `COC` or the
/// pseudonymised BSN listed in `namespaces`, [`Error::Url`],
/// [`Error::UrlCredentials`] and [`Error::Cleartext`] for the URL,
/// [`Error::GrantNotHere`] for an OAuth 2.0 grant, and the errors of a
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
    let section = format!("{MITZ_KEY}.credentials");
    let credentials = mitz
        .credentials
        .as_ref()
        .map(|credentials| resolve_credentials(&section, credentials))
        .transpose()?;
    if matches!(credentials, Some(Scheme::OAuth2(_))) {
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
        data_user: mitz.data_user.clone(),
        holders: mitz.holders.clone(),
    })
}

/// Holds the question's own keys to what the closed authorization question
/// takes: a purpose, data categories, no pseudonym as the BSN, a timeout and
/// a data user.
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
        .any(|namespace| is_pseudonym_system(namespace))
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
    let user = &mitz.data_user;
    for (key, value) in [
        ("ura", &user.ura),
        ("type", &user.kind),
        ("responsible_root", &user.responsible_root),
        ("responsible", &user.responsible),
        ("role", &user.role),
    ] {
        if value.is_empty() {
            return Err(missing(&format!("data_user.{key}")));
        }
    }
    Ok(())
}
