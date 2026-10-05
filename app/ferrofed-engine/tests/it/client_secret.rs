// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client-credentials grant authenticated by a client secret, as an
//! identity service's grant is (RFC 6749 §2.3.1, §4.4; IHE IUA ITI-71
//! §3.71.4.1.2.1): the secret in the Basic scheme or the request body, the
//! scope of RFC 6749 §3.3 sent as written, the token cached until shortly
//! before it expires and dropped on a `401`, against the harness token
//! endpoint.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::grant::client_credentials::{ClientCredentials, Recipient};
use ferrofed_engine::onward::token::{ErrorCode, REFRESH_MARGIN, TokenError};
use ferrofed_engine::onward::{Clock, Grant, Scope, ScopeError, SecretMethod, SystemClock};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::oauth::TokenEndpoint;
use openehr_its::rest::client::{Credentials, CredentialsProvider, ReqwestTransport};
use secrecy::SecretString;

use crate::onward::ManualClock;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the identity service's authorization server registered the
/// gateway as, with a colon a Basic user-id cannot carry unencoded.
const CLIENT_ID: &str = "urn:oid:2.999.7:pix-consumer";

/// The synthetic secret, with characters the form encoding changes.
const SECRET: &str = "synthetic:s3cret+&%/ x";

/// The scope, held to RFC 6749 §3.3 alone.
const SCOPE: &str = "* urn:example:pix";

/// The grant at `endpoint` authenticated by [`SECRET`] sent by `method`.
fn grant(endpoint: &TokenEndpoint, method: SecretMethod) -> Result<Grant, Box<dyn Error>> {
    Ok(Grant::new(
        &SecretUrl::new(endpoint.token_url()),
        CLIENT_ID,
        Scope::tokens(SCOPE)?,
    )?
    .with_client_secret(method, SecretString::from(SECRET)))
}

/// The provider of `grant` for the PIX Manager, read by `clock`.
fn provider(
    grant: Grant,
    clock: Arc<dyn Clock>,
) -> Result<ClientCredentials<ReqwestTransport>, Box<dyn Error>> {
    Ok(ClientCredentials::by_secret(
        Recipient::Service("pixm.manager[0]".to_owned()),
        grant,
        Duration::from_secs(5),
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
        clock,
    ))
}

/// The endpoint for [`CLIENT_ID`] that takes [`SECRET`], issuing tokens that
/// live `expires_in` seconds.
async fn endpoint(expires_in: Option<u64>) -> TokenEndpoint {
    let endpoint = TokenEndpoint::start(CLIENT_ID, expires_in).await;
    endpoint.accept_client_secret(SECRET);
    endpoint.expect_scope(SCOPE);
    endpoint
}

/// The value of the form parameter `name` of the request at `index`.
fn sent(endpoint: &TokenEndpoint, index: usize, name: &str) -> Option<String> {
    endpoint.forms().get(index).and_then(|form| {
        form.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    })
}

#[tokio::test]
async fn the_secret_travels_in_the_basic_scheme_each_part_form_encoded() -> TestResult {
    let endpoint = endpoint(Some(3600)).await;
    let provider = provider(
        grant(&endpoint, SecretMethod::Basic)?,
        Arc::new(SystemClock),
    )?;
    let credentials = provider.credentials().await?;
    assert!(
        matches!(credentials, Credentials::Bearer(_)),
        "{credentials:?}"
    );
    assert_eq!(vec!["client_secret_basic"], endpoint.secret_methods());
    assert_eq!(
        Some("client_credentials".to_owned()),
        sent(&endpoint, 0, "grant_type")
    );
    assert_eq!(Some(SCOPE.to_owned()), sent(&endpoint, 0, "scope"));
    assert_eq!(
        None,
        sent(&endpoint, 0, "client_secret"),
        "IUA ITI-71 §3.71.4.1.2.1"
    );
    assert_eq!(None, sent(&endpoint, 0, "client_assertion"));
    Ok(())
}

