// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `aud` of the `oauth` grant's client assertion: the token endpoint
//! by default, or the authorization server's issuer identifier, the two
//! values RFC 7523 §3 admits, the second the one the FAPI 2.0 Security
//! Profile requires (§5.3.2.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::upstream::UpstreamToken;
use ferrofed_testkit::fapi::AuthorizationServer;
use ferrofed_testkit::issuance::Verdict;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::keys::KeyRing;
use ferrohealth_smart::client::{AssertionAudience, ClientCredentials, Endpoint};
use jsonwebtoken::Algorithm;
use openehr_its::rest::client::CredentialsProvider as _;
use url::Url;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// A fresh ES384 ring that `endpoint` trusts.
fn trusted_ring(endpoint: &TokenEndpoint) -> Result<Arc<KeyRing>, Box<dyn Error>> {
    let key = AssertionKey::from_pem(
        oauth::Curve::P384.pem()?.as_bytes(),
        Some(Algorithm::ES384),
        None,
    )?;
    let keys = Arc::new(KeyRing::new(key));
    endpoint.trust(serde_json::from_value(keys.published())?);
    Ok(keys)
}

/// The `private_key_jwt` grant at `endpoint` over `keys`.
fn grant(endpoint: Endpoint, keys: Arc<KeyRing>) -> ClientCredentials {
    let mut grant = ClientCredentials::with_key_ring(endpoint, CLIENT_ID.to_owned(), keys);
    grant.scope = Some("system/aql-*.s".to_owned());
    grant.timeout = Duration::from_secs(5);
    grant
}

/// By default every assertion names the token endpoint (RFC 7523 §3).
// conformance: CP-17
#[tokio::test]
async fn the_token_endpoint_is_the_default_audience() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    let keys = trusted_ring(&endpoint)?;
    let grant = grant(Endpoint::Token(Url::parse(&endpoint.token_url())?), keys);
    assert_eq!(AssertionAudience::TokenEndpoint, grant.assertion_audience);
    endpoint.expect_assertion(Algorithm::ES384, &endpoint.token_url());
    UpstreamToken::new(grant)?.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.issuance().verdicts());
    Ok(())
}

/// A grant naming the issuer as the audience signs every assertion with the
/// issuer, one string, as its `aud`, and a server that takes only its
/// issuer issues the token (RFC 7523 §3; FAPI 2.0 §5.3.2.1).
// conformance: CP-17
#[tokio::test]
async fn the_issuer_audience_names_the_issuer() -> TestResult {
    let server = AuthorizationServer::plain(CLIENT_ID, Some(300)).await;
    let endpoint = server.endpoint();
    let keys = trusted_ring(endpoint)?;
    endpoint.expect_assertion(Algorithm::ES384, &server.issuer());
    let mut grant = grant(Endpoint::Issuer(server.issuer()), keys);
    grant.assertion_audience = AssertionAudience::Issuer;
    UpstreamToken::new(grant)?.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.issuance().verdicts());
    Ok(())
}

/// A server that takes only its issuer refuses an assertion naming the
/// token endpoint, and the gateway reports the refusal.
#[tokio::test]
async fn a_server_taking_only_its_issuer_refuses_the_token_endpoint() -> TestResult {
    let server = AuthorizationServer::plain(CLIENT_ID, Some(300)).await;
    let endpoint = server.endpoint();
    let keys = trusted_ring(endpoint)?;
    endpoint.expect_assertion(Algorithm::ES384, &server.issuer());
    let grant = grant(Endpoint::Issuer(server.issuer()), keys);
    assert!(UpstreamToken::new(grant)?.credentials().await.is_err());
    assert_eq!(1, endpoint.issuance().verdicts().len());
    assert_eq!(0, endpoint.issuance().issued());
    Ok(())
}
