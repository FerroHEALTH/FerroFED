// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[pmir]` table a gateway refuses to start with: no registry, no feed
//! token, a path on the ITS-REST surface or the health family, a URL with
//! credentials in it, an OAuth 2.0 grant, and a Registry or a callback reached
//! in the clear outside the development profile, since both carry patient
//! identities (PMIR §2:3.93.5). No specification governs the table: our own
//! design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;

use super::{TOKEN, text};

type TestResult = Result<(), Box<dyn Error>>;

fn resolve(text: &str) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(&crate::support::signed(text)), &BTreeMap::new())?.resolve())
}

fn development(extra: &str) -> Result<(tempfile::TempDir, String), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let text = text(
        dir.path(),
        "http://127.0.0.1:9/fhir/",
        "http://127.0.0.1:9/pmir/feed",
        extra,
    )?;
    Ok((dir, text))
}

#[test]
fn a_development_gateway_resolves_the_table() -> TestResult {
    let (_dir, text) = development("")?;
    let settings = resolve(&text)?.map_err(|error| error.to_string())?;
    let pmir = settings.pmir.ok_or("[pmir] is resolved")?;
    assert_eq!("/pmir/feed", pmir.path);
    assert_eq!(TOKEN, pmir.feed_token.expose());
    Ok(())
}

#[test]
fn the_feed_token_is_read_from_its_file() -> TestResult {
    let dir = tempfile::tempdir()?;
    let token = dir.path().join("feed-token");
    std::fs::write(&token, "Qz7filetoken\n")?;
    let (_registry, text) = development("")?;
    let text = text.replacen(
        &format!("feed_token = \"{TOKEN}\""),
        &format!(
            "feed_token_file = {}",
            toml::Value::String(token.display().to_string())
        ),
        1,
    );
    let settings = resolve(&text)?.map_err(|error| error.to_string())?;
    let pmir = settings.pmir.ok_or("[pmir] is resolved")?;
    assert_eq!("Qz7filetoken", pmir.feed_token.expose());
    Ok(())
}

#[test]
fn a_table_without_a_registry_is_refused() -> TestResult {
    let text = format!(
        "[pmir]\nurl = \"https://pmir.example.org/fhir/\"\ncallback_url = \"https://gateway.example.org/pmir/feed\"\nfeed_token = \"{TOKEN}\"\n"
    );
    let error = resolve(&text)?.err().ok_or("refused")?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "registry.document"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_table_without_a_feed_token_is_refused() -> TestResult {
    let (_dir, text) = development("")?;
    let text = text.replacen(&format!("feed_token = \"{TOKEN}\"\n"), "", 1);
    let error = resolve(&text)?.err().ok_or("refused")?;
    assert!(
        matches!(&error, ConfigError::Missing { key } if key == "pmir.feed_token"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_feed_path_on_another_surface_is_refused() -> TestResult {
    for path in ["/v1/pmir", "/health/pmir", "/.well-known/pmir", "/"] {
        let (_dir, text) = development("")?;
        let text = text.replacen("path = \"/pmir/feed\"", &format!("path = \"{path}\""), 1);
        let error = resolve(&text)?.err().ok_or("refused")?;
        assert!(
            matches!(error, ConfigError::FeedPath { .. }),
            "{path}: {error}"
        );
    }
    Ok(())
}

#[test]
fn a_url_with_credentials_is_refused() -> TestResult {
    let (_dir, text) = development("")?;
    let text = text.replacen(
        "url = \"http://127.0.0.1:9/fhir/\"\ncallback_url",
        "url = \"http://user:Qz7password@127.0.0.1:9/fhir/\"\ncallback_url",
        1,
    );
    let error = resolve(&text)?.err().ok_or("refused")?;
    assert!(
        matches!(&error, ConfigError::HttpUrl { key } if key == "pmir.url"),
        "{error}"
    );
    assert!(!error.to_string().contains("Qz7password"));
    Ok(())
}

#[test]
fn an_oauth2_grant_is_refused() -> TestResult {
    let (_dir, text) = development(
        "\n[pmir.credentials.oauth2]\ngrant = \"client_credentials\"\nclient_auth = \"private_key_jwt\"\ntoken_endpoint = \"http://127.0.0.1:9/token\"\nclient_id = \"gateway\"\nscope = \"system/aql-*.s\"\n",
    )?;
    let error = resolve(&text)?.err().ok_or("refused")?;
    assert!(
        matches!(&error, ConfigError::GrantNotHere { section } if section == "pmir.credentials"),
        "{error}"
    );
    Ok(())
}

#[test]
fn a_registry_or_callback_in_the_clear_is_refused_outside_development() -> TestResult {
    for (url, callback, key) in [
        (
            "http://pmir.example.org/fhir/",
            "https://gateway.example.org/pmir/feed",
            "pmir.url",
        ),
        (
            "https://pmir.example.org/fhir/",
            "http://gateway.example.org/pmir/feed",
            "pmir.callback_url",
        ),
    ] {
        let dir = tempfile::tempdir()?;
        let text = text(dir.path(), url, callback, "")?.replacen(
            "profile = \"development\"",
            "profile = \"production\"",
            1,
        );
        let error = resolve(&text)?.err().ok_or("refused")?;
        assert!(
            matches!(&error, ConfigError::Cleartext(refused) if refused.site.url_key == key),
            "{key}: {error}"
        );
    }
    Ok(())
}