#[tokio::test]
async fn the_secret_travels_in_the_body_for_a_server_that_takes_it_there() -> TestResult {
    let endpoint = endpoint(Some(3600)).await;
    let provider = provider(grant(&endpoint, SecretMethod::Post)?, Arc::new(SystemClock))?;
    provider.credentials().await?;
    assert_eq!(vec!["client_secret_post"], endpoint.secret_methods());
    assert_eq!(Some(CLIENT_ID.to_owned()), sent(&endpoint, 0, "client_id"));
    assert_eq!(Some(SECRET.to_owned()), sent(&endpoint, 0, "client_secret"));
    Ok(())
}

#[tokio::test]
async fn a_wrong_secret_is_a_typed_refusal_and_never_a_credential() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(3600)).await;
    endpoint.accept_client_secret("another synthetic secret");
    let provider = provider(
        grant(&endpoint, SecretMethod::Basic)?,
        Arc::new(SystemClock),
    )?;
    let refused = provider
        .credentials()
        .await
        .err()
        .ok_or("a wrong secret obtains no token")?;
    let token = Error::source(&refused)
        .and_then(|source| source.downcast_ref::<TokenError>())
        .ok_or("the cause is the token request's refusal")?;
    assert_eq!(Some(ErrorCode::InvalidClient), token.code(), "{token}");
    assert!(!format!("{refused:?} {token}").contains(SECRET));
    Ok(())
}

#[tokio::test]
async fn the_token_is_refreshed_before_it_expires() -> TestResult {
    let endpoint = endpoint(Some(120)).await;
    let clock = ManualClock::new();
    let provider = provider(grant(&endpoint, SecretMethod::Basic)?, clock.clone())?;
    let first = provider.credentials().await?;
    let cached = provider.credentials().await?;
    assert_eq!(1, endpoint.issued(), "a fresh token is reused");
    assert_eq!(first.header_value()?, cached.header_value()?);
    let fresh_for = Duration::from_secs(120)
        .checked_sub(REFRESH_MARGIN)
        .ok_or("the lifetime outlasts the margin")?;
    clock.advance(fresh_for)?;
    let refreshed = provider.credentials().await?;
    assert_eq!(2, endpoint.issued(), "a token near its end is replaced");
    assert_ne!(first.header_value()?, refreshed.header_value()?);
    Ok(())
}

#[tokio::test]
async fn a_refusal_of_the_service_drops_the_cached_token() -> TestResult {
    let endpoint = endpoint(Some(3600)).await;
    let provider = provider(
        grant(&endpoint, SecretMethod::Basic)?,
        Arc::new(SystemClock),
    )?;
    let first = provider.credentials().await?;
    provider.refused();
    let fresh = provider.credentials().await?;
    assert_eq!(2, endpoint.issued());
    assert_ne!(first.header_value()?, fresh.header_value()?);
    Ok(())
}

#[test]
fn the_provider_names_its_service_and_never_its_secret() -> TestResult {
    let grant = Grant::new(
        &SecretUrl::new("https://as.example.org/token".to_owned()),
        CLIENT_ID,
        Scope::tokens(SCOPE)?,
    )?
    .with_client_secret(SecretMethod::Basic, SecretString::from(SECRET));
    let shown = format!("{:?}", provider(grant, Arc::new(SystemClock))?);
    assert!(shown.contains("pixm.manager[0]"), "{shown}");
    assert!(
        shown.contains("client_secret_basic") || shown.contains("Basic"),
        "{shown}"
    );
    assert!(!shown.contains(SECRET), "{shown}");
    Ok(())
}

#[test]
fn a_scope_is_held_to_the_scope_token_grammar_alone() -> TestResult {
    assert!(Scope::tokens("*").is_ok());
    assert_eq!(
        "patient/Patient.read openid",
        Scope::tokens(" patient/Patient.read \t openid ")?.as_str()
    );
    assert_eq!(Err(ScopeError::Empty), Scope::tokens("  "));
    for refused in ["say\"hi\"", "back\\slash", "caf\u{e9}"] {
        assert_eq!(
            Err(ScopeError::NotScopeToken {
                scope: refused.to_owned()
            }),
            Scope::tokens(refused),
            "{refused}"
        );
    }
    Ok(())
}
