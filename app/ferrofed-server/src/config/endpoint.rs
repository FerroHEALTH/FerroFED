// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `endpoint` table every outbound service of the gateway is reached
//! through: `ferrohealth-smart`'s `EndpointSection`, read the one way.
//!
//! [`checked`] holds a table to the gateway's rules before
//! `ferrohealth-smart` validates the rest: a URL that is set and parses and
//! carries no userinfo, its credential in its own section, and a timeout
//! that is not zero, each refusal naming the full dotted key. No
//! specification governs the table: the family's design.

#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrohealth_smart::config::credentials::CredentialsSection;
use ferrohealth_smart::config::endpoint::EndpointSection;
use url::Url;

use crate::config::error::Error;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use crate::config::service_grant::{ServiceContext, resolve_service};
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use crate::config::settings::Scheme;

/// The URL of the endpoint table at `key`, once the table passes the
/// gateway's rules and `ferrohealth-smart`'s own.
///
/// # Errors
///
/// [`Error::Missing`] for no `url`, [`Error::Url`] for one that does not
/// parse, [`Error::UrlCredentials`] for one with userinfo, [`Error::Zero`]
/// for a zero `timeout`, and [`Error::Smart`] for a rule of the crate's.
pub fn checked(key: &str, endpoint: &EndpointSection) -> Result<Url, Error> {
    let url_key = format!("{key}.url");
    if endpoint.url.is_empty() {
        return Err(Error::Missing { key: url_key });
    }
    let url = Url::parse(endpoint.url.expose()).map_err(|source| Error::Url {
        key: url_key.clone(),
        source,
    })?;
    // NOTE: no specification governs this: our own design; as the registry refuses it on an
    // endpoint URL, a credential goes in its own section.
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::UrlCredentials {
            key: url_key,
            section: format!("{key}.credentials"),
        });
    }
    if endpoint.timeout.is_zero() {
        return Err(Error::Zero {
            key: format!("{key}.timeout"),
        });
    }
    endpoint.validate().map_err(|source| Error::Smart {
        section: key.to_owned(),
        source: Box::new(source),
    })
}

/// The default endpoint table of a service a query asks on its way.
///
/// Each call is tried once within `timeout`, so a failure fails the step at
/// once within the query's own budget, as it did before the table moved
/// under `endpoint`. No specification governs the default: our own design.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
#[must_use]
pub fn tried_once(timeout: ferrohealth_config::units::Duration) -> EndpointSection {
    EndpointSection {
        timeout,
        retry: ferrohealth_smart::config::endpoint::RetrySection {
            max_attempts: 1,
            ..ferrohealth_smart::config::endpoint::RetrySection::default()
        },
        ..EndpointSection::default()
    }
}

/// Whether the endpoint table sends a credential: HTTP Basic or a client
/// credentials grant.
#[must_use]
pub fn credentialed(endpoint: &EndpointSection) -> bool {
    !endpoint.credentials.user.is_empty() || endpoint.credentials.oauth.is_some()
}

/// An identity, localization or consent service's endpoint table, resolved:
/// its URL and the credential its `credentials` name.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
#[derive(Debug)]
pub struct ServiceEndpoint {
    /// The service's URL, known to parse with no userinfo.
    pub url: Url,
    /// Basic credentials or the client credentials grant of `oauth`, when
    /// the table names one.
    pub credentials: Option<Scheme>,
    /// The TLS material its `tls` names, every file read once at load, which
    /// a reload compares so a file renewed at the same path is a change.
    pub tls: std::sync::Arc<ferrohealth_server::tls_client::TlsClient>,
}

/// Resolves the endpoint table at `key` of a service the gateway calls as
/// itself.
///
/// The table passes [`checked`], its credential is one [`resolve_service`]
/// admits with its token endpoint held to `https` outside development, and
/// its TLS material is read once at load, so a file that cannot be read
/// stops `config check`.
///
/// # Errors
///
/// The errors of [`checked`], of [`resolve_service`], and of a TLS file
/// that cannot be read.
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub(crate) fn service(
    key: &str,
    endpoint: &EndpointSection,
    context: ServiceContext,
) -> Result<ServiceEndpoint, Error> {
    let url = checked(key, endpoint)?;
    let section = format!("{key}.credentials");
    let credentials = (endpoint.credentials != CredentialsSection::default())
        .then(|| resolve_service(&section, &endpoint.credentials, context))
        .transpose()?;
    let tls = crate::config::tls::read(&format!("{key}.tls"), &endpoint.tls)?;
    Ok(ServiceEndpoint {
        url,
        credentials,
        tls,
    })
}
