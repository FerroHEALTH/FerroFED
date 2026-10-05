// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The bearer token itself (CP-17, inbound half; §13.1, N25): every failure
//! of RFC 6750, RFC 7519, RFC 8725 and RFC 9068 is a `401` that reaches no
//! node, and an admitted token never travels on (RFC 9700 §2.3).
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use axum::body::Body;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_testkit::issuer::Issuer;
use http::{Request, StatusCode, header};
use jsonwebtoken::{Algorithm, EncodingKey, Header};

use super::{
    Gateway, TestResult, assert_admitted, assert_refused, bearing, claims, minted, query, sent,
};
use crate::facade::wire;
use crate::support::{self, AUDIENCE, ISSUER};

/// The current time, in seconds since the epoch.
fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// CP-17, inbound half: a request with no credential is a `401` whose
/// challenge names no error (RFC 6750 §3.1), and reaches no node.
// conformance: CP-17
#[tokio::test]
async fn a_query_without_a_token_is_401_and_reaches_no_node() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    assert_refused(&gateway, query()?, Refusal::Missing).await
}

/// CP-17, inbound half: a credential in another scheme is no bearer token.
// conformance: CP-17
#[tokio::test]
async fn a_basic_credential_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut request = query()?;
    request.headers_mut().insert(
        header::AUTHORIZATION,
        "Basic c3ludGhldGljOnN5bnRoZXRpYw==".parse()?,
    );
    assert_refused(&gateway, request, Refusal::Malformed).await
}

/// CP-17, inbound half: two `Authorization` fields are refused, never one of
/// them chosen.
// conformance: CP-17
#[tokio::test]
async fn two_authorization_fields_are_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut request = query()?;
    for _ in 0..2 {
        request
            .headers_mut()
            .append(header::AUTHORIZATION, support::bearer()?.parse()?);
    }
    assert_refused(&gateway, request, Refusal::Malformed).await
}

/// CP-17, inbound half: a token past its `exp`, beyond the clock skew, is a
/// `401` (RFC 7519 §4.1.4).
// conformance: CP-17
#[tokio::test]
async fn an_expired_token_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut expired = claims();
    expired.exp = now() - 3_600;
    expired.iat = now() - 7_200;
    let request = bearing(query()?, &minted(&expired)?)?;
    assert_refused(&gateway, request, Refusal::Expired).await
}

/// CP-17, inbound half: a token that expired inside the configured clock
/// skew is still admitted (RFC 7519 §4.1.4).
// conformance: CP-17
#[tokio::test]
async fn a_token_expired_within_the_clock_skew_is_admitted() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut lately = claims();
    lately.exp = now() - 20;
    let request = bearing(query()?, &minted(&lately)?)?;
    assert_admitted(&gateway, request).await
}

/// CP-17, inbound half: a token before its `nbf`, beyond the clock skew, is
/// a `401` (RFC 7519 §4.1.5).
// conformance: CP-17
#[tokio::test]
async fn a_token_not_yet_valid_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut early = claims();
    early.nbf = Some(now() + 3_600);
    let request = bearing(query()?, &minted(&early)?)?;
    assert_refused(&gateway, request, Refusal::NotYetValid).await
}

/// CP-17, inbound half: a token issued for another audience is a `401`
/// (RFC 9068 §4).
// conformance: CP-17
#[tokio::test]
async fn a_token_for_another_audience_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut elsewhere = claims();
    elsewhere.aud = String::from("urn:example:another-resource-server");
    let request = bearing(query()?, &minted(&elsewhere)?)?;
    assert_refused(&gateway, request, Refusal::Audience).await
}

/// CP-17, inbound half: a token from an issuer not on the trust list is a
/// `401`, however well formed.
// conformance: CP-17
#[tokio::test]
async fn a_token_from_an_untrusted_issuer_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let stranger = Issuer::new("https://untrusted.example.test")?;
    let token = stranger.mint(&ferrofed_testkit::issuer::Claims::new(
        stranger.name(),
        AUDIENCE,
    ))?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Issuer).await
}

/// CP-17, inbound half: a token naming the trusted issuer and its key, but
/// signed with another key, is a `401` (RFC 7515 §5.2).
// conformance: CP-17
#[tokio::test]
async fn a_token_signed_by_another_key_under_a_trusted_name_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let impostor = Issuer::new(ISSUER)?;
    let token = impostor.mint(&claims())?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Signature).await
}

