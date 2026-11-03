// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the configuration reads from a PostgreSQL connection string: whether
//! it parses, and whether it sends a password over a network without TLS.
//! No specification governs this: our own design.

use std::str::FromStr;

use ferrohealth_secret::secret_url::SecretUrl;
use tokio_postgres::Config;
use tokio_postgres::config::{Host, SslMode};

/// Whether `url` parses as a PostgreSQL connection string, a URL or libpq
/// key/value pairs.
#[must_use]
pub fn parses(url: &SecretUrl) -> bool {
    // NOTE: no specification governs this: our own design; a parse failure is
    // the answer here, and its message may quote the secret.
    Config::from_str(url.expose()).is_ok()
}

/// Whether the connection `url` names sends a password over a network
/// without requiring TLS.
///
/// The driver reads `sslmode` as `disable`, `prefer` (its default) or
/// `require`, and refuses any other value, so only `require` encrypts for
/// certain; the connector then verifies the server against the platform's
/// roots. A connection whose every host is a Unix socket, with no `hostaddr`,
/// crosses no network.
/// A string that does not parse cannot be shown to require TLS, so it counts
/// as exposing its password.
#[must_use]
pub fn exposes_password(url: &SecretUrl) -> bool {
    let Ok(config) = Config::from_str(url.expose()) else {
        return true;
    };
    let networked = !config.get_hostaddrs().is_empty()
        || config
            .get_hosts()
            .iter()
            .any(|host| !matches!(host, Host::Unix(_)));
    config.get_password().is_some()
        && networked
        && !matches!(config.get_ssl_mode(), SslMode::Require)
}

#[cfg(test)]
mod tests {
    use ferrohealth_secret::secret_url::SecretUrl;

    use super::parses;

    #[test]
    fn a_url_and_libpq_pairs_parse_and_garbage_does_not() {
        for parsed in [
            "postgres://ferrofed:secret@db.example.org:5432/ferrofed?sslmode=require",
            "host=db.example.org user=ferrofed password=secret dbname=ferrofed",
        ] {
            assert!(parses(&SecretUrl::new(parsed)), "{parsed}");
        }
        for refused in ["postgres://a:b@host:notaport/db", "host='unterminated"] {
            assert!(!parses(&SecretUrl::new(refused)), "{refused}");
        }
    }
}
