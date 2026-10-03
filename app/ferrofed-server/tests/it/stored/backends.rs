// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[stored_queries]` configuration (§12.7): the backend, `redb` unless
//! the table names another, each backend refusing a key it does not read,
//! the PostgreSQL connection string read inline or from its `_file` sibling
//! and never shown, and `config check` refusing a definition directory that
//! does not load. No specification governs the configuration: our own
//! design.
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
use ferrofed_server::config::stored_queries::{Backend, Store};
use ferrofed_server::telemetry::{DEFAULT_FILTER, Rendering, subscriber};

use crate::run::binary;
use crate::support::Logs;

type TestResult = Result<(), Box<dyn Error>>;

/// A password no output may show.
const PASSWORD: &str = "S3CRET-pw-91x";

/// The configuration text of a gateway over a registry document in `dir`,
/// with the `[stored_queries]` lines `table`.
fn text(dir: &Path, table: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        crate::facade::registry("http://a.invalid", "http://b.invalid", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(format!(
        "[registry]\ndocument = {document}\n\n[federation]\nid = \"example-federation\"\n\
         node_selection = \"ask-all\"\n\n[stored_queries]\n{table}\n"
    ))
}

/// What `table` resolves to.
fn resolved(dir: &Path, table: &str) -> Result<Result<Option<Store>, ConfigError>, Box<dyn Error>> {
    let config = Config::from_sources(Some(&text(dir, table)?), &BTreeMap::new())?;
    Ok(config.resolve().map(|settings| settings.stored_queries))
}

/// What `settings` log when the configuration resolves.
fn logged(settings: &Settings) -> Result<String, Box<dyn Error>> {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, DEFAULT_FILTER, false, logs.clone())?;
    tracing::subscriber::with_default(capture, || settings.log_summary());
    Ok(logs.text())
}

#[test]
fn the_backend_is_redb_unless_the_table_names_another() -> TestResult {
    let dir = tempfile::tempdir()?;
    let store = resolved(dir.path(), "path = \"/var/lib/ferrofed/q.redb\"")??;
    assert_eq!(
        Some(Backend::Redb),
        store.as_ref().map(Store::backend),
        "the default"
    );
    let store = resolved(
        dir.path(),
        "backend = \"files\"\npath = \"/etc/ferrofed/q\"",
    )??;
    assert_eq!(
        Some(Backend::Files),
        store.as_ref().map(Store::backend),
        "named"
    );
    let unset =
        Config::from_sources(Some("profile = \"production\"\n"), &BTreeMap::new())?.resolve()?;
    assert!(unset.stored_queries.is_none(), "no table, no registry");
    Ok(())
}

#[test]
fn the_startup_log_names_the_backend_and_never_where_it_is() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = "path = \"/var/lib/ferrofed/hidden-store-k3.redb\"";
    let config = Config::from_sources(Some(&text(dir.path(), table)?), &BTreeMap::new())?;
    let logged = logged(&config.resolve()?)?;
    assert!(
        logged.contains(r#""stored_query_backend":"redb""#),
        "{logged}"
    );
    assert!(!logged.contains("hidden-store-k3"), "{logged}");
    Ok(())
}

#[test]
fn each_backend_refuses_a_key_it_does_not_read_and_needs_its_own() -> TestResult {
    let dir = tempfile::tempdir()?;
    for (table, key) in [
        ("backend = \"redb\"", "stored_queries.path"),
        ("backend = \"files\"", "stored_queries.path"),
        ("backend = \"files\"\npath = \"\"", "stored_queries.path"),
    ] {
        let refused = resolved(dir.path(), table)?.err().ok_or("refused")?;
        assert!(
            matches!(&refused, ConfigError::Missing { key: missing } if missing == key),
            "{table}: {refused:?}"
        );
    }
    for (table, key, backend) in [
        (
            "path = \"/q.redb\"\nurl = \"postgres://h/d\"",
            "stored_queries.url",
            Backend::Redb,
        ),
        (
            "backend = \"files\"\npath = \"/q\"\nurl_file = \"/run/secrets/url\"",
            "stored_queries.url_file",
            Backend::Files,
        ),
        (
            "backend = \"postgres\"\npath = \"/q\"",
            "stored_queries.path",
            Backend::Postgres,
        ),
    ] {
        let refused = resolved(dir.path(), table)?.err().ok_or("refused")?;
        assert!(
            matches!(&refused, ConfigError::StoreKey { key: set, backend: named } if set == key && *named == backend),
            "{table}: {refused:?}"
        );
    }
    let unknown = Config::from_sources(
        Some(&text(dir.path(), "backend = \"sqlite\"")?),
        &BTreeMap::new(),
    );
    assert!(
        matches!(&unknown, Err(ConfigError::Parse { fault }) if fault.key.as_deref() == Some("stored_queries.backend")),
        "an unknown backend is refused at its key: {unknown:?}"
    );
    Ok(())
}