/// CP-17, inbound half: a token naming a key its issuer does not publish is
/// a `401`.
// conformance: CP-17
#[tokio::test]
async fn a_token_naming_an_unpublished_key_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut header = support::issuer().header();
    header.kid = Some(String::from("k9"));
    let token = support::issuer().mint_with(&header, &claims())?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Key).await
}

/// CP-17, inbound half: an HMAC-signed token is a `401`, never verified
/// with a published key as its secret (RFC 8725 §3.1, §3.2).
// conformance: CP-17
#[tokio::test]
async fn an_hmac_token_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(String::from("k1"));
    header.typ = Some(String::from("at+jwt"));
    let token = jsonwebtoken::encode(
        &header,
        &claims(),
        &EncodingKey::from_secret(support::issuer().jwks_json()?.as_bytes()),
    )?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Algorithm).await
}

/// CP-17, inbound half: an unsigned token, `alg` `none`, is a `401` (RFC
/// 8725 §3.1).
// conformance: CP-17
#[tokio::test]
async fn an_unsigned_token_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let signed = minted(&claims())?;
    let payload = signed.split('.').nth(1).ok_or("a JWS has a payload")?;
    // The base64url of {"alg":"none","typ":"at+jwt"}.
    let token = format!("eyJhbGciOiJub25lIiwidHlwIjoiYXQrand0In0.{payload}.");
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Malformed).await
}

/// CP-17, inbound half: a token not typed `at+jwt` is a `401` (RFC 9068 §4).
// conformance: CP-17
#[tokio::test]
async fn a_token_not_typed_as_an_access_token_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let mut header = support::issuer().header();
    header.typ = Some(String::from("JWT"));
    let token = support::issuer().mint_with(&header, &claims())?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Type).await
}

/// CP-17, inbound half: a token without a claim RFC 9068 §2.2 requires is a
/// `401`.
// conformance: CP-17
#[tokio::test]
async fn a_token_without_its_client_id_is_401() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let now = now();
    let payload = format!(
        r#"{{"iss":"{ISSUER}","sub":"synthetic-caller","aud":"{AUDIENCE}","exp":{},"iat":{now},"jti":"synthetic-jti","scope":"user/aql-*.s"}}"#,
        now + 600
    );
    let token = support::issuer().sign(&support::issuer().header(), &payload)?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Malformed).await
}

/// CP-17, inbound half: the caller's own token reaches no node of a
/// federated query; each node sees its own onward credential, or none (RFC
/// 9700 §2.3).
// conformance: CP-17
#[tokio::test]
async fn an_admitted_token_reaches_no_node() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = minted(&claims())?;
    assert_admitted(&gateway, bearing(query()?, &token)?).await?;
    for node in [&gateway.a, &gateway.b] {
        let captured = wire(node).await?;
        assert!(!captured.is_empty(), "the node was asked");
        assert!(
            !captured.contains(&token),
            "the caller's token reached a node: {captured}"
        );
        assert!(
            !captured.contains_ignoring_ascii_case("bearer"),
            "no bearer credential reached a node configured with none: {captured}"
        );
    }
    Ok(())
}

/// §7a.2, §13.1: `OPTIONS {base}/` answers `401` without a token, and `200`
/// to an authenticated caller, who needs no scope and no purpose of use.
// conformance: CP-17 CP-23
#[tokio::test]
async fn options_root_is_behind_the_gate() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let options = || Request::options("/").body(Body::empty());
    assert_refused(&gateway, options()?, Refusal::Missing).await?;
    let mut bare = claims();
    bare.scope = None;
    bare.extensions = None;
    let (status, _, text) = sent(&gateway.app, bearing(options()?, &minted(&bare)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(())
}

/// The health family and `GET {base}/` describe the process, and answer
/// without a token; an unknown path outside the ITS-REST surface is a `404`.
#[tokio::test]
async fn the_health_family_and_the_root_need_no_token() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    for path in ["/", "/health", "/health/dependencies"] {
        let (status, _, text) = sent(&gateway.app, Request::get(path).body(Body::empty())?).await?;
        assert_eq!(StatusCode::OK, status, "{path}: {text}");
    }
    for path in ["/v1", "//v1/query/aql", "/V1/query/aql", "/elsewhere"] {
        let (status, _, text) = sent(&gateway.app, Request::get(path).body(Body::empty())?).await?;
        assert_eq!(StatusCode::NOT_FOUND, status, "{path}: {text}");
    }
    gateway.nobody_asked().await
}
