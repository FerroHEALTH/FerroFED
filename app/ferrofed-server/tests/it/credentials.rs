// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials of each endpoint, through the real configuration
//! path: a credential the `Authorization` header cannot carry refuses `serve`
//! and `config check`, naming its key and never its value, and a valid one
//! reaches its node on every request.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::io::Write as _;

use ferrofed_registry::error::IdError;
use ferrofed_registry::id::{EndpointId, MAX_ID_LEN};
use ferrofed_server::EXIT_CONFIG;
use ferrofed_server::config::Config;
use ferrofed_server::config::settings::Scheme;
use http::StatusCode;

use crate::facade::{body, gateway, node_answering, post, registry};
use crate::run::binary;
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The two halves of every refused secret: no part of either may reach the
/// refusal.
const HALVES: [&str; 2] = ["Qz7left", "Qz7right"];

/// Runs `serve` and `config check` on `toml` and asserts both refuse it with
/// `EX_CONFIG`, naming `key` and quoting no part of the secret.
fn refuses_naming(toml: &str, key: &str) -> TestResult {
    for job in [&["serve"][..], &["config", "check"][..]] {
        let output = binary(job, toml)?;
        assert_eq!(
            Some(i32::from(EXIT_CONFIG)),
            output.status.code(),
            "{job:?} refuses {key}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(key), "{job:?} names {key}: {stderr}");
        assert!(
            stderr.contains("Authorization") || stderr.contains("RFC 7617"),
            "{job:?} says why: {stderr}"
        );
        assert!(
            !stderr.contains("Qz7"),
            "{job:?} quotes no part of the secret: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn a_bearer_token_holding_a_newline_refuses_to_boot() -> TestResult {
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"hospital-a\"]\nbearer_token = \"{}\\n{}\"\n",
        HALVES[0], HALVES[1]
    );
    refuses_naming(&toml, "credentials.hospital-a.bearer_token")
}

#[test]
fn a_basic_password_holding_a_newline_refuses_to_boot() -> TestResult {
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"clinic-b\"]\nuser = \"gateway\"\npassword = \"{}\\n{}\"\n",
        HALVES[0], HALVES[1]
    );
    refuses_naming(&toml, "credentials.clinic-b.password")
}

#[test]
fn a_basic_user_holding_a_colon_refuses_to_boot() -> TestResult {
    let toml = "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"clinic-b\"]\nuser = \"gate:way\"\npassword = \"Qz7left\"\n";
    refuses_naming(toml, "credentials.clinic-b.user")
}

#[test]
fn a_secret_file_holding_a_control_character_refuses_to_boot() -> TestResult {
    let mut token = tempfile::NamedTempFile::new()?;
    writeln!(token, "{}\u{1}{}", HALVES[0], HALVES[1])?;
    let path = toml::Value::String(token.path().display().to_string());
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"hospital-a\"]\nbearer_token_file = {path}\n"
    );
    refuses_naming(&toml, "credentials.hospital-a.bearer_token_file")?;

    let mut password = tempfile::NamedTempFile::new()?;
    writeln!(password, "{}\u{7f}{}", HALVES[0], HALVES[1])?;
    let path = toml::Value::String(password.path().display().to_string());
    let toml = format!(
        "[server]\nlisten = \"127.0.0.1:1\"\n[credentials.\"clinic-b\"]\nuser = \"gateway\"\npassword_file = {path}\n"
    );
    refuses_naming(&toml, "credentials.clinic-b.password_file")
}