#[cfg(not(feature = "postgres"))]
#[test]
fn a_build_without_the_postgres_feature_refuses_the_postgres_backend() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = format!("backend = \"postgres\"\nurl = \"postgres://u:{PASSWORD}@h/d\"");
    let refused = resolved(dir.path(), &table)?.err().ok_or("refused")?;
    assert!(
        matches!(
            &refused,
            ConfigError::StoreBackendUnavailable {
                backend: Backend::Postgres
            }
        ),
        "{refused:?}"
    );
    assert!(!refused.to_string().contains(PASSWORD), "{refused}");
    Ok(())
}

#[cfg(feature = "postgres")]
#[test]
fn a_postgres_url_resolves_inline_or_from_its_file_and_is_never_shown() -> TestResult {
    let dir = tempfile::tempdir()?;
    let url = format!("postgres://ferrofed:{PASSWORD}@db.example.org/ferrofed?sslmode=require");
    let secret = dir.path().join("url");
    std::fs::write(&secret, format!("{url}\n"))?;
    let secret = toml::Value::String(secret.display().to_string());
    for table in [
        format!("backend = \"postgres\"\nurl = \"{url}\""),
        format!("backend = \"postgres\"\nurl_file = {secret}"),
    ] {
        let config = Config::from_sources(Some(&text(dir.path(), &table)?), &BTreeMap::new())?;
        assert!(!format!("{config:?}").contains(PASSWORD), "{table}");
        let settings = config.resolve()?;
        let store = settings.stored_queries.as_ref().ok_or("offered")?;
        let Store::Postgres(held) = store else {
            panic!("a postgres store: {store:?}");
        };
        assert_eq!(url, held.expose(), "trimmed");
        assert!(!format!("{settings:?}").contains(PASSWORD), "Debug redacts");
        let logged = logged(&settings)?;
        assert!(
            logged.contains(r#""stored_query_backend":"postgres""#),
            "{logged}"
        );
        for hidden in [PASSWORD, "db.example.org"] {
            assert!(
                !logged.contains(hidden),
                "the log names the kind only: {logged}"
            );
        }
    }
    let both = format!("backend = \"postgres\"\nurl = \"{url}\"\nurl_file = {secret}");
    let refused = resolved(dir.path(), &both)?.err().ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Conflict { key } if key == "stored_queries.url"),
        "{refused:?}"
    );
    let refused = resolved(dir.path(), "backend = \"postgres\"")?
        .err()
        .ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::Missing { key } if key == "stored_queries.url"),
        "{refused:?}"
    );
    Ok(())
}

#[cfg(feature = "postgres")]
#[test]
fn an_unparsable_postgres_url_is_refused_without_quoting_it() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = format!("backend = \"postgres\"\nurl = \"postgres://u:{PASSWORD}@h:port/d\"");
    let refused = resolved(dir.path(), &table)?.err().ok_or("refused")?;
    assert!(
        matches!(&refused, ConfigError::StoreUrl { key } if key == "stored_queries.url"),
        "{refused:?}"
    );
    assert!(!refused.to_string().contains(PASSWORD), "{refused}");
    assert!(!format!("{refused:?}").contains(PASSWORD), "{refused:?}");
    Ok(())
}

#[test]
fn config_check_refuses_a_definition_directory_that_does_not_load() -> TestResult {
    let dir = tempfile::tempdir()?;
    let definitions = dir.path().join("definitions");
    let named = definitions.join("org.example::q");
    std::fs::create_dir_all(&named)?;
    std::fs::write(named.join("1.0.0.aql"), "SELECT SENTINEL_TEXT_k2 FROM")?;
    let path = toml::Value::String(definitions.display().to_string());
    let table = format!("backend = \"files\"\npath = {path}");
    let output = binary(&["config", "check"], &text(dir.path(), &table)?)?;
    let stderr = String::from_utf8(output.stderr)?;
    assert_eq!(Some(78), output.status.code(), "EX_CONFIG: {stderr}");
    assert!(stderr.contains("1.0.0.aql"), "names the file: {stderr}");
    assert!(
        !stderr.contains("SENTINEL_TEXT_k2"),
        "never its content: {stderr}"
    );

    std::fs::write(named.join("1.0.0.aql"), "SELECT e/ehr_id/value FROM EHR e")?;
    let output = binary(&["config", "check"], &text(dir.path(), &table)?)?;
    assert_eq!(
        Some(0),
        output.status.code(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
