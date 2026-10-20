// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[eehrxf]` table: the interoperability component's own settings.
//!
//! `[eehrxf.terminology]` names the FHIR terminology server the component
//! asks for `$translate`, `$lookup` and `$validate-code`
//! (<https://hl7.org/fhir/R4/terminology-service.html>):
//!
//! ```toml
//! [eehrxf.terminology]
//! url = "https://terminology.example.org/fhir/"
//! timeout_ms = 2000
//!
//! [eehrxf.terminology.credentials]
//! user = "ferrofed"
//! password_file = "/run/secrets/terminology-password"
//! ```
//!
//! `client_identity`, `client_identity_file` and `trust_roots_file` give the
//! mutual TLS the server asks for, as `[pdqm]` does
//! ([`tls`](crate::config::tls)). The server is sent the codes of the
//! documents the face receives, which are health data, so its URL is held to
//! the protected-payload policy of [`transport`]. The table is optional:
//! with none, the component fails closed where it needs a server, refusing a
//! received coding that carries no display, and names every `required`
//! binding no vendored expansion decides as not evaluated. It serves the
//! `[fhir]` face, so it is refused without one. No specification governs the
//! shape of the table: our own design.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use eehrxf::terminology::Terminology;
use ferrofed_eehrxf::terminology::{FhirTerminology, FhirTerminologyError};
use ferrofed_registry::secret::{Secret, SecretUrl};
use serde::Deserialize;

use crate::config::error::Error;
use crate::config::secrets::resolve_credentials;
use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;
use crate::config::transport::{self, Encryption, ProtectedSite};
use crate::config::{Config, Credentials};
use crate::service::{GrantRefused, TlsRefused};

/// The key of the terminology table.
const KEY: &str = "eehrxf.terminology";

/// The `[eehrxf]` table as written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Eehrxf {
    /// The FHIR terminology server, when one is configured.
    pub terminology: Option<TerminologyServer>,
}

/// The `[eehrxf.terminology]` table as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TerminologyServer {
    /// The server's FHIR base URL, `https`; `http` under development only.
    pub url: SecretUrl,
    /// How the gateway authenticates to the server, when the transport does
    /// not.
    pub credentials: Option<Credentials>,
    /// The gateway's client certificate chain and private key, PEM, for
    /// mutual TLS with the server, inline or through `client_identity_file`.
    pub client_identity: Option<Secret>,
    /// A file holding the client identity, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A file of PEM trust roots the server's certificate chains to, beside
    /// the platform's.
    pub trust_roots_file: Option<PathBuf>,
    /// How long one operation may take, in milliseconds.
    pub timeout_ms: u64,
}

impl Default for TerminologyServer {
    fn default() -> Self {
        Self {
            url: SecretUrl::default(),
            credentials: None,
            client_identity: None,
            client_identity_file: None,
            trust_roots_file: None,
            timeout_ms: 2_000,
        }
    }
}

/// The terminology server, with every secret and file read.
#[derive(Debug)]
pub struct TerminologySettings {
    /// The server's FHIR base URL, already known to parse.
    pub url: SecretUrl,
    /// How the gateway authenticates to it: basic credentials.
    pub credentials: Option<Scheme>,
    /// The TLS material it is reached with.
    pub tls: TlsSettings,
    /// How long one operation may take.
    pub timeout: Duration,
}

/// Why the terminology server's client cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TerminologyFault {
    /// `[eehrxf.terminology]` is set and `[fhir]` is not, so nothing asks it.
    #[error("[eehrxf.terminology] serves the [fhir] face, and no [fhir] is set")]
    Unused,
    /// The credentials name a grant the server does not take.
    #[error(transparent)]
    Grant(#[from] GrantRefused),
    /// The TLS material does not read.
    #[error(transparent)]
    Tls(#[from] TlsRefused),
    /// The client cannot be built.
    #[error("the client of [eehrxf.terminology] cannot be built")]
    Client(#[source] FhirTerminologyError),
}

impl Config {
    /// Resolves `[eehrxf.terminology]`: a URL that parses, carries no
    /// userinfo and is `https` outside development, basic credentials, and a
    /// positive timeout.
    ///
    /// # Errors
    /// [`Error::Missing`] for no `url`, [`Error::Url`],
    /// [`Error::UrlCredentials`] and [`Error::Cleartext`] for the URL,
    /// [`Error::Zero`] for a zero timeout, [`TerminologyFault::Unused`] with
    /// no `[fhir]`, and the errors of a secret that cannot be read.
    pub(crate) fn resolve_terminology(&self) -> Result<Option<TerminologySettings>, Error> {
        let Some(table) = &self.eehrxf.terminology else {
            return Ok(None);
        };
        if self.fhir.is_none() {
            return Err(TerminologyFault::Unused.into());
        }
        let url_key = format!("{KEY}.url");
        if table.url.expose().is_empty() {
            return Err(Error::Missing { key: url_key });
        }
        let url = url::Url::parse(table.url.expose()).map_err(|source| Error::Url {
            key: url_key.clone(),
            source,
        })?;
        // NOTE: no specification governs this: our own design; as the registry refuses
        // it on an endpoint URL, a credential goes in its own section.
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Error::UrlCredentials {
                key: url_key,
                section: format!("{KEY}.credentials"),
            });
        }
        if table.timeout_ms == 0 {
            return Err(Error::Zero {
                key: format!("{KEY}.timeout_ms"),
            });
        }
        let section = format!("{KEY}.credentials");
        let credentials = table
            .credentials
            .as_ref()
            .map(|credentials| resolve_credentials(&section, credentials))
            .transpose()?;
        let payload = if credentials.is_some() {
            format!("{section} and the codes of received documents")
        } else {
            String::from("the codes of received documents")
        };
        transport::protected_payload(
            self.profile,
            table.url.expose(),
            ProtectedSite {
                url_key,
                payload,
                requires: Encryption::Https,
            },
        )?;
        let tls = crate::config::tls::resolve(
            KEY,
            table.client_identity.as_ref(),
            table.client_identity_file.as_deref(),
            table.trust_roots_file.as_ref(),
        )?;
        Ok(Some(TerminologySettings {
            url: table.url.clone(),
            credentials,
            tls,
            timeout: Duration::from_millis(table.timeout_ms),
        }))
    }
}

/// Builds the terminology `settings` describe: none without a server, or
/// the HTTP client of the server.
///
/// # Errors
/// The [`TerminologyFault`] that keeps the client from being built.
pub fn terminology(
    settings: Option<&TerminologySettings>,
) -> Result<Terminology, TerminologyFault> {
    let Some(settings) = settings else {
        return Ok(Terminology::none());
    };
    let tls = crate::service::tls_of(KEY, &settings.tls)?;
    let auth = crate::service::authentication(
        &format!("{KEY}.credentials"),
        settings.credentials.as_ref(),
    )?;
    let client = FhirTerminology::new(settings.url.expose(), &auth, &tls, settings.timeout)
        .map_err(TerminologyFault::Client)?;
    Ok(Terminology::server(Arc::new(client)))
}
