// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Mitz reached with the OAuth 2.0 client credentials grant of
//! `ferrohealth-smart` (RFC 6749 §4.4), `[nl_gf.mitz.credentials.oauth]`,
//! through the real configuration path against the harness token endpoint:
//! each question carries the token as a bearer token (RFC 6750 §2.1), the
//! token is reused while it is fresh, and a `401` gets the question once
//! more with a fresh token (RFC 6750 §3.1).

use std::error::Error;
use std::path::Path;

use ferrofed_testkit::mitz::Mitz;
use ferrofed_testkit::oauth::TokenEndpoint;
use http::StatusCode;

use super::{TestResult, URA_B, ask, consent_error, holders, metered_with, mitz_table};
use crate::facade::{PATIENT, node_answering, statuses, wire};

/// The client Mitz's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-mitz";

/// The synthetic client secret.
const SECRET: &str = "Qz7synthetic-mitz-secret";

/// The `[nl_gf.mitz]` tables asking `mitz`, authenticated by the grant at
/// `token_url` with the secret in `secret_file`.
fn tables(mitz: &str, token_url: &str, secret_file: &Path) -> String {
    let file = toml::Value::String(secret_file.display().to_string());
    format!(
        "{}\n[nl_gf.mitz.credentials.oauth]\ntoken_endpoint = \"{token_url}\"\nclient_id = \"{CLIENT_ID}\"\nclient_auth = \"client_secret_basic\"\nclient_secret_file = {file}\n",
        mitz_table(mitz, &holders())
    )
}

/// Writes [`SECRET`] to a file under `dir`.
fn secret_file(dir: &Path) -> Result<std::path::PathBuf, Box<dyn Error>> {
    let file = dir.join("mitz-client-secret");
    std::fs::write(&file, format!("{SECRET}\n"))?;
    Ok(file)
}

// conformance: CP-36
#[tokio::test]
async fn mitz_is_asked_with_the_token_the_grant_obtained_and_reused() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret(SECRET);
    let mitz = Mitz::start_requiring(endpoint.bearer()).await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let table = tables(
        &mitz.endpoint(),
        &endpoint.token_url(),
        &secret_file(dir.path())?,
    );
    let (app, _) = metered_with(dir.path(), (&a.uri(), &b.uri()), &table)?;

    for _ in 0..2 {
        let (status, text, answer) = ask(app.clone()).await?;
        assert_eq!(StatusCode::OK, status, "{text}");
        assert_eq!(None, consent_error(&text)?, "Mitz admitted the token");
        assert_eq!(
            vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
            statuses(&answer),
            "N27a: the decision of the authenticated question applies"
        );
    }
    assert!(wire(&b).await?.is_empty(), "node B receives nothing");
    assert_eq!(4, mitz.questions().await?.len(), "two holders, two queries");
    assert_eq!(
        1,
        endpoint.issued(),
        "the token is reused while it is fresh"
    );
    Ok(())
}

#[tokio::test]
async fn a_question_mitz_refuses_with_401_is_asked_once_more_with_a_fresh_token() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret(SECRET);
    let mitz = Mitz::start_requiring(endpoint.bearer()).await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let table = tables(
        &mitz.endpoint(),
        &endpoint.token_url(),
        &secret_file(dir.path())?,
    );
    let (app, _) = metered_with(dir.path(), (&a.uri(), &b.uri()), &table)?;
    let (status, text, _) = ask(app.clone()).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, endpoint.issued());

    endpoint.revoke_all();
    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        None,
        consent_error(&text)?,
        "RFC 6750 §3.1: the retry answered"
    );
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer)
    );
    assert!(endpoint.issued() >= 2, "a fresh token was obtained");
    Ok(())
}

#[tokio::test]
async fn a_grant_its_token_endpoint_refuses_leaves_every_candidate_to_its_node() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret("Qz7another-secret");
    let mitz = Mitz::start_requiring(endpoint.bearer()).await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let table = tables(
        &mitz.endpoint(),
        &endpoint.token_url(),
        &secret_file(dir.path())?,
    );
    let (app, _) = metered_with(dir.path(), (&a.uri(), &b.uri()), &table)?;

    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "N27a, §13.2.1: no consent signal, so each node is the sole gate"
    );
    let carried = consent_error(&text)?.ok_or("consent.error names the failure")?;
    assert!(!carried.contains(SECRET), "{carried}");
    assert!(
        mitz.questions().await?.is_empty(),
        "no question went untokened"
    );
    Ok(())
}
