// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[audit]` table a gateway refuses to start with. Each FHIR profile
//! has its actors record their transactions (PIXm §2:3.83.5.1, mCSD
//! §2:3.90.5.1, PMIR §2:3.93.5.1), so outside development a PIXm, mCSD or
//! PMIR binding needs a declared destination and `off` is refused, a
//! registry refuses `log` (Regulation (EU) 2025/327 Annex II 3.2), and the
//! repository is `https` and its spool on disk, since every record names the
//! patient. No specification governs the table: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use ferrofed_server::binding::ihe::xcpd::AuditDestination;
use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;

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
    Ok(
        Config::from_sources(Some(&crate::support::signing_only(text)), &BTreeMap::new())?
            .resolve(),
    )
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

/// The `[audit]` tables of a repository spooling in `spool`.
fn spooled_in(spool: &Path) -> String {
    format!(
        "[audit]\ndestination = \"repository\"\n\n[audit.repository]\nurl = \"https://arr.example.org/fhir\"\nhostname = \"gateway.example.org\"\nspool_dir = {}\n",
        toml::Value::String(spool.display().to_string())
    )
}

/// Every path under `dir`, sorted.
fn tree(dir: &Path) -> Result<Vec<std::path::PathBuf>, Box<dyn Error>> {
    let mut paths = Vec::new();
    let mut pending = vec![dir.to_owned()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path.clone());
            }
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Every cause of `error`, as one line.
fn chain(error: &dyn Error) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

#[test]
fn config_check_creates_no_spool_directory() -> TestResult {
    let dir = tempfile::tempdir()?;
    let spool = dir.path().join("lib").join("audit-feed-spool");
    let settings = resolve(&text(dir.path(), "production", &spooled_in(&spool))?)?
        .map_err(|error| error.to_string())?;
    let before = tree(dir.path())?;
    ferrofed_server::state::AppState::check(&settings).map_err(|error| chain(&error))?;
    assert!(!spool.exists(), "config check created {}", spool.display());
    assert_eq!(before, tree(dir.path())?);
    // The trail the check built is forgotten, so serving opens the spool.
    drop(ferrofed_server::state::AppState::build(&settings).map_err(|error| chain(&error))?);
    assert!(spool.is_dir(), "serving opens the spool");
    Ok(())
}

#[cfg(unix)]
#[test]
fn config_check_leaves_a_spool_in_use_as_it_found_it() -> TestResult {
    use std::os::unix::fs::DirBuilderExt as _;

    let dir = tempfile::tempdir()?;
    let spool = dir.path().join("audit-feed-spool");
    std::fs::DirBuilder::new().mode(0o700).create(&spool)?;
    std::fs::write(spool.join("00000000000000000003.partial"), b"torn")?;
    let settings = resolve(&text(dir.path(), "production", &spooled_in(&spool))?)?
        .map_err(|error| error.to_string())?;
    let before = tree(dir.path())?;
    ferrofed_server::state::AppState::check(&settings).map_err(|error| chain(&error))?;
    assert_eq!(
        before,
        tree(dir.path())?,
        "a partial file serve would remove"
    );
    Ok(())
}

/// The `[audit]` table that sends every record to the log target.
const LOG: &str = "[audit]\ndestination = \"log\"\n";

#[test]
fn log_is_refused_for_a_registry_outside_development_naming_the_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    match resolve(&text(dir.path(), "production", LOG)?)? {
        Err(ConfigError::AccessAuditLog { key }) => {
            assert_eq!("audit.destination", key);
            Ok(())
        }
        other => Err(format!(
            "the access log of a registry names the caller and the patient (Annex II 3.2): {other:?}"
        )
        .into()),
    }
}

#[test]
fn log_is_accepted_under_development_and_without_a_registry() -> TestResult {
    let dir = tempfile::tempdir()?;
    let settings =
        resolve(&text(dir.path(), "development", LOG)?)?.map_err(|error| error.to_string())?;
    assert_eq!(AuditDestination::Log, settings.audit.destination);
    let unregistered = format!(
        "profile = \"production\"\n\n{LOG}\n[[pixm.manager]]\nurl = \"https://pix.example.org/fhir/\"\n\n[pixm.manager.members]\n\"node-a\" = \"urn:oid:2.999.10\"\n"
    );
    let settings = resolve(&unregistered)?.map_err(|error| error.to_string())?;
    assert_eq!(
        AuditDestination::Log,
        settings.audit.destination,
        "a gateway with no registry records no access"
    );
    Ok(())
}

#[test]
fn config_check_refuses_a_spool_serve_could_not_open_naming_its_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("a-file");
    std::fs::write(&file, b"no directory")?;
    for spool in [file.clone(), file.join("audit-feed-spool")] {
        let settings = resolve(&text(dir.path(), "production", &spooled_in(&spool))?)?
            .map_err(|error| error.to_string())?;
        let before = tree(dir.path())?;
        let refused = ferrofed_server::state::AppState::check(&settings)
            .err()
            .ok_or("a spool serve cannot create is refused")?;
        let message = chain(&refused);
        assert!(message.contains("audit.repository.spool_dir"), "{message}");
        assert!(message.contains("is no directory"), "{message}");
        assert_eq!(before, tree(dir.path())?);
    }
    Ok(())
}

#[test]
fn config_check_prints_the_refusal_of_log_for_a_registry() -> TestResult {
    let dir = tempfile::tempdir()?;
    let output = crate::run::binary(&["config", "check"], &text(dir.path(), "production", LOG)?)?;
    assert_eq!(
        Some(i32::from(ferrofed_server::EXIT_CONFIG)),
        output.status.code()
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("audit.destination") && stderr.contains("\"repository\""),
        "the refusal names the key and the destination it needs: {stderr}"
    );
    Ok(())
}
