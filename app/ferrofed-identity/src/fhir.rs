// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP client the gateway asks an identity, localization, consent or
//! audit service through.
//!
//! One build and one TLS type serve every outbound client of this crate: a
//! PIX Manager (PIXm ITI-83), a Patient Demographics Supplier (PDQm ITI-78
//! and ITI-119), a Patient Identity Registry (PMIR ITI-94), a care services
//! directory (mCSD ITI-90 and ITI-91), an XCPD responding gateway (ITI-55),
//! the NVI Localization Service and Mitz of the Dutch Generic Functions, and
//! the FHIR Feed audit repository (ITI-20). [`http_client`] sends the
//! [`Authentication`] as a sensitive default header, composed as the node
//! client composes it, presents and trusts the [`Tls`] material, and follows
//! no redirect: a request can carry a patient identifier and always carries
//! the credential, and neither goes anywhere the configured base does not
//! name. [`http_client_builder`] is the same build left open, for a client
//! that finishes it itself. An OAuth 2.0 grant ([`Authentication::Grant`])
//! rides in no default header: the PIXm, PDQm, PMIR and mCSD clients
//! incorporate its token in each request themselves (IUA ITI-72), and every
//! other client refuses it.

use std::fmt;
use std::sync::Arc;

use http::header::{AUTHORIZATION, HeaderMap};
use openehr_its::rest::client::{Credentials, CredentialsProvider, InvalidCredentials};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// How the gateway authenticates to an IHE FHIR server (ITI TF-2 Appendix
/// Z.8).
///
/// `Debug` redacts every secret, because [`SecretString`] does, and shows a
/// grant by its provider's own `Debug`, which names no token.
#[derive(Debug)]
#[non_exhaustive]
pub enum Authentication {
    /// No `Authorization` header: the transport (mutual TLS, a private
    /// network) authenticates the gateway.
    None,
    /// An RFC 6750 bearer token.
    Bearer(SecretString),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
    /// An OAuth 2.0 access token the provider obtains and refreshes,
    /// incorporated in each request (IUA ITI-71, ITI-72 §3.72.4.2). Only a
    /// client that takes an authorizer sends it; [`http_client`] refuses it
    /// ([`ClientError::Grant`]).
    Grant(Arc<dyn CredentialsProvider>),
}

impl Authentication {
    /// The credentials the `Authorization` header carries, or `None` when
    /// the transport authenticates the gateway.
    fn credentials(&self) -> Option<Credentials> {
        match self {
            Self::None | Self::Grant(_) => None,
            Self::Bearer(token) => Some(Credentials::bearer(token.clone())),
            Self::Basic { user, password } => {
                Some(Credentials::basic(user.as_str(), password.clone()))
            }
        }
    }
}

/// The TLS material the gateway presents to an IHE FHIR server and trusts
/// it by, read once.
///
/// The default presents no client certificate and trusts the platform's
/// roots alone. `Debug` shows only which parts are present.
#[derive(Clone, Default)]
pub struct Tls {
    identity: Option<reqwest::Identity>,
    roots: Vec<reqwest::Certificate>,
}

impl Tls {
    /// Reads `identity`, the gateway's client certificate chain and private
    /// key in PEM for mutual TLS, and `roots`, a PEM bundle of trust roots
    /// beside the platform's.
    ///
    /// # Errors
    /// A [`TlsError`] naming the part that does not read as PEM; it never
    /// carries the key.
    pub fn from_pem(
        identity: Option<&SecretString>,
        roots: Option<&str>,
    ) -> Result<Self, TlsError> {
        let identity = identity
            .map(|pem| reqwest::Identity::from_pem(pem.expose_secret().as_bytes()))
            .transpose()
            .map_err(TlsError::Identity)?;
        let roots = roots
            .map(|pem| reqwest::Certificate::from_pem_bundle(pem.as_bytes()))
            .transpose()
            .map_err(TlsError::Roots)?
            .unwrap_or_default();
        Ok(Self { identity, roots })
    }
}

impl fmt::Debug for Tls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tls")
            .field("identity", &self.identity.is_some())
            .field("roots", &self.roots.len())
            .finish()
    }
}

/// TLS material that does not read.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TlsError {
    /// The client certificate chain and key do not read as PEM.
    #[error("the client certificate and key for an IHE FHIR server do not read as PEM")]
    Identity(#[source] reqwest::Error),
    /// The trust roots do not read as a PEM bundle.
    #[error("the trust roots for an IHE FHIR server do not read as a PEM bundle")]
    Roots(#[source] reqwest::Error),
}

/// An HTTP client for an IHE FHIR server that cannot be built.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ClientError {
    /// A credential does not form an `Authorization` value (RFC 7617 §2,
    /// RFC 6750 §2.1).
    #[error("the credentials of an IHE FHIR server cannot be sent in the Authorization header")]
    Credentials(#[source] InvalidCredentials),
    /// The credential is a grant, whose token changes from request to
    /// request and so rides in no default header: the client that sends
    /// it takes an authorizer.
    #[error(
        "an OAuth 2.0 grant cannot be a default header of the HTTP client for an IHE FHIR server"
    )]
    Grant,
    /// The HTTP client could not be built.
    #[error("the HTTP client for an IHE FHIR server could not be built")]
    Build(#[source] reqwest::Error),
}

/// Builds the HTTP client a service is asked through: `auth` as a sensitive
/// default `Authorization` header, the `tls` material, and no redirects.
///
/// # Errors
/// A [`ClientError`] for a credential that forms no `Authorization` value or
/// is a grant, or a client that cannot be built.
pub fn http_client(auth: &Authentication, tls: &Tls) -> Result<reqwest::Client, ClientError> {
    http_client_builder(auth, tls)?
        .build()
        .map_err(ClientError::Build)
}

/// Returns the builder [`http_client`] finishes: `auth` as a sensitive
/// default `Authorization` header, the `tls` material, and no redirects.
///
/// # Errors
/// [`ClientError::Credentials`] for a credential that forms no
/// `Authorization` value, and [`ClientError::Grant`] for a grant.
pub fn http_client_builder(
    auth: &Authentication,
    tls: &Tls,
) -> Result<reqwest::ClientBuilder, ClientError> {
    if matches!(auth, Authentication::Grant(_)) {
        return Err(ClientError::Grant);
    }
    let mut headers = HeaderMap::new();
    if let Some(credentials) = auth.credentials() {
        let header = credentials
            .header_value()
            .map_err(ClientError::Credentials)?;
        headers.insert(AUTHORIZATION, header);
    }
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .default_headers(headers)
        .tls_certs_merge(tls.roots.iter().cloned());
    if let Some(identity) = &tls.identity {
        builder = builder.identity(identity.clone());
    }
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use secrecy::SecretString;

    use super::{Authentication, Tls, TlsError, http_client};

    #[test]
    fn material_that_does_not_read_names_its_part() {
        let key = SecretString::from("not a PEM key");
        assert!(matches!(
            Tls::from_pem(Some(&key), None),
            Err(TlsError::Identity(_))
        ));
        assert!(matches!(
            Tls::from_pem(None, Some("-----BEGIN CERTIFICATE-----\nQz7\n")),
            Err(TlsError::Roots(_))
        ));
    }

    #[test]
    fn no_material_builds_the_client_the_platform_roots_serve() {
        let tls = Tls::from_pem(None, None).expect("no material reads");
        assert_eq!("Tls { identity: false, roots: 0 }", format!("{tls:?}"));
        http_client(&Authentication::None, &tls).expect("the client builds");
    }
}
