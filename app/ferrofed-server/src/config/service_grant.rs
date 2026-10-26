// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credentials of an IHE FHIR service, a PIX Manager, the PDQm
//! Supplier, the PMIR Registry and the mCSD directory, and of Mitz.
//!
//! `[<service>.credentials]` names basic credentials or `oauth`, the client
//! credentials grant of `ferrohealth-smart` (RFC 6749 §4.4), which IHE IUA
//! ITI-71 prescribes for a client acting as itself (§3.71.4.1.2.1). The
//! token is incorporated in each request as a bearer token (IUA ITI-72
//! §3.72.4.2).
//!
//! ```toml
//! [pixm.manager.credentials.oauth]
//! token_endpoint = "https://mpi.example.org/auth/oauth2_token"
//! client_id = "ferrofed-pix-consumer"
//! client_auth = "client_secret_basic"
//! client_secret_file = "/run/secrets/pix-consumer"
//! scope = "*"
//! ```
//!
//! No specification governs the shape of the table: our own design.

use ferrofed_identity::profile::Profile;

use crate::config::Credentials;
use crate::config::error::Error;
use crate::config::error::credentials::CredentialsFault;
use crate::config::oauth;
use crate::config::secrets::resolve_credentials;
use crate::config::settings::{Scheme, Settings};
use crate::config::transport::{self, Encryption, ProtectedSite};

/// What an identity service's credentials are resolved against: the
/// deployment profile, which decides whether a token endpoint may be plain
/// `http`.
#[derive(Debug, Clone, Copy)]
pub struct ServiceContext {
    /// The deployment profile.
    pub profile: Profile,
}

impl ServiceContext {
    /// The context `settings` give.
    #[must_use]
    pub fn of(settings: &Settings) -> Self {
        Self {
            profile: settings.profile,
        }
    }
}

/// The site of the token endpoint of the grant at `key`, sent the gateway's
/// client credentials.
#[must_use]
pub fn site(key: &str) -> ProtectedSite {
    ProtectedSite {
        url_key: format!("{key}.token_endpoint"),
        payload: format!("{key}, the gateway's client credentials"),
        requires: Encryption::Https,
    }
}

/// Returns the scheme an identity service's `credentials` at `section`
/// describe: basic credentials, or the client credentials grant of its
/// `oauth` table.
///
/// # Errors
///
/// The refusals of [`resolve_credentials`],
/// [`CredentialsFault::ServiceGrantNotHere`] for an `oauth2`, Nuts or FAPI
/// 2.0 grant, and [`Error::Cleartext`] for a token endpoint that is not
/// `https` outside the development profile.
pub(crate) fn resolve_service(
    section: &str,
    credentials: &Credentials,
    context: ServiceContext,
) -> Result<Scheme, Error> {
    let refused = || {
        Error::Credentials(CredentialsFault::ServiceGrantNotHere {
            section: section.to_owned(),
        })
    };
    if credentials.oauth2.is_some() || credentials.fapi2.is_some() {
        return Err(refused());
    }
    let scheme = resolve_credentials(section, credentials)?;
    match &scheme {
        Scheme::Basic { .. } => Ok(scheme),
        Scheme::Oauth { grant, .. } => {
            // NOTE: RFC 6749 §2.3.1 requires TLS for a client password, and an assertion
            // or a token is no less a credential, so cleartext is for development alone.
            if let Some(endpoint) = oauth::token_endpoint(grant) {
                transport::protected_payload(
                    context.profile,
                    endpoint.as_str(),
                    site(&format!("{section}.oauth")),
                )?;
            }
            Ok(scheme)
        }
        _ => Err(refused()),
    }
}
