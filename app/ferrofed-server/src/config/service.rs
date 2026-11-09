// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credential the gateway reaches a service with.
//!
//! Every identity, localization, consent and audit service the gateway calls
//! takes its credential from [`authentication`], over
//! `ferrofed_identity::fhir`, so the mapping from the configuration is
//! written once (#507); its TLS material is the `TlsClient` its table's
//! `tls` sub-table read at load. A credential section names basic
//! credentials, and [`authentication`] refuses a grant, never reading it as
//! no credential. Three kinds of service take a grant: the IHE FHIR services
//! (PIXm, PDQm, PMIR, mCSD) the client credentials grant of IUA ITI-71, and
//! Mitz the same grant, which `ferrohealth-smart` obtains and
//! `service_authentication` hands to the client, and the NVI the Nuts grant,
//! which the Dutch binding wires itself. No specification governs the mapping: our own design.

use ferrofed_identity::fhir::Authentication;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrohealth_smart::client::ClientCredentials;

use crate::config::settings::Scheme;

/// A credential section that names a grant the service does not take.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{section} names an oauth or nuts grant this service does not take; give it basic credentials"
)]
pub struct GrantRefused {
    /// The credentials section, such as `pixm.manager[0].credentials`.
    pub section: String,
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
/// [`ServiceAuthError::Refused`] for an `oauth` or Nuts grant, and
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
