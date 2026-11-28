// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The revocation clearing of the outbound clients the gateway builds itself
//! from a `tls` table.
//!
//! A chain to a root of `trust_roots_file` opens a connection only once
//! `ferrohealth-server`'s revocation check cleared its endpoint, so each such
//! client holds one [`Clearing`] over the endpoints it reaches, started when
//! the client is built. The clients `ferrohealth-smart` builds keep their own.
//! A table's roots of `trust_roots_without_revocation_file` are named here
//! for the startup banner. No specification governs this: our own design.

use std::time::Duration;

use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_server::revocation::{Clearing, RevocationCheck};
use ferrohealth_server::tls_client::TlsClient;
use url::Url;

/// How long one endpoint's revocation check may take, as
/// `ferrohealth-server`'s telemetry clears its collector.
const CLEAR_WITHIN: Duration = Duration::from_secs(10);

/// Keeps `endpoints` cleared with `check` in a task of the current runtime,
/// or `None` outside one, where no connection is made.
#[must_use]
pub fn keep(check: &RevocationCheck, endpoints: Vec<Url>) -> Option<Clearing> {
    // NOTE: no specification governs this: our own design; with no runtime no
    // client sends, so a build outside one (a test, `config check`) starts nothing.
    tokio::runtime::Handle::try_current()
        .ok()
        .map(|_runtime| Clearing::spawn(check.clone(), endpoints, CLEAR_WITHIN))
}

/// The `https` URL a revocation check reaches the TLS endpoint `endpoint` by.
///
/// An endpoint of another scheme, such as a syslog repository's
/// `tls://host:port`, is reached at the same host and port, whose handshake
/// presents the same chain.
#[must_use]
pub fn handshake(endpoint: &Url) -> Option<Url> {
    if endpoint.scheme() == "https" {
        return Some(endpoint.clone());
    }
    let host = endpoint.host_str()?;
    let port = endpoint.port()?;
    // NOTE: no specification governs this: our own design; a host that is no
    // URL host has no handshake to check, and the client's own build refuses it.
    Url::parse(&format!("https://{host}:{port}/")).ok()
}

/// The banner line naming the roots the table `section` at `key` trusts
/// with no revocation check.
///
/// `None` when it names no `trust_roots_without_revocation_file`, or when
/// its material does not read, which the build refuses.
#[must_use]
pub fn unchecked_roots(key: &str, section: &TlsClientSection) -> Option<String> {
    section.trust_roots_without_revocation_file.as_ref()?;
    // NOTE: no specification governs this: our own design; material that does
    // not read is refused with its typed error by the build over the same table.
    TlsClient::read(key, section)
        .and_then(|client| client.platform_config())
        .ok()?
        .unchecked_roots()
}

/// The banner lines naming each table of the configuration `tree` that
/// trusts roots with no revocation check, in key order.
#[must_use]
pub fn unchecked_in(tree: &toml::Table) -> Vec<String> {
    let mut lines = Vec::new();
    walk("", tree, &mut lines);
    lines
}

/// Adds to `lines` the unchecked roots of `table` at `prefix` and of every
/// table under it.
fn walk(prefix: &str, table: &toml::Table, lines: &mut Vec<String>) {
    if table.contains_key("trust_roots_without_revocation_file") {
        // NOTE: no specification governs this: our own design; a table that does
        // not deserialize is refused with its typed error by the build over this tree.
        if let Ok(section) = toml::Value::Table(table.clone()).try_into::<TlsClientSection>()
            && let Some(line) = unchecked_roots(prefix, &section)
        {
            lines.push(line);
        }
        return;
    }
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            toml::Value::Table(inner) => walk(&path, inner, lines),
            toml::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    if let toml::Value::Table(inner) = item {
                        walk(&format!("{path}[{index}]"), inner, lines);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{handshake, unchecked_in};

    #[test]
    fn a_syslog_endpoint_is_checked_at_its_host_and_port() {
        let syslog = url::Url::parse("tls://audit.example.org:6514").expect("a URL");
        assert_eq!(
            Some("https://audit.example.org:6514/"),
            handshake(&syslog).as_ref().map(url::Url::as_str)
        );
        let https = url::Url::parse("https://node.example.org/ehr").expect("a URL");
        assert_eq!(Some(&https), handshake(&https).as_ref());
        let portless = url::Url::parse("tls://audit.example.org").expect("a URL");
        assert_eq!(None, handshake(&portless));
    }

    #[test]
    fn a_table_naming_no_unchecked_roots_puts_nothing_in_the_banner() {
        let tree: toml::Table = toml::from_str(
            "[pdqm.endpoint.tls]\ntrust_roots_file = \"/etc/roots.pem\"\n[[pixm.manager]]\nendpoint.url = \"https://pix.example.org/\"\n",
        )
        .expect("a table");
        assert_eq!(Vec::<String>::new(), unchecked_in(&tree));
    }
}
