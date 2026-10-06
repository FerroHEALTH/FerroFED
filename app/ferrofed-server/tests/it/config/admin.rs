// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[metrics]` and the admin listener's authentication: outside the
//! development profile, a listener off loopback is refused unless a scrape
//! token or a client CA authenticates the scrape; the scrape token is a
//! secret with a `_file` sibling, never shown. No specification governs the
//! admin listener: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error as StdError;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::settings::Settings;
use ferrofed_testkit::listener::ListenerCertificates;

use super::{everything, refusal, secret_file};

type TestResult = Result<(), Box<dyn StdError>>;

/// A synthetic scrape token.
const TOKEN: &str = "synthetic-scrape-token-a41e";

/// The addresses off loopback a listener may name.
const REMOTE: [&str; 3] = ["0.0.0.0:9464", "192.0.2.10:9464", "[::]:9464"];

/// The `[metrics]` table that serves `listen` with remote serving allowed,
/// with `extra` keys appended.
fn remote(listen: &str, extra: &str) -> String {
    format!("[metrics]\nlisten = \"{listen}\"\nallow_remote = true\n{extra}")
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
            matches!(refused, Error::MetricsUnauthenticated { address } if address == listen.parse()?),
            "{listen}: {refused}"
        );
        let shown = refused.to_string();
        assert!(
            shown.contains("metrics.scrape_token_file")
                && shown.contains("metrics.tls.client_ca_file"),
            "the refusal names what authenticates the scrape: {shown}"
        );
    }
    Ok(())
}

#[test]
fn a_scrape_token_admits_a_remote_listener_and_is_never_shown() -> TestResult {
    for listen in REMOTE {
        let settings = resolved(&remote(listen, &format!("scrape_token = \"{TOKEN}\"\n")))?;
        assert_eq!(Some(listen.parse()?), settings.metrics.listen);
        let token = settings.metrics.scrape_token.as_ref().ok_or("a token")?;
        assert_eq!(TOKEN, token.expose());
        assert!(!format!("{:?}", settings.metrics).contains(TOKEN));
        assert!(!format!("{settings:?}").contains(TOKEN));
    }
    Ok(())
}

#[test]
fn the_scrape_token_is_read_from_its_file() -> TestResult {
    let file = secret_file(&format!("{TOKEN}\n"))?;
    let extra = format!("scrape_token_file = {}\n", quoted(file.path()));
    let settings = resolved(&remote("0.0.0.0:9464", &extra))?;
    let token = settings.metrics.scrape_token.as_ref().ok_or("a token")?;
    assert_eq!(TOKEN, token.expose(), "read and trimmed");
    Ok(())
}

#[test]
fn the_scrape_token_and_its_file_together_are_a_conflict() -> TestResult {
    let file = secret_file(TOKEN)?;
    let extra = format!(
        "scrape_token = \"{TOKEN}\"\nscrape_token_file = {}\n",
        quoted(file.path())
    );
    let refused = refusal(&remote("127.0.0.1:9464", &extra))?;
    assert!(
        matches!(&refused, Error::Conflict { key } if key == "metrics.scrape_token"),
        "{refused}"
    );
    assert!(!everything(&refused).contains(TOKEN), "{refused}");
    Ok(())
}

#[test]
fn an_empty_or_missing_scrape_token_file_is_refused() -> TestResult {
    let empty = secret_file("\n")?;
    let refused = refusal(&remote(
        "127.0.0.1:9464",
        &format!("scrape_token_file = {}\n", quoted(empty.path())),
    ))?;
    assert!(
        refused.to_string().contains("metrics.scrape_token_file"),
        "{refused}"
    );
    let dir = tempfile::tempdir()?;
    let refused = refusal(&remote(
        "127.0.0.1:9464",
        &format!(
            "scrape_token_file = {}\n",
            quoted(&dir.path().join("absent"))
        ),
    ))?;
    assert!(
        refused.to_string().contains("metrics.scrape_token_file"),
        "{refused}"
    );
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
        matches!(without_ca, Error::MetricsUnauthenticated { .. }),
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
        assert!(settings.metrics.scrape_token.is_none());
    }
    for loopback in ["127.0.0.1:9464", "[::1]:9464"] {
        let settings = resolved(&format!("[metrics]\nlisten = \"{loopback}\"\n"))?;
        assert_eq!(Some(loopback.parse()?), settings.metrics.listen);
    }
    Ok(())
}
