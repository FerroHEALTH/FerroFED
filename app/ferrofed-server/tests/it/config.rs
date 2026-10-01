// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The configuration contract: the file, the environment over it, the `_file`
//! secrets, and every refusal.

use ferrofed_server::config::Config;
use ferrofed_server::config::error::Error;
use ferrofed_server::config::settings::Scheme;
use ferrofed_server::telemetry::Format;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::io::Write;
use std::time::Duration;

/// A file with every section set, so a test can override one key at a time.
const FULL: &str = r#"
[server]
listen = "0.0.0.0:9000"
request_timeout_ms = 1000
shutdown_timeout_ms = 2000
body_limit_bytes = 4096

[telemetry]
format = "json"
filter = "debug"

[credentials."hospital-a"]
bearer_token = "synthetic-token"

[credentials."clinic-b"]
user = "gateway"
password = "synthetic-password"
"#;

/// Returns the environment map a single override makes.
fn env(name: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_owned(), value.to_owned())])
}

/// Resolves `text` with no environment and returns the refusal.
fn refusal(text: &str) -> Result<Error, Box<dyn StdError>> {
    match Config::from_sources(Some(text), &BTreeMap::new()).and_then(|config| config.resolve()) {
        Ok(_) => Err("the configuration was accepted".into()),
        Err(error) => Ok(error),
    }
}