/// A credentials key is held to the registry's endpoint id rule whether or
/// not a registry is configured, and the refusal names the key.
#[test]
fn a_credentials_key_follows_the_registry_endpoint_id_rule() -> TestResult {
    let too_long = "a".repeat(MAX_ID_LEN + 1);
    for key in [
        "",
        "node a",
        "-node",
        "node:a",
        "n\u{e9}",
        too_long.as_str(),
    ] {
        let toml = format!("[credentials.\"{key}\"]\nbearer_token = \"t\"\n");
        let Err(error) = Config::from_sources(Some(&toml), &BTreeMap::new())?.resolve() else {
            return Err(format!("{key:?} was accepted").into());
        };
        assert!(
            matches!(
                &error,
                ferrofed_server::config::error::Error::EndpointId {
                    key: given,
                    source: IdError::Empty { .. }
                        | IdError::Malformed { .. }
                        | IdError::TooLong { .. },
                } if given == key
            ),
            "{key:?}: {error:?}"
        );
    }
    let output = binary(
        &["config", "check"],
        "[credentials.\"node:a\"]\nbearer_token = \"t\"\n",
    )?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), output.status.code());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("node:a"), "names the key: {stderr}");

    let longest = "a".repeat(MAX_ID_LEN);
    for key in ["hospital-a.query", "node_a", "9b", longest.as_str()] {
        let toml = format!("[credentials.\"{key}\"]\nbearer_token = \"t\"\n");
        let settings = Config::from_sources(Some(&toml), &BTreeMap::new())?.resolve()?;
        assert!(
            settings.credentials.contains_key(&EndpointId::new(key)?),
            "{key:?} is an endpoint id"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_valid_credential_reaches_its_node_on_every_request() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "",
        "[credentials.\"node-a-pub\"]\nbearer_token = \"synthetic-token\"\n\n\
         [credentials.\"node-b-pub\"]\nuser = \"gateway\"\npassword = \"synthetic-pw\"\n",
    )?;
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (server, expected) in [
        (&a, "Bearer synthetic-token"),
        // The base64 of `gateway:synthetic-pw` (RFC 7617 §2).
        (&b, "Basic Z2F0ZXdheTpzeW50aGV0aWMtcHc="),
    ] {
        let requests = server.received_requests().await.ok_or("recording is on")?;
        assert_eq!(1, requests.len(), "each node is asked once");
        for request in requests {
            let sent = request
                .headers
                .get(http::header::AUTHORIZATION)
                .map(http::HeaderValue::to_str)
                .transpose()?;
            assert_eq!(Some(expected), sent, "the node receives its credential");
        }
    }
    Ok(())
}

/// A credential read from a `_file` sibling travels as a redacting secret
/// from the configuration to the node client, and still reaches its node as
/// written.
#[tokio::test]
async fn a_credential_from_a_file_reaches_its_node_as_written() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let token = dir.path().join("token");
    std::fs::write(&token, "synthetic-file-token\n")?;
    let password = dir.path().join("password");
    std::fs::write(&password, "synthetic-file-pw\n")?;
    let token = toml::Value::String(token.display().to_string());
    let password = toml::Value::String(password.display().to_string());
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "",
        &format!(
            "[credentials.\"node-a-pub\"]\nbearer_token_file = {token}\n\n\
             [credentials.\"node-b-pub\"]\nuser = \"gateway\"\npassword_file = {password}\n"
        ),
    )?;
    let aql = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

    let (status, text) = call(app, post(body(aql)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (server, expected) in [
        (&a, "Bearer synthetic-file-token"),
        // The base64 of `gateway:synthetic-file-pw` (RFC 7617 §2).
        (&b, "Basic Z2F0ZXdheTpzeW50aGV0aWMtZmlsZS1wdw=="),
    ] {
        let requests = server.received_requests().await.ok_or("recording is on")?;
        assert_eq!(1, requests.len(), "each node is asked once");
        for request in requests {
            let sent = request
                .headers
                .get(http::header::AUTHORIZATION)
                .map(http::HeaderValue::to_str)
                .transpose()?;
            assert_eq!(Some(expected), sent, "the node receives its credential");
        }
    }
    Ok(())
}

/// A secret written inline is accepted under the production profile, by
/// `config check` and by the configuration path `serve` reads, as the book's
/// configuration page and the README say.
#[test]
fn an_inline_secret_is_accepted_under_the_production_profile() -> TestResult {
    let toml = "profile = \"production\"\n\n[server]\nlisten = \"127.0.0.1:1\"\n\n\
                [credentials.\"hospital-a\"]\nbearer_token = \"synthetic-inline-token\"\n\n\
                [credentials.\"clinic-b\"]\nuser = \"gateway\"\npassword = \"synthetic-inline-pw\"\n";
    let output = binary(&["config", "check"], toml)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        Some(0),
        output.status.code(),
        "config check accepts it: {stderr}"
    );
    assert!(
        !stderr.contains("synthetic-inline"),
        "config check quotes no secret"
    );

    let settings = Config::from_sources(Some(toml), &BTreeMap::new())?.resolve()?;
    assert!(
        matches!(
            settings.credentials.get(&EndpointId::new("hospital-a")?),
            Some(Scheme::Bearer(token)) if token.expose() == "synthetic-inline-token"
        ),
        "the inline bearer token resolves"
    );
    assert!(
        matches!(
            settings.credentials.get(&EndpointId::new("clinic-b")?),
            Some(Scheme::Basic { .. })
        ),
        "the inline user and password resolve"
    );
    Ok(())
}
