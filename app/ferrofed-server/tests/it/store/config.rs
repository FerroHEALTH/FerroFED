// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[store]` as written and resolved, and the boot refusals of a
//! configuration that needs it and sets none. No specification governs the
//! store or its configuration: our own design.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;
use std::time::Duration;

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error as ConfigError;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::store::{MAX_CONNECTIONS, StoreFault};
use ferrohealth_secret::file::SecretError;

use crate::run::binary;

/// A synthetic password the redaction checks look for, which no output may
/// show.
const STORE_PASSWORD: &str = "synthetic-store-pw-4q8";

type TestResult = Result<(), Box<dyn Error>>;

/// A connection string requiring TLS, with [`STORE_PASSWORD`].
fn url(query: &str) -> String {
    format!("postgres://ferrofed:{STORE_PASSWORD}@db.example.org:5432/ferrofed{query}")
}

/// A trusted issuer declared a national contact point, every claim of its
/// table named.
const CONTACT_POINT: &str = "[auth]\n\n\
[[auth.issuer]]\nissuer = \"https://ncp.example.test\"\nkind = \"persons\"\naudience = [\"urn:example:gateway\"]\n\n\
[[trust.issuer]]\nissuer = \"https://ncp.example.test\"\n\n\
[trust.issuer.national_contact_point]\nfamily_name = \"f\"\ngiven_name = \"g\"\n\
country_code = \"c\"\nprofessional_issuing_authority = \"pa\"\n\
provider_issuing_authority = \"ha\"\nprovider_address = \"addr\"\ntrc_patient = \"trc\"\n";

/// A stored-query registry on the `postgres` backend, over a registry
/// document in `dir`.
fn postgres_backend(dir: &Path) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        crate::facade::registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\n\n[stored_queries]\nbackend = \"postgres\"\n{}",
        crate::support::audit_toml()
    ))
}

/// What `text` resolves to.
fn resolved(text: &str) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    Ok(Config::from_sources(Some(text), &BTreeMap::new())?.resolve())
}

/// The settings of a `[store]` table holding `lines`.
fn store(lines: &str) -> Result<Result<Settings, ConfigError>, Box<dyn Error>> {
    resolved(&format!("[store]\n{lines}\n"))
}

#[test]
fn a_contact_point_issuer_without_the_store_is_refused_naming_both() -> TestResult {
    let refused = resolved(CONTACT_POINT)?.err().ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Store(StoreFault::Required { key }) if key == "trust.issuer[0].national_contact_point"),
        "{refused:?}"
    );
    let text = refused.to_string();
    assert!(
        text.contains("trust.issuer[0].national_contact_point") && text.contains("[store]"),
        "{text}"
    );
    let settings = resolved(&format!(
        "{}\n{CONTACT_POINT}",
        crate::support::store_table()
    ))??;
    assert!(settings.store.is_some(), "it resolves beside [store]");
    Ok(())
}

#[test]
fn the_postgres_backend_without_the_store_is_refused_naming_both() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = postgres_backend(dir.path())?;
    let refused = resolved(&text)?.err().ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Store(StoreFault::Required { key }) if key == "stored_queries.backend"),
        "{refused:?}"
    );
    let line = refused.to_string();
    assert!(
        line.contains("stored_queries.backend") && line.contains("[store]"),
        "{line}"
    );
    let settings = resolved(&format!("{text}\n{}", crate::support::store_table()))??;
    assert!(settings.store.is_some(), "it resolves beside [store]");
    Ok(())
}

