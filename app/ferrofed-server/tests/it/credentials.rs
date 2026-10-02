// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials of each endpoint, through the real configuration
//! path: a credential the `Authorization` header cannot carry refuses `serve`
//! and `config check`, naming its key and never its value, and a valid one
//! reaches its node on every request.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::io::Write as _;

use ferrofed_server::EXIT_CONFIG;
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
