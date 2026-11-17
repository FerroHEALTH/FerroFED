// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `aud` and `typ` of a plain `oauth` grant's client assertion, which
//! the grant's security profile fixes and no setting chooses: the token
//! endpoint under `typ` `JWT` (SMART App Launch 2.2.0, Client
//! Authentication: Asymmetric: `aud` is "The FHIR authorization server's
//! 'token URL'" and `typ` is "Fixed value: JWT"). The FAPI 2.0 grant's
//! issuer audience is `fapi2`'s.
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
use ferrohealth_smart::client::{ClientCredentials, Endpoint};
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

/// The `typ` header of the one client assertion `endpoint` received.
fn assertion_typ(endpoint: &TokenEndpoint) -> Result<Option<String>, Box<dyn Error>> {
    let forms = endpoint.issuance().forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    let assertion = form
        .iter()
        .find(|(key, _)| key == "client_assertion")
        .map(|(_, value)| value.as_str())
        .ok_or("an assertion")?;
    Ok(jsonwebtoken::decode_header(assertion)?.typ)
}

/// A plain grant at a token endpoint names that endpoint as its assertion's
/// `aud`, under `typ` `JWT` (SMART App Launch 2.2.0, Client Authentication:
/// Asymmetric).
// conformance: CP-17
#[tokio::test]
async fn a_plain_grant_names_the_token_endpoint_under_jwt() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    let keys = trusted_ring(&endpoint)?;
    let grant = grant(Endpoint::Token(Url::parse(&endpoint.token_url())?), keys);
    endpoint.expect_assertion(Algorithm::ES384, &endpoint.token_url());
    UpstreamToken::new(grant)?.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.issuance().verdicts());
    assert_eq!(Some("JWT".to_owned()), assertion_typ(&endpoint)?);
    Ok(())
}

/// A plain grant that discovers its token endpoint from the issuer still
/// names the token endpoint, never the issuer: only the FAPI 2.0 profile
/// sends the issuer (SMART App Launch 2.2.0, Client Authentication:
/// Asymmetric).
// conformance: CP-17
#[tokio::test]
async fn a_plain_grant_at_an_issuer_names_its_token_endpoint() -> TestResult {
    let server = AuthorizationServer::plain(CLIENT_ID, Some(300)).await;
    let endpoint = server.endpoint();
    let keys = trusted_ring(endpoint)?;
    endpoint.expect_assertion(Algorithm::ES384, &endpoint.token_url());
    let grant = grant(Endpoint::Issuer(server.issuer()), keys);
    UpstreamToken::new(grant)?.credentials().await?;
    assert_eq!(vec![Verdict::Issued], endpoint.issuance().verdicts());
    assert_eq!(Some("JWT".to_owned()), assertion_typ(endpoint)?);
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