/// Returns a temporary file holding `content`.
fn secret_file(content: &str) -> Result<tempfile::NamedTempFile, Box<dyn StdError>> {
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(content.as_bytes())?;
    Ok(file)
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_file_states_every_section_and_the_resolver_reads_it() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    assert_eq!("0.0.0.0:9000", settings.server.listen.to_string());
    assert_eq!(Duration::from_millis(1000), settings.server.request_timeout);
    assert_eq!(
        Duration::from_millis(2000),
        settings.server.shutdown_timeout
    );
    assert_eq!(4096, settings.server.body_limit);
    assert_eq!(Format::Json, settings.telemetry.format);
    assert_eq!("debug", settings.telemetry.filter);
    match settings.credentials.get("hospital-a") {
        Some(Scheme::Bearer(token)) => assert_eq!("synthetic-token", token.expose_secret()),
        other => return Err(format!("hospital-a is a bearer scheme: {other:?}").into()),
    }
    match settings.credentials.get("clinic-b") {
        Some(Scheme::Basic { user, password }) => {
            assert_eq!("gateway", user);
            assert_eq!("synthetic-password", password.expose_secret());
        }
        other => return Err(format!("clinic-b is a basic scheme: {other:?}").into()),
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_resolved_secret_never_reaches_debug_output() -> Result<(), Box<dyn StdError>> {
    let settings = Config::from_sources(Some(FULL), &BTreeMap::new())?.resolve()?;
    let rendered = format!("{settings:?}");
    // NOTE: a failure message never echoes the rendering, which would put the
    // leaked secret into the test log it is meant to keep it out of.
    assert!(
        !rendered.contains("synthetic-token"),
        "Debug output carries the bearer token"
    );
    assert!(
        !rendered.contains("synthetic-password"),
        "Debug output carries the basic password"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_environment_override_wins_over_the_file_and_keeps_its_type() -> Result<(), Box<dyn StdError>>
{
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__SERVER__LISTEN", "127.0.0.1:7777"),
    )?
    .resolve()?;
    assert_eq!("127.0.0.1:7777", settings.server.listen.to_string());
    let settings = Config::from_sources(
        Some(FULL),
        &env("FERROFED__SERVER__BODY_LIMIT_BYTES", "512"),
    )?
    .resolve()?;
    assert_eq!(512, settings.server.body_limit);
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_environment_override_adds_a_credentials_section_with_no_file() -> Result<(), Box<dyn StdError>>
{
    let settings = Config::from_sources(
        None,
        &env("FERROFED__CREDENTIALS__NODE_A__BEARER_TOKEN", "synthetic"),
    )?
    .resolve()?;
    assert!(
        matches!(settings.credentials.get("node_a"), Some(Scheme::Bearer(_))),
        "node_a should resolve to a bearer scheme"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_is_read_from_its_file_sibling_and_trimmed() -> Result<(), Box<dyn StdError>> {
    let file = secret_file("synthetic-from-file\n")?;
    let text = format!(
        "[credentials.\"hospital-a\"]\nbearer_token_file = {:?}\n",
        file.path()
    );
    let settings = Config::from_sources(Some(&text), &BTreeMap::new())?.resolve()?;
    match settings.credentials.get("hospital-a") {
        Some(Scheme::Bearer(token)) => assert_eq!("synthetic-from-file", token.expose_secret()),
        other => return Err(format!("a bearer scheme: {other:?}").into()),
    }
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_set_inline_and_through_its_file_refuses_to_boot() -> Result<(), Box<dyn StdError>> {
    let file = secret_file("synthetic-from-file")?;
    let text = format!(
        "[credentials.\"hospital-a\"]\nbearer_token = \"synthetic\"\nbearer_token_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Conflict { key } if key == "credentials.hospital-a.bearer_token"),
        "{error:?}"
    );
    let file = secret_file("synthetic-password")?;
    let text = format!(
        "[credentials.\"clinic-b\"]\nuser = \"gateway\"\npassword = \"synthetic\"\npassword_file = {:?}\n",
        file.path()
    );
    let error = refusal(&text)?;
    assert!(
        matches!(&error, Error::Conflict { key } if key == "credentials.clinic-b.password"),
        "{error:?}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn an_unknown_key_refuses_to_boot_and_the_message_names_it() -> Result<(), Box<dyn StdError>> {
    for (text, key) in [
        ("[server]\nlisten_port = 8080\n", "listen_port"),
        (
            "[telemetry]\nlogged_query_parameters = [\"q\"]\n",
            "logged_query_parameters",
        ),
        ("[credentials.\"a\"]\ntoken = \"synthetic\"\n", "token"),
        ("[unknown]\nkey = 1\n", "unknown"),
    ] {
        let error = refusal(text)?;
        assert!(matches!(error, Error::Parse { .. }), "{error:?}");
        let source = std::error::Error::source(&error).map(ToString::to_string);
        assert!(
            source.as_deref().is_some_and(|text| text.contains(key)),
            "the refusal names {key}: {source:?}"
        );
    }
    Ok(())
}

#[test]
fn an_unknown_key_set_through_the_environment_refuses_to_boot() {
    let outcome = Config::from_sources(None, &env("FERROFED__SERVER__LISTEN_PORT", "8080"));
    assert!(matches!(outcome, Err(Error::Parse { .. })), "{outcome:?}");
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_bad_value_refuses_to_boot_naming_its_key() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[server]\nlisten = \"not an address\"\n")?;
    assert!(
        matches!(&error, Error::Listen { key, .. } if key == "server.listen"),
        "{error:?}"
    );
    for key in [
        "request_timeout_ms",
        "shutdown_timeout_ms",
        "body_limit_bytes",
    ] {
        let error = refusal(&format!("[server]\n{key} = 0\n"))?;
        assert!(
            matches!(&error, Error::Zero { key: named } if named == &format!("server.{key}")),
            "{error:?}"
        );
    }
    let error = refusal("[server]\nrequest_timeout_ms = \"thirty\"\n")?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let error = refusal("[telemetry]\nformat = \"xml\"\n")?;
    assert!(matches!(error, Error::Parse { .. }), "{error:?}");
    let error = refusal("[telemetry]\nfilter = \"info,=,,\"\n")?;
    assert!(matches!(error, Error::Filter { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_credentials_section_must_name_exactly_one_complete_scheme() -> Result<(), Box<dyn StdError>> {
    let error = refusal("[credentials.\"a\"]\nbearer_token = \"t\"\nuser = \"u\"\n")?;
    assert!(matches!(error, Error::Scheme { .. }), "{error:?}");
    let error = refusal("[credentials.\"a\"]\nuser = \"u\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.password"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\npassword = \"p\"\n")?;
    assert!(
        matches!(&error, Error::Missing { key } if key == "credentials.a.user"),
        "{error:?}"
    );
    let error = refusal("[credentials.\"a\"]\n")?;
    assert!(matches!(error, Error::NoScheme { .. }), "{error:?}");
    let error = refusal("[credentials.\"node a\"]\nbearer_token = \"t\"\n")?;
    assert!(matches!(error, Error::EndpointId { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_secret_file_that_cannot_be_read_or_is_empty_refuses_to_boot() -> Result<(), Box<dyn StdError>>
{
    let error =
        refusal("[credentials.\"a\"]\nbearer_token_file = \"/nonexistent/ferrofed/token\"\n")?;
    assert!(
        matches!(&error, Error::Secret { key, .. } if key == "credentials.a.bearer_token_file"),
        "{error:?}"
    );
    let file = secret_file("  \n")?;
    let error = refusal(&format!(
        "[credentials.\"a\"]\nbearer_token_file = {:?}\n",
        file.path()
    ))?;
    assert!(matches!(error, Error::EmptySecret { .. }), "{error:?}");
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_refusal_never_quotes_a_secret() -> Result<(), Box<dyn StdError>> {
    let error =
        refusal("[credentials.\"a\"]\nbearer_token = \"synthetic-secret\"\nuser = \"u\"\n")?;
    let mut rendered = error.to_string();
    let mut cause = std::error::Error::source(&error);
    while let Some(source) = cause {
        rendered.push_str(&source.to_string());
        cause = source.source();
    }
    assert!(!rendered.contains("synthetic-secret"), "{rendered}");
    Ok(())
}
