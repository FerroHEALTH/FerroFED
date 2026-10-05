// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `aud` of the `oauth2` grant's client assertion: the token endpoint
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

use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{AssertionAudience, Grant, Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::oauth::{self, TokenEndpoint, Verdict};
use jsonwebtoken::Algorithm;
use oauth_server_metadata::Issuer;
use openehr_its::rest::client::{CredentialsProvider as _, ReqwestTransport};
use secrecy::SecretString;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The issuer identifier of the node's authorization server.
const ISSUER: &str = "https://as.cdr-a.example.org";

/// A token endpoint trusting a fresh ES384 ring, and the provider of
/// `grant` signing with it.
async fn provider_of(
    grant: impl FnOnce(&TokenEndpoint) -> Result<Grant, Box<dyn Error>>,
) -> Result<(TokenEndpoint, ClientCredentials<ReqwestTransport>), Box<dyn Error>> {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    let key = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    let keys = Arc::new(KeyRing::new(
        key,
        None,
        Duration::ZERO,
        Arc::new(SystemClock),
    )?);
    endpoint.trust(keys.published());
    let provider = ClientCredentials::new(
        EndpointId::new("node-a-pub")?,
        grant(&endpoint)?,
        keys,
        (Duration::from_secs(300), Duration::from_secs(5)),
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
        Arc::new(SystemClock),
    );
    Ok((endpoint, provider))
}

/// The grant at `endpoint`'s token URL.
fn grant(endpoint: &TokenEndpoint) -> Result<Grant, Box<dyn Error>> {
    Ok(Grant::new(
        &SecretUrl::new(endpoint.token_url()),
        CLIENT_ID,
        Scope::parse("system/aql-*.s")?,
    )?)
}

/// By default every assertion names the token endpoint (RFC 7523 §3).
// conformance: CP-17
#[tokio::test]
async fn the_token_endpoint_is_the_default_audience() -> TestResult {
    let (endpoint, provider) = provider_of(grant).await?;
    assert_eq!(
        &AssertionAudience::TokenEndpoint,
        provider.grant().assertion_audience()
    );
    provider.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.verdicts());
    Ok(())
}

/// A grant naming the issuer as the audience signs every assertion with the
/// issuer, one string, as its `aud`, and a server that takes only its
/// issuer issues the token (RFC 7523 §3; FAPI 2.0 §5.3.2.1).
// conformance: CP-17
#[tokio::test]
async fn the_issuer_audience_names_the_issuer() -> TestResult {
    let (endpoint, provider) =
        provider_of(|endpoint| Ok(grant(endpoint)?.with_issuer_audience(Issuer::parse(ISSUER)?)))
            .await?;
    endpoint.expect_assertion(Algorithm::ES384, ISSUER);
    provider.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.verdicts());
    assert_eq!(ISSUER, provider.grant().assertion_aud());
    Ok(())
}

/// A server that takes only its issuer refuses an assertion naming the
/// token endpoint, and the gateway reports the refusal.
#[tokio::test]
async fn a_server_taking_only_its_issuer_refuses_the_token_endpoint() -> TestResult {
    let (endpoint, provider) = provider_of(grant).await?;
    endpoint.expect_assertion(Algorithm::ES384, ISSUER);
    assert!(provider.credentials().await.is_err());
    assert_eq!(1, endpoint.verdicts().len());
    assert_eq!(0, endpoint.issued());
    Ok(())
}
