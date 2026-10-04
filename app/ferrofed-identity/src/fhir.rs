// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The HTTP client the gateway asks an IHE FHIR server through.
//!
//! One build serves every IHE FHIR server the gateway calls: a PIX Manager
//! (PIXm ITI-83), a Patient Identity Registry (PMIR ITI-94) and a care
//! services directory (mCSD ITI-90 and ITI-91). [`http_client`] sends the
//! [`Authentication`] as a sensitive default header, composed as the node
//! client composes it, presents and trusts the [`Tls`] material, and follows
//! no redirect: a request can carry a patient identifier and always carries
//! the credential, and neither goes anywhere the configured base does not
//! name.

use std::fmt;

use http::header::{AUTHORIZATION, HeaderMap};
use openehr_its::rest::client::{Credentials, InvalidCredentials};
use secrecy::{ExposeSecret, SecretString};
use thiserror::Error;

/// How the gateway authenticates to an IHE FHIR server (ITI TF-2 Appendix
/// Z.8).
///
/// `Debug` redacts every secret, because [`SecretString`] does.
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
}

impl Authentication {
    /// The credentials the `Authorization` header carries, or `None` when
    /// the transport authenticates the gateway.
    fn credentials(&self) -> Option<Credentials> {
        match self {
            Self::None => None,
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
    /// The HTTP client could not be built.
    #[error("the HTTP client for an IHE FHIR server could not be built")]
    Build(#[source] reqwest::Error),
}

/// Builds the HTTP client an IHE FHIR server is asked through: `auth` as a
/// sensitive default `Authorization` header, the `tls` material, and no
/// redirects.
///
/// # Errors
/// A [`ClientError`] for a credential that forms no `Authorization` value,
/// or a client that cannot be built.
pub fn http_client(auth: &Authentication, tls: &Tls) -> Result<reqwest::Client, ClientError> {
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
    builder.build().map_err(ClientError::Build)
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
