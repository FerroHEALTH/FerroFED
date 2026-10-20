// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[metrics]` and the admin listener's authentication: outside the
//! development profile, a listener off loopback is refused unless a Basic
//! user of `[[auth.basic.users]]` named in `scrape_users` or a client CA
//! authenticates the scrape; a user's password hash is a secret with a
//! `_file` sibling, never shown. No specification governs the admin
//! listener: our own design.
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

use super::{everything, refusal, secret_file};
use crate::support::{SCRAPE_HASH, SCRAPE_USER};

type TestResult = Result<(), Box<dyn StdError>>;

/// The addresses off loopback a listener may name.
const REMOTE: [&str; 3] = ["0.0.0.0:9464", "192.0.2.10:9464", "[::]:9464"];

/// The `[metrics]` table that serves `listen` with remote serving allowed,
/// with `extra` keys appended.
fn remote(listen: &str, extra: &str) -> String {
    format!("[metrics]\nlisten = \"{listen}\"\nallow_remote = true\n{extra}")
}

/// The `[metrics]` keys that name [`SCRAPE_USER`] as the scrape's user.
fn scraping() -> String {
    format!("scrape_users = [\"{SCRAPE_USER}\"]\n")
}

/// `[auth]` writing [`SCRAPE_USER`] as a Basic user with `password` keys.
fn basic_user(password: &str) -> String {
    format!("\n[auth]\n\n[[auth.basic.users]]\nuser = \"{SCRAPE_USER}\"\n{password}")
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
            shown.contains("metrics.scrape_users") && shown.contains("metrics.tls.client_ca_file"),
            "the refusal names what authenticates the scrape: {shown}"
        );
    }
    Ok(())
}

#[test]
fn a_scrape_user_admits_a_remote_listener_and_its_hash_is_never_shown() -> TestResult {
    for listen in REMOTE {
        let text = format!(
            "{}{}",
            remote(listen, &scraping()),
            basic_user(&format!("password_hash = \"{SCRAPE_HASH}\"\n"))
        );
        let settings = resolved(&text)?;
        assert_eq!(Some(listen.parse()?), settings.metrics.listen);
        assert_eq!(vec![SCRAPE_USER.to_owned()], settings.metrics.scrape_users);
        assert!(!format!("{settings:?}").contains(SCRAPE_HASH));
    }
    Ok(())
}

#[test]
fn a_scrape_user_no_basic_user_writes_is_refused() -> TestResult {
    let refused = refusal(&remote("127.0.0.1:9464", &scraping()))?;
    assert!(
        matches!(&refused, Error::Server(ServerFault::ScrapeUser { user }) if user == SCRAPE_USER),
        "{refused}"
    );
    Ok(())
}

#[test]
fn the_password_hash_is_read_from_its_file() -> TestResult {
    let file = secret_file(&format!("{SCRAPE_HASH}\n"))?;
    let text = format!(
        "{}{}",
        remote("0.0.0.0:9464", &scraping()),
        basic_user(&format!("password_hash_file = {}\n", quoted(file.path())))
    );
    let settings = resolved(&text)?;
    assert!(
        settings
            .server
            .auth
            .basic
            .as_ref()
            .is_some_and(|basic| basic.names(SCRAPE_USER))
    );
    Ok(())
}

#[test]
fn the_password_hash_and_its_file_together_are_refused() -> TestResult {
    let file = secret_file(SCRAPE_HASH)?;
    let text = format!(
        "{}{}",
        remote("127.0.0.1:9464", &scraping()),
        basic_user(&format!(
            "password_hash = \"{SCRAPE_HASH}\"\npassword_hash_file = {}\n",
            quoted(file.path())
        ))
    );
    let refused = refusal(&text)?;
    assert!(
        matches!(&refused, Error::Smart { section, .. } if section == "auth"),
        "{refused}"
    );
    assert!(!everything(&refused).contains(SCRAPE_HASH), "{refused}");
    Ok(())
}

#[test]
fn an_empty_or_missing_password_hash_file_is_refused() -> TestResult {
    let empty = secret_file("\n")?;
    let dir = tempfile::tempdir()?;
    for path in [empty.path().to_owned(), dir.path().join("absent")] {
        let text = format!(
            "{}{}",
            remote("127.0.0.1:9464", &scraping()),
            basic_user(&format!("password_hash_file = {}\n", quoted(&path)))
        );
        let refused = refusal(&text)?;
        assert!(
            matches!(&refused, Error::Smart { section, .. } if section == "auth"),
            "{}: {refused}",
            path.display()
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
    let mutual = format!("{tls}client_ca_file = {}\n", quoted(&files.client_ca));
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
        assert!(settings.metrics.scrape_users.is_empty());
    }
    for loopback in ["127.0.0.1:9464", "[::1]:9464"] {
        let settings = resolved(&format!("[metrics]\nlisten = \"{loopback}\"\n"))?;
        assert_eq!(Some(loopback.parse()?), settings.metrics.listen);
    }
    Ok(())
}
