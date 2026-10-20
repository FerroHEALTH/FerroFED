// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An issuer's key set read from the `jwks_uri` its RFC 8414 metadata names,
//! or from a file (CP-17 inbound half; RFC 7517 §5), by
//! `ferrohealth-smart`: the set is cached, a rotation is picked up by one
//! read, the read is rate-limited, and a set that cannot be had fails closed
//! with a `503`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::auth::{AuthSettings, IssuerSettings, Verification};
use ferrofed_testkit::issuer::{Claims, Issuer, JWKS_PATH, METADATA_PATH};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::unreachable;
use ferrohealth_smart::resource::Settings as KeySetSettings;
use jsonwebtoken::Algorithm;
use wiremock::ResponseTemplate;
use wiremock::matchers::{method, path};

use super::{Gateway, TestResult, assert_admitted, assert_refused, bearing, query};
use crate::support::AUDIENCE;

/// The issuer identifier of the issuer these tests read from a file.
const ROTATING: &str = "https://rotating-issuer.example.test";

/// `[auth]` trusting `issuer`, its key set read from `file` when one is
/// given and from its metadata otherwise, read again at most every
/// `refresh`.
fn trusting(issuer: &str, file: Option<PathBuf>, refresh: Duration) -> AuthSettings {
    let mut settings = KeySetSettings::new(issuer, AUDIENCE);
    settings.jwks_file = file;
    settings.jwks_refresh_interval = refresh;
    AuthSettings {
        audience: vec![AUDIENCE.to_owned()],
        issuers: vec![IssuerSettings::new(
            issuer,
            Verification::KeySet(Box::new(settings)),
        )],
        ..AuthSettings::default()
    }
}

/// How many times `server` was asked for its key set.
async fn fetches(server: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(server
        .received_requests()
        .await
        .ok_or("recording is on")?
        .iter()
        .filter(|request| request.url.path() == JWKS_PATH)
        .count())
}

/// A token of `issuer` with the default claims.
fn token(issuer: &Issuer) -> Result<String, Box<dyn Error>> {
    Ok(issuer.mint(&Claims::new(issuer.name(), AUDIENCE))?)
}

/// CP-17, inbound half: a token verifies under the key set fetched from its
/// issuer's `jwks_uri`, fetched once for many tokens.
// conformance: CP-17
#[tokio::test]
async fn a_key_set_is_fetched_once_and_verifies_every_token() -> TestResult {
    let (issuer, keys) = Issuer::at().await?;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_secs(30))).await?;
    for _ in 0..3 {
        assert_admitted(&gateway, bearing(query()?, &token(&issuer)?)?).await?;
    }
    assert_eq!(1, fetches(&keys).await?, "the set is cached");
    Ok(())
}

/// CP-17, inbound half: an ES384 key verifies its tokens as an ES256 one
/// does.
// conformance: CP-17
#[tokio::test]
async fn an_es384_key_verifies_its_tokens() -> TestResult {
    let (mut issuer, keys) = Issuer::at().await?;
    issuer.add_key("k384", Algorithm::ES384)?;
    issuer.retire_old_keys();
    issuer.publish(&keys).await?;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_secs(30))).await?;
    assert_admitted(&gateway, bearing(query()?, &token(&issuer)?)?).await
}

/// CP-17, inbound half: after a rotation, a token naming the new key fetches
/// the set once more and verifies.
// conformance: CP-17
#[tokio::test]
async fn a_rotated_key_is_picked_up_by_one_refetch() -> TestResult {
    let (mut issuer, keys) = Issuer::at().await?;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_millis(1))).await?;
    assert_admitted(&gateway, bearing(query()?, &token(&issuer)?)?).await?;
    issuer.add_key("k2", Algorithm::ES256)?;
    issuer.publish(&keys).await?;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_admitted(&gateway, bearing(query()?, &token(&issuer)?)?).await?;
    assert_eq!(1, fetches(&keys).await?, "one refetch after the rotation");
    Ok(())
}

/// CP-17, inbound half: tokens naming a key the set does not hold refetch it
/// at most once per refetch interval, and are refused.
// conformance: CP-17
#[tokio::test]
async fn an_unknown_key_refetches_at_most_once_per_interval() -> TestResult {
    let (issuer, keys) = Issuer::at().await?;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_secs(3_600))).await?;
    let mut header = issuer.header();
    header.kid = Some(String::from("k-unknown"));
    for _ in 0..3 {
        let forged = issuer.mint_with(&header, &Claims::new(issuer.name(), AUDIENCE))?;
        assert_refused(&gateway, bearing(query()?, &forged)?, Refusal::Invalid).await?;
    }
    assert_eq!(1, fetches(&keys).await?, "no refetch inside the interval");
    Ok(())
}

/// CP-17, inbound half: an issuer whose metadata and key set cannot be
/// fetched is a `503`, never a pass.
// conformance: CP-17
#[tokio::test]
async fn an_unreachable_key_set_is_503() -> TestResult {
    let issuer = Issuer::new(unreachable::BASE.trim_end_matches('/'))?;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_secs(30))).await?;
    assert_refused(
        &gateway,
        bearing(query()?, &token(&issuer)?)?,
        Refusal::Unavailable,
    )
    .await
}

/// CP-17, inbound half: a key set answered with an error is a `503`.
// conformance: CP-17
#[tokio::test]
async fn a_key_set_answered_with_an_error_is_503() -> TestResult {
    let keys = Server::start().await;
    let issuer = Issuer::new(keys.uri())?;
    let metadata = serde_json::json!({
        "issuer": issuer.name(),
        "jwks_uri": format!("{}{JWKS_PATH}", keys.uri()),
    });
    wiremock::Mock::given(method("GET"))
        .and(path(METADATA_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_json(metadata))
        .mount(&keys)
        .await;
    wiremock::Mock::given(method("GET"))
        .and(path(JWKS_PATH))
        .respond_with(ResponseTemplate::new(500))
        .mount(&keys)
        .await;
    let gateway = Gateway::with(trusting(issuer.name(), None, Duration::from_secs(30))).await?;
    assert_refused(
        &gateway,
        bearing(query()?, &token(&issuer)?)?,
        Refusal::Unavailable,
    )
    .await
}

/// CP-17, inbound half: a key set read from a file verifies, and a missing
/// file is a `503`.
// conformance: CP-17
#[tokio::test]
async fn a_key_set_file_verifies_and_a_missing_one_is_503() -> TestResult {
    let issuer = Issuer::new(ROTATING)?;
    let dir = tempfile::tempdir()?;
    let file = dir.path().join("jwks.json");
    std::fs::write(&file, issuer.jwks_json()?)?;
    let gateway = Gateway::with(trusting(ROTATING, Some(file), Duration::from_secs(30))).await?;
    assert_admitted(&gateway, bearing(query()?, &token(&issuer)?)?).await?;
    let missing = dir.path().join("absent.json");
    let gateway = Gateway::with(trusting(ROTATING, Some(missing), Duration::from_secs(30))).await?;
    assert_refused(
        &gateway,
        bearing(query()?, &token(&issuer)?)?,
        Refusal::Unavailable,
    )
    .await
}
