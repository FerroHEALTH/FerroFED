// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` table a gateway refuses to start with. Each FHIR profile
//! has its actors record their transactions (PIXm §2:3.83.5.1, mCSD
//! §2:3.90.5.1, PMIR §2:3.93.5.1), so outside development a PIXm, mCSD or
//! PMIR binding needs a declared destination and `off` is refused; the
//! repository is `https` and its spool on disk, since every record names the
//! patient. No specification governs the table: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::xcpd::AuditDestination;

use crate::facade::registry;

type TestResult = Result<(), Box<dyn Error>>;

/// A gateway under `profile` resolving at an `https` PIX Manager, with the
/// tables `audit`.
fn text(dir: &Path, profile: &str, audit: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("https://cdr-a.example.org", "https://cdr-b.example.org", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "profile = \"{profile}\"\n\n[registry]\ndocument = {document}\n\n[federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.10\"\n\"node-b\" = \"urn:oid:2.999.20\"\n\n{audit}"
    ))
}

fn resolve(text: &str) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(&crate::support::signed(text)), &BTreeMap::new())?.resolve())
}

/// The message of the refusal `text` resolves to.
fn refusal(text: &str) -> Result<String, Box<dyn Error>> {
    match resolve(text)? {
        Ok(_) => Err("the configuration is refused".into()),
        Err(error) => Ok(error.to_string()),
    }
}

#[test]
fn a_binding_outside_development_needs_a_declared_destination() -> TestResult {
    let dir = tempfile::tempdir()?;
    let message = refusal(&text(dir.path(), "production", "")?)?;
    assert!(message.contains("audit.destination"), "{message}");
    Ok(())
}

#[test]
fn off_is_refused_outside_development_and_admitted_inside() -> TestResult {
    let dir = tempfile::tempdir()?;
    let off = "[audit]\ndestination = \"off\"\n";
    let message = refusal(&text(dir.path(), "production", off)?)?;
    assert!(message.contains("audit.destination"), "{message}");
    let settings =
        resolve(&text(dir.path(), "development", off)?)?.map_err(|error| error.to_string())?;
    assert_eq!(AuditDestination::Off, settings.audit.destination);
    let unset =
        resolve(&text(dir.path(), "development", "")?)?.map_err(|error| error.to_string())?;
    assert_eq!(
        AuditDestination::Off,
        unset.audit.destination,
        "development without [audit] records nothing"
    );
    Ok(())
}

#[test]
fn the_repository_table_and_the_destination_go_together() -> TestResult {
    let dir = tempfile::tempdir()?;
    let message = refusal(&text(
        dir.path(),
        "production",
        "[audit]\ndestination = \"repository\"\n",
    )?)?;
    assert!(message.contains("audit.repository"), "{message}");
    let message = refusal(&text(
        dir.path(),
        "production",
        "[audit]\ndestination = \"log\"\n\n[audit.repository]\nurl = \"https://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\n",
    )?)?;
    assert!(message.contains("[audit.repository]"), "{message}");
    Ok(())
}

#[test]
fn outside_development_the_repository_is_https_with_a_spool_on_disk() -> TestResult {
    let dir = tempfile::tempdir()?;
    let spool = toml::Value::String(dir.path().join("spool").display().to_string());
    let message = refusal(&text(
        dir.path(),
        "production",
        "[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"https://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\n",
    )?)?;
    assert!(message.contains("audit.repository.spool_dir"), "{message}");
    let cleartext = text(
        dir.path(),
        "production",
        &format!(
            "[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"http://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\nspool_dir = {spool}\n"
        ),
    )?;
    let settings = resolve(&cleartext)?.map_err(|error| error.to_string())?;
    let refused = ferrofed_server::config::transport::check(&settings, None)
        .err()
        .ok_or("plain http is refused outside development")?;
    assert_eq!("audit.repository.url", refused.site.url_key);
    let https = text(
        dir.path(),
        "production",
        &format!(
            "[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"https://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\nspool_dir = {spool}\n"
        ),
    )?;
    let settings = resolve(&https)?.map_err(|error| error.to_string())?;
    let repository = settings.audit.repository.ok_or("a repository")?;
    assert_eq!("gateway.example.org", repository.observer.source_id);
    assert!(!repository.cleartext);
    Ok(())
}

#[test]
fn the_spool_write_timeout_bounds_each_record_and_is_never_zero() -> TestResult {
    let dir = tempfile::tempdir()?;
    let spool = toml::Value::String(dir.path().join("spool").display().to_string());
    let table = |extra: &str| {
        format!(
            "[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"https://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\nspool_dir = {spool}\n{extra}"
        )
    };
    let settings = resolve(&text(dir.path(), "production", &table(""))?)?
        .map_err(|error| error.to_string())?;
    let repository = settings.audit.repository.ok_or("a repository")?;
    assert_eq!(
        std::time::Duration::from_secs(2),
        repository.bounds.write_timeout,
        "the default bound"
    );
    let message = refusal(&text(
        dir.path(),
        "production",
        &table("spool_write_timeout_ms = 0\n"),
    )?)?;
    assert!(
        message.contains("audit.repository.spool_write_timeout_ms"),
        "{message}"
    );
    Ok(())
}