#[test]
fn a_deployment_without_a_contact_point_or_the_postgres_backend_needs_no_store() -> TestResult {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(
        &document,
        crate::facade::registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let stored = toml::Value::String(dir.path().join("q.redb").display().to_string());
    let document = toml::Value::String(document.display().to_string());
    for text in [
        String::from("profile = \"production\"\n"),
        format!(
            "[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
             node_selection = \"ask-all\"\n\n[stored_queries]\npath = {stored}\n{}",
            crate::support::audit_toml()
        ),
    ] {
        let settings = resolved(&text)??;
        assert!(settings.store.is_none(), "{text}");
    }
    Ok(())
}

#[test]
fn the_serve_and_check_jobs_refuse_a_contact_point_without_the_store_by_name() -> TestResult {
    for job in [&["serve"][..], &["config", "check"][..]] {
        let output = binary(job, CONTACT_POINT)?;
        let stderr = String::from_utf8(output.stderr)?;
        assert_eq!(
            Some(78),
            output.status.code(),
            "{job:?}: EX_CONFIG: {stderr}"
        );
        assert!(
            stderr.contains("trust.issuer[0].national_contact_point") && stderr.contains("[store]"),
            "{job:?}: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn a_store_with_tls_off_is_refused_under_every_profile_naming_the_key() -> TestResult {
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("url");
    std::fs::write(&file, format!("{}\n", url("?sslmode=prefer")))?;
    let file = toml::Value::String(file.display().to_string());
    for profile in ["production", "development"] {
        for (lines, key) in [
            (format!("url = \"{}\"", url("")), "store.url"),
            (
                format!("url = \"{}\"", url("?sslmode=disable")),
                "store.url",
            ),
            (format!("url = \"{}\"", url("?sslmode=prefer")), "store.url"),
            (
                String::from("url = \"host=/run/postgresql user=ferrofed\""),
                "store.url",
            ),
            (format!("url_file = {file}"), "store.url_file"),
        ] {
            let text = format!("profile = \"{profile}\"\n\n[store]\n{lines}\n");
            let refused = resolved(&text)?.err().ok_or("refused")?;
            assert!(
                matches!(&refused, ConfigError::Store(StoreFault::TlsOff { key: named }) if named == key),
                "{profile} {lines}: {refused:?}"
            );
            let line = refused.to_string();
            assert!(line.contains("sslmode=require"), "{line}");
            assert!(!line.contains(STORE_PASSWORD), "{line}");
        }
    }
    Ok(())
}

#[test]
fn the_url_reads_inline_or_from_its_file_and_is_never_shown() -> TestResult {
    let dir = tempfile::tempdir()?;
    let required = url("?sslmode=require");
    let secret = dir.path().join("url");
    std::fs::write(&secret, format!("{required}\n"))?;
    let secret = toml::Value::String(secret.display().to_string());
    for lines in [
        format!("url = \"{required}\""),
        format!("url_file = {secret}"),
    ] {
        let config = Config::from_sources(Some(&format!("[store]\n{lines}\n")), &BTreeMap::new())?;
        assert!(!format!("{config:?}").contains(STORE_PASSWORD), "{lines}");
        let settings = config.resolve()?;
        let held = settings.store.as_ref().ok_or("resolved")?;
        assert_eq!(required, held.url.expose(), "trimmed");
        assert!(
            !format!("{settings:?}").contains(STORE_PASSWORD),
            "Debug redacts"
        );
        assert_eq!(4, held.max_connections.get());
        assert_eq!(Duration::from_secs(10), held.timeout);
        assert_eq!(Duration::from_secs(60), held.sweep_interval);
        assert_eq!(None, held.ca);
    }
    let refused = store(&format!("url = \"{required}\"\nurl_file = {secret}"))?
        .err()
        .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::SecretFile(SecretError::Both { key }) if key == "store.url"),
        "{refused:?}"
    );
    let refused = store("max_connections = 2")?.err().ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Missing { key } if key == "store.url"),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn an_unparsable_url_is_refused_without_quoting_it() -> TestResult {
    let refused = store(&format!(
        "url = \"postgres://u:{STORE_PASSWORD}@h:port/d?sslmode=require\""
    ))?
    .err()
    .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Store(StoreFault::Url { key }) if key == "store.url"),
        "{refused:?}"
    );
    assert!(!refused.to_string().contains(STORE_PASSWORD), "{refused}");
    assert!(
        !format!("{refused:?}").contains(STORE_PASSWORD),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn the_bounds_and_the_ca_file_are_held_to_their_rules() -> TestResult {
    let dir = tempfile::tempdir()?;
    let base = format!("url = \"{}\"", url("?sslmode=require"));
    let too_many = MAX_CONNECTIONS + 1;
    for count in [0, too_many] {
        let refused = store(&format!("{base}\nmax_connections = {count}"))?
            .err()
            .ok_or("refused")?;
        assert!(
            matches!(&refused, ConfigError::Store(StoreFault::Connections { value }) if *value == count),
            "{refused:?}"
        );
    }
    for key in ["timeout_ms", "sweep_interval_s"] {
        let refused = store(&format!("{base}\n{key} = 0"))?
            .err()
            .ok_or("refused")?;
        assert!(
            matches!(&refused, ConfigError::Zero { key: named } if *named == format!("store.{key}")),
            "{refused:?}"
        );
    }
    let empty = dir.path().join("empty.pem");
    std::fs::write(&empty, "no certificate here\n")?;
    let refused = store(&format!(
        "{base}\nca_file = {}",
        toml::Value::String(empty.display().to_string())
    ))?
    .err()
    .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Store(StoreFault::Ca { .. })),
        "{refused:?}"
    );
    let missing = dir.path().join("missing.pem");
    let refused = store(&format!(
        "{base}\nca_file = {}",
        toml::Value::String(missing.display().to_string())
    ))?
    .err()
    .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Secret { key, .. } if key == "store.ca_file"),
        "{refused:?}"
    );
    let certificates = ferrofed_testkit::listener::ListenerCertificates::generate()?;
    let ca = dir.path().join("ca.pem");
    std::fs::write(&ca, certificates.ca())?;
    let settings = store(&format!(
        "{base}\nca_file = {}\nmax_connections = {MAX_CONNECTIONS}\ntimeout_ms = 2500\nsweep_interval_s = 5",
        toml::Value::String(ca.display().to_string())
    ))??;
    let held = settings.store.ok_or("resolved")?;
    assert_eq!(Some(certificates.ca()), held.ca.as_deref());
    assert_eq!(MAX_CONNECTIONS, held.max_connections.get());
    assert_eq!(Duration::from_millis(2500), held.timeout);
    assert_eq!(Duration::from_secs(5), held.sweep_interval);
    let unknown = Config::from_sources(
        Some(&format!("[store]\n{base}\npool = 3\n")),
        &BTreeMap::new(),
    );
    assert!(
        unknown.as_ref().err().and_then(crate::support::refused_key) == Some("store.pool"),
        "an unknown key is refused at its key: {unknown:?}"
    );
    Ok(())
}
