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

use ferrohealth_smart::config::endpoint::EndpointSection;
use url::Url;

use crate::config::error::Error;

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

/// Whether the endpoint table sends a credential: HTTP Basic or a client
/// credentials grant.
#[must_use]
pub fn credentialed(endpoint: &EndpointSection) -> bool {
    !endpoint.credentials.user.is_empty() || endpoint.credentials.oauth.is_some()
}
