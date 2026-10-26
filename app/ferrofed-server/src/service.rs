// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credential and the TLS material the gateway reaches a service with.
//!
//! Every identity, localization, consent and audit service the gateway calls
//! takes its credential from [`authentication`] and its TLS material from
//! [`tls`], both over `ferrofed_identity::fhir`, so the mapping from the
//! configuration is written once (#507). A credential section names basic
//! credentials, and [`authentication`] refuses a grant, never reading it as
//! no credential. Three kinds of service take a grant: the IHE FHIR services
//! (PIXm, PDQm, PMIR, mCSD) the client credentials grant of IUA ITI-71, and
//! Mitz the same grant, which `ferrohealth-smart` obtains and
//! `service_authentication` hands to the client, and the NVI the Nuts grant,
//! which the Dutch binding wires itself. No specification governs the mapping: our own design.

use ferrofed_identity::fhir::{Authentication, Tls, TlsError};
use ferrofed_registry::secret::Secret;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrohealth_smart::client::ClientCredentials;

use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;

/// A credential section that names a grant the service does not take.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{section} names an oauth, oauth2, nuts or fapi2 grant this service does not take; give it basic credentials"
)]
pub struct GrantRefused {
    /// The credentials section, such as `pixm.manager[0].credentials`.
    pub section: String,
}

/// TLS material that does not read.
#[derive(Debug, thiserror::Error)]
#[error("the TLS material of {key} does not read")]
pub struct TlsRefused {
    /// The table the material belongs to, such as `pdqm`.
    pub key: String,
    /// The part that does not read; it never carries the key.
    #[source]
    pub source: TlsError,
}

/// The credential the section `section` resolved to: none or basic
/// credentials.
///
/// # Errors
///
/// [`GrantRefused`] for any grant; an IHE FHIR service's grant is built by
/// `service_authentication`.
pub fn authentication(
    section: &str,
    scheme: Option<&Scheme>,
) -> Result<Authentication, GrantRefused> {
    match scheme {
        None => Ok(Authentication::None),
        Some(Scheme::Basic { user, password }) => Ok(Authentication::Basic {
            user: user.clone(),
            password: password.to_secret_string(),
        }),
        Some(_) => Err(GrantRefused {
            section: section.to_owned(),
        }),
    }
}

/// A credential of an IHE FHIR service or Mitz that cannot be used.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServiceAuthError {
    /// The section names a grant the service does not take.
    #[error(transparent)]
    Refused(#[from] GrantRefused),
    /// The HTTP client the grant's token requests are sent through could
    /// not be built.
    #[error("the HTTP client of the token endpoint of {section} could not be built")]
    TokenTransport {
        /// The credentials section.
        section: String,
        /// Why it could not be built; it names no secret.
        #[source]
        source: Box<ferrohealth_smart::client::TokenError>,
    },
}

/// The credential the section `section` of an IHE FHIR service or Mitz
/// resolved to.
///
/// It is none, basic credentials, or the token of its client credentials
/// grant (IUA ITI-71, RFC 6749 §4.4), which `ferrohealth-smart` obtains from
/// its token endpoint with the platform's trust roots.
///
/// # Errors
///
/// [`ServiceAuthError::Refused`] for an `oauth2`, Nuts or FAPI 2.0 grant, and
/// [`ServiceAuthError::TokenTransport`] for a token client that cannot be
/// built.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub fn service_authentication(
    section: &str,
    scheme: Option<&Scheme>,
) -> Result<Authentication, ServiceAuthError> {
    let Some(Scheme::Oauth { grant, .. }) = scheme else {
        return Ok(authentication(section, scheme)?);
    };
    let source =
        ferrohealth_smart::client::cache::TokenSource::new(ClientCredentials::clone(grant))
            .map_err(|source| ServiceAuthError::TokenTransport {
                section: section.to_owned(),
                source: Box::new(source),
            })?;
    Ok(Authentication::Grant(source))
}

/// The TLS material the table at `key` names: the client `identity` and the
/// trust `roots`, both PEM.
///
/// # Errors
///
/// [`TlsRefused`] for an identity or roots that do not read as PEM.
pub fn tls(key: &str, identity: Option<&Secret>, roots: Option<&str>) -> Result<Tls, TlsRefused> {
    let identity = identity.map(Secret::to_secret_string);
    Tls::from_pem(identity.as_ref(), roots).map_err(|source| TlsRefused {
        key: key.to_owned(),
        source,
    })
}

/// The TLS material `settings` hold for the table at `key`.
///
/// # Errors
///
/// [`TlsRefused`] for material that does not read as PEM.
pub fn tls_of(key: &str, settings: &TlsSettings) -> Result<Tls, TlsRefused> {
    tls(
        key,
        settings.client_identity.as_ref(),
        settings.trust_roots.as_deref(),
    )
}
