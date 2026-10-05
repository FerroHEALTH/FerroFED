// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The credential and the TLS material the gateway reaches a service with.
//!
//! Every identity, localization, consent and audit service the gateway calls
//! takes its credential from [`authentication`] and its TLS material from
//! [`tls`], both over `ferrofed_identity::fhir`, so the mapping from the
//! configuration is written once (#507). A credential section names a bearer
//! token or basic credentials; an OAuth 2.0, Nuts or FAPI 2.0 grant is
//! refused here, never read as no credential. The NVI's Nuts grant is the
//! one grant a service takes, and the Dutch binding wires it itself. No
//! specification governs the mapping: our own design.

use ferrofed_identity::fhir::{Authentication, Tls, TlsError};
use ferrofed_registry::secret::Secret;

use crate::config::settings::Scheme;
use crate::config::tls::TlsSettings;

/// A credential section that names a grant, which only a node takes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{section} names an oauth2, nuts or fapi2 grant, which only a node takes; give it a bearer token or basic credentials"
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

/// The credential the section `section` resolved to: none, a bearer token
/// or basic credentials.
///
/// # Errors
///
/// [`GrantRefused`] for an OAuth 2.0, Nuts or FAPI 2.0 grant.
pub fn authentication(
    section: &str,
    scheme: Option<&Scheme>,
) -> Result<Authentication, GrantRefused> {
    match scheme {
        None => Ok(Authentication::None),
        Some(Scheme::Bearer(token)) => Ok(Authentication::Bearer(token.to_secret_string())),
        Some(Scheme::Basic { user, password }) => Ok(Authentication::Basic {
            user: user.clone(),
            password: password.to_secret_string(),
        }),
        Some(Scheme::OAuth2(_) | Scheme::Fapi2(_) | Scheme::Binding(_)) => Err(GrantRefused {
            section: section.to_owned(),
        }),
    }
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
