// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[eehrxf]` table: the interoperability component's own settings.
//!
//! `[eehrxf.terminology]` names the FHIR terminology server the component
//! asks for `$translate`, `$lookup` and `$validate-code`
//! (<https://hl7.org/fhir/R4/terminology-service.html>):
//!
//! ```toml
//! [eehrxf.terminology.endpoint]
//! url = "https://terminology.example.org/fhir/"
//! timeout = "2s"
//!
//! [eehrxf.terminology.endpoint.credentials]
//! user = "ferrofed"
//! password_file = "/run/secrets/terminology-password"
//! ```
//!
//! The endpoint is `ferrohealth-smart`'s outbound table: its `tls` gives
//! the mutual TLS the server asks for, its `credentials` HTTP Basic or a
//! client credentials grant, and its `retry` the tries of one call, once by
//! default, since the face that asks fails closed on the first refusal. The
//! server is sent the codes of the documents the face receives, which are
//! health data, so its URL is held to the protected-payload policy of
//! [`transport`]. Its `transcode = "myhealth-eu"` also translates each
//! summary's codes into the systems the MyHealth@EU Core required bindings
//! take ([`Transcode`]); the default, `none`, translates only into the
//! eHealth Network's preferred systems. The table is optional: with none,
//! the component fails closed where it needs a server, refusing a received
//! coding that carries no display, and names every `required` binding no
//! vendored expansion decides as not evaluated. It serves the `[fhir]`
//! face, so it is refused without one. No specification governs the shape
//! of the table: our own design.

use ferrohealth_config::units;
use ferrohealth_smart::config::endpoint::{EndpointSection, RetrySection};
use std::sync::Arc;

use eehrxf::terminology::Terminology;
use eehrxf::terminology::translation::{Binding, MYHEALTH_EU_CORE};
use ferrobridge_term::error::Error as TermError;
use ferrofed_eehrxf::terminology::BridgeTerminology;
use serde::Deserialize;

use crate::config::Config;
use crate::config::error::Error;
use crate::config::transport::{self, Encryption, ProtectedSite};

/// The key of its endpoint table.
const ENDPOINT: &str = "eehrxf.terminology.endpoint";

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
    /// The server's FHIR base and how it is reached: `https`, `http` under
    /// development only.
    pub endpoint: EndpointSection,
    /// The codings each summary the face writes gains through the server
    /// beside the recorded ones.
    pub transcode: Transcode,
}

/// The codings a summary gains through the terminology server, as
/// `[eehrxf.terminology] transcode` names them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transcode {
    /// Only the eHealth Network's preferred systems for a code recorded in
    /// none of them (eHN PS Art 10).
    #[default]
    None,
    /// Also the code systems of the MyHealth@EU Core required bindings that
    /// take no SNOMED CT code, the ICD-10 problem and the ATC allergen, as
    /// the Master Value Sets Catalogue draws them, for a deployment that is
    /// a national contact point's infrastructure (Implementing Regulation
    /// (EU) 2026/2083 Art 3(4)).
    MyhealthEu,
}

impl Transcode {
    /// The required bindings each summary's codes are translated to meet.
    #[must_use]
    pub const fn bindings(self) -> &'static [Binding] {
        match self {
            Self::None => &[],
            Self::MyhealthEu => &MYHEALTH_EU_CORE,
        }
    }
}

impl Default for TerminologyServer {
    fn default() -> Self {
        Self {
            // NOTE: no specification governs this: our own design; the face that asks
            // fails closed on the first refusal, so one call is tried once by default.
            endpoint: EndpointSection {
                timeout: units::Duration::from_secs(2),
                retry: RetrySection {
                    max_attempts: 1,
                    ..RetrySection::default()
                },
                ..EndpointSection::default()
            },
            transcode: Transcode::None,
        }
    }
}

/// The terminology server, checked.
#[derive(Debug)]
pub struct TerminologySettings {
    /// How the server is reached, its URL known to parse and held to the
    /// protected-payload policy.
    pub endpoint: EndpointSection,
    /// The codings each summary gains through the server.
    pub transcode: Transcode,
}

/// Why the terminology server's client cannot be built.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TerminologyFault {
    /// `[eehrxf.terminology]` is set and `[fhir]` is not, so nothing asks it.
    #[error("[eehrxf.terminology] serves the [fhir] face, and no [fhir] is set")]
    Unused,
    /// The client cannot be built.
    #[error("the client of [eehrxf.terminology] cannot be built")]
    Client(#[source] Box<TermError>),
}

impl Config {
    /// Resolves `[eehrxf.terminology]`: an endpoint table
    /// `ferrohealth-smart` accepts, whose URL carries no userinfo and is
    /// `https` outside development.
    ///
    /// # Errors
    /// The errors of [`crate::config::endpoint::checked`],
    /// [`Error::Cleartext`] for the URL, and [`TerminologyFault::Unused`]
    /// with no `[fhir]`.
    pub(crate) fn resolve_terminology(&self) -> Result<Option<TerminologySettings>, Error> {
        let Some(table) = &self.eehrxf.terminology else {
            return Ok(None);
        };
        if self.fhir.is_none() {
            return Err(TerminologyFault::Unused.into());
        }
        let url = crate::config::endpoint::checked(ENDPOINT, &table.endpoint)?;
        let url_key = format!("{ENDPOINT}.url");
        let section = format!("{ENDPOINT}.credentials");
        let payload = if crate::config::endpoint::credentialed(&table.endpoint) {
            format!("{section} and the codes of received documents")
        } else {
            String::from("the codes of received documents")
        };
        transport::protected_payload(
            self.profile,
            url.as_str(),
            ProtectedSite {
                url_key,
                payload,
                requires: Encryption::Https,
            },
        )?;
        Ok(Some(TerminologySettings {
            endpoint: table.endpoint.clone(),
            transcode: table.transcode,
        }))
    }
}

/// The terminology of [`terminology`]: the component's seam, the client the
/// validator of a received document asks, and the bindings each summary's
/// codes are translated to meet.
pub type Built = (Terminology, Option<BridgeTerminology>, &'static [Binding]);

/// Builds the terminology `settings` describe.
///
/// None without a server, or the HTTP client of the server, as the
/// component's seam and as the client the validator of a received document
/// asks, with the bindings its `transcode` names.
///
/// # Errors
/// The [`TerminologyFault`] that keeps the client from being built.
pub fn terminology(settings: Option<&TerminologySettings>) -> Result<Built, TerminologyFault> {
    let Some(settings) = settings else {
        return Ok((Terminology::none(), None, &[]));
    };
    let client = BridgeTerminology::from_endpoint(&settings.endpoint, ENDPOINT)
        .map_err(|error| TerminologyFault::Client(Box::new(error)))?;
    Ok((
        Terminology::server(Arc::new(client.clone())),
        Some(client),
        settings.transcode.bindings(),
    ))
}
