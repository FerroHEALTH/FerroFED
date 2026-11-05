// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PostgreSQL connections under the encrypted-connection policy.
//!
//! FerroFED's own store (`[store]`) is reached over TLS under every profile:
//! a connection string that does not set `sslmode=require` is refused by
//! `config check`, `serve` and a reload, naming `store.url`. The revision
//! store's `config.url` keeps the policy of every other credential: outside
//! the development profile a connection string that carries a password to a
//! networked host must set `sslmode=require`, and under it the connection
//! starts and is named. No specification governs this: our own design.

use std::sync::Arc;

use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::store::StoreFault;
use ferrofed_server::config::transport::{Encryption, ProtectedSite};
use ferrofed_server::reload::ReloadError;

use super::{
    Gateway, HTTPS_A, SECRET, TestResult, binary, configuration, document, refused_on, reported,
    resolved,
};

/// The site the revision store's connection string is held as.
fn revisions_site() -> ProtectedSite {
    ProtectedSite {
        url_key: String::from("config.url"),
        payload: String::from("the password in config.url"),
        requires: Encryption::Tls,
    }
}

/// The `[config]` table of a shared revision store at `url`, with the admin
/// listener it needs.
fn revisions(url: &str) -> String {
    format!(
        "[metrics]\nlisten = \"127.0.0.1:0\"\n\n\
         [config]\nui = \"managed\"\nbackend = \"postgres\"\nurl = \"{url}\"\n"
    )
}

/// The `[store]` table at `url`.
fn store(url: &str) -> String {
    format!("[store]\nurl = \"{url}\"\n")
}

/// A connection to a networked host with a password and `query` appended.
fn networked(query: &str) -> String {
    format!("postgres://ferrofed:{SECRET}@db.example.org:5432/ferrofed{query}")
}

#[test]
fn the_store_without_required_tls_is_refused_under_every_profile() -> TestResult {
    let dir = tempfile::tempdir()?;
    let pairs = format!("host=db.example.org user=ferrofed password={SECRET} dbname=ferrofed");
    for profile in ["production", "development"] {
        for url in [
            networked(""),
            networked("?sslmode=prefer"),
            networked("?sslmode=disable"),
            String::from("postgres://ferrofed@db.example.org:5432/ferrofed"),
            pairs.clone(),
        ] {
            let refused = resolved(dir.path(), profile, HTTPS_A, &store(&url))
                .err()
                .ok_or("refused")?;
            let refused = refused
                .downcast_ref::<ConfigError>()
                .ok_or("a configuration refusal")?;
            assert!(
                matches!(refused, ConfigError::Store(StoreFault::TlsOff { key }) if key == "store.url"),
                "{profile}: {refused:?}"
            );
            assert!(!refused.to_string().contains(SECRET), "{refused}");
        }
    }
    let settings = resolved(
        dir.path(),
        "production",
        HTTPS_A,
        &store(&networked("?sslmode=require")),
    )?;
    reported(&settings, &[])
}

#[test]
fn config_check_refuses_the_store_naming_its_key_and_never_its_password() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = configuration(
        "development",
        &document(dir.path(), HTTPS_A)?,
        &store(&networked("?sslmode=prefer")),
    );
    let output = binary(&["config", "check"], &text)?;
    assert_eq!(Some(78), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("store.url"), "{stderr}");
    assert!(stderr.contains("sslmode=require"), "{stderr}");
    assert!(!stderr.contains(SECRET), "{stderr}");
    assert!(!stderr.contains("db.example.org"), "{stderr}");
    Ok(())
}

#[test]
fn a_reload_that_adds_a_store_without_tls_is_refused() -> TestResult {
    let gateway = Gateway::start("production")?;
    let running = gateway
        .state
        .federation()
        .ok_or("a registry is configured")?;
    std::fs::write(
        &gateway.config,
        configuration(
            "production",
            &gateway.document,
            &format!("{}\n{}", super::oauth(), store(&networked(""))),
        ),
    )?;

    let refused = gateway
        .reloader
        .reload()
        .err()
        .ok_or("a store the restart would refuse is refused now")?;
    let ReloadError::Config(cause) = &refused else {
        return Err(format!("a configuration refusal: {refused:?}").into());
    };
    assert!(
        matches!(cause, ConfigError::Store(StoreFault::TlsOff { key }) if key == "store.url"),
        "{cause:?}"
    );
    let after = gateway
        .state
        .federation()
        .ok_or("a registry is configured")?;
    assert!(Arc::ptr_eq(&running, &after), "the running registry stays");
    Ok(())
}

#[test]
fn a_revision_store_password_without_required_tls_is_refused_outside_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    for query in ["", "?sslmode=prefer", "?sslmode=disable"] {
        let settings = resolved(
            dir.path(),
            "production",
            HTTPS_A,
            &revisions(&networked(query)),
        )?;
        refused_on(&settings, &revisions_site())?;
    }
    let settings = resolved(
        dir.path(),
        "production",
        HTTPS_A,
        &revisions(&networked("?sslmode=require")),
    )?;
    reported(&settings, &[])
}

#[test]
fn the_development_profile_starts_and_names_the_unencrypted_revision_store() -> TestResult {
    let dir = tempfile::tempdir()?;
    let settings = resolved(
        dir.path(),
        "development",
        HTTPS_A,
        &revisions(&networked("")),
    )?;
    reported(&settings, &[revisions_site()])
}
