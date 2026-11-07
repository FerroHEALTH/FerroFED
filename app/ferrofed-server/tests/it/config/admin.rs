// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[metrics]` and the admin listener's authentication: outside the
//! development profile, a listener off loopback is refused unless a client
//! named in `scrape_clients`, whose client-credentials token a trusted issuer
//! signs, or a client CA authenticates the scrape; and `[[auth.basic.users]]`
//! is refused, naming the user. No specification governs the admin listener:
//! our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::error::server::ServerFault;
use ferrofed_server::config::settings::Settings;
use ferrofed_testkit::listener::ListenerCertificates;

use super::refusal;
use crate::support::caller::{SCRAPE_CLIENT, auth_toml};

type TestResult = Result<(), Box<dyn StdError>>;

/// The addresses off loopback a listener may name.
const REMOTE: [&str; 3] = ["0.0.0.0:9464", "192.0.2.10:9464", "[::]:9464"];

/// The `[metrics]` table that serves `listen` with remote serving allowed,
/// with `extra` keys appended.
fn remote(listen: &str, extra: &str) -> String {
    format!("[metrics]\nlisten = \"{listen}\"\nallow_remote = true\n{extra}")
}

/// The `[metrics]` keys that name [`SCRAPE_CLIENT`] as the scrape's client.
fn scraping() -> String {
    format!("scrape_clients = [\"{SCRAPE_CLIENT}\"]\n")
}

/// Resolves `text` with no environment.
fn resolved(text: &str) -> Result<Settings, Box<dyn StdError>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?)
}

/// A TOML string holding `path`.
fn quoted(path: &std::path::Path) -> String {
    toml::Value::String(path.display().to_string()).to_string()
}

#[test]
fn production_refuses_a_remote_listener_nothing_authenticates() -> TestResult {
    for listen in REMOTE {
        let refused = refusal(&remote(listen, ""))?;
        assert!(
            matches!(refused, Error::Server(ServerFault::MetricsUnauthenticated { address }) if address == listen.parse()?),
            "{listen}: {refused}"
        );
        let shown = refused.to_string();
        assert!(
            shown.contains("metrics.scrape_clients")
                && shown.contains("metrics.tls.client_ca_file"),
            "the refusal names what authenticates the scrape: {shown}"
        );
    }
    Ok(())
}

#[test]
fn a_scrape_client_admits_a_remote_listener() -> TestResult {
    for listen in REMOTE {
        let text = format!("{}{}", remote(listen, &scraping()), auth_toml());
        let settings = resolved(&text)?;
        assert_eq!(Some(listen.parse()?), settings.metrics.listen);
        assert_eq!(
            vec![SCRAPE_CLIENT.to_owned()],
            settings.metrics.scrape_clients
        );
    }
    Ok(())
}

#[test]
fn a_scrape_client_with_no_issuer_to_sign_its_token_is_refused() -> TestResult {
    let refused = refusal(&remote("127.0.0.1:9464", &scraping()))?;
    assert!(
        matches!(&refused, Error::Server(ServerFault::ScrapeIssuer)),
        "{refused}"
    );
    Ok(())
}

// RFC 6749 §4.4: a machine authenticates as a client, so a Basic user is refused
// and the refusal names it; one granted a `system/` scope is refused by the crate
// first (`auth::config`).
#[test]
fn a_basic_user_is_refused_naming_the_user() -> TestResult {
    {
        let text = format!(
            "{}{}\n[[auth.basic.users]]\nuser = \"synthetic-basic-machine\"\npassword_hash = \"$argon2id$v=19$m=19456,t=2,p=1$c3ludGhldGljLXNhbHQtMTY$4es9c3W3V1fKeaFNam0B7dB/9kjZXP6/+9VB3yhq8ZA\"\n",
            remote("127.0.0.1:9464", &scraping()),
            auth_toml()
        );
        let refused = refusal(&text)?;
        assert!(
            matches!(&refused, Error::Auth { key, .. } if key.contains("synthetic-basic-machine")),
            "{refused}"
        );
        assert!(
            refused.to_string().contains("metrics.scrape_clients"),
            "the refusal names the replacement: {refused}"
        );
    }
    Ok(())
}

#[test]
fn a_client_ca_admits_a_remote_listener() -> TestResult {
    let dir = tempfile::tempdir()?;
    let files = ListenerCertificates::generate()?.write(dir.path())?;
    let tls = format!(
        "\n[metrics.tls]\ncertificate_file = {}\nkey_file = {}\n",
        quoted(&files.certificate),
        quoted(&files.key),
    );
    let without_ca = refusal(&remote("0.0.0.0:9464", &tls))?;
    assert!(
        matches!(
            without_ca,
            Error::Server(ServerFault::MetricsUnauthenticated { .. })
        ),
        "TLS without a client CA authenticates no one: {without_ca}"
    );
    let mutual = format!(
        "{tls}client_auth = \"required\"\nclient_ca_file = {}\n",
        quoted(&files.client_ca)
    );
    let settings = resolved(&remote("0.0.0.0:9464", &mutual))?;
    assert_eq!(Some("0.0.0.0:9464".parse()?), settings.metrics.listen);
    Ok(())
}

#[test]
fn development_and_loopback_need_no_scrape_authentication() -> TestResult {
    for listen in REMOTE {
        let settings = resolved(&format!(
            "profile = \"development\"\n\n{}",
            remote(listen, "")
        ))?;
        assert_eq!(Some(listen.parse()?), settings.metrics.listen);
        assert_eq!(settings.metrics.scrape_clients, Vec::<String>::new());
    }
    for loopback in ["127.0.0.1:9464", "[::1]:9464"] {
        let settings = resolved(&format!("[metrics]\nlisten = \"{loopback}\"\n"))?;
        assert_eq!(Some(loopback.parse()?), settings.metrics.listen);
    }
    Ok(())
}
