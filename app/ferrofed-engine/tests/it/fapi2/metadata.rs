// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The metadata a FAPI 2.0 grant refuses, and the grants that cannot be
//! built: each refusal sends no token request (RFC 8414 §2, §3.2, §3.3; RFC
//! 9396 §10; RFC 9449 §5.1; FAPI 2.0 Security Profile §5.3.2.1, §5.3.3.1,
//! §5.4.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;

use ferrofed_engine::onward::Scope;
use ferrofed_engine::onward::authorization_details::AuthorizationDetails;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::grant::fapi2::metadata::{DiscoveryError, RESPONSE_LIMIT};
use ferrofed_engine::onward::grant::fapi2::{Fapi2Error, Fapi2Grant, Fapi2GrantError};
use ferrofed_engine::onward::keys::SigningKey;
use ferrofed_testkit::fapi::{AuthorizationServer, Metadata};
use ferrofed_testkit::oauth;
use oauth_server_metadata::{EndpointError, Issuer};
use openehr_its::rest::client::{CredentialsError, CredentialsProvider as _};
use secrecy::SecretString;

use super::{CLIENT_ID, DETAILS, SCOPE, TestResult, client_key, grant, prover, provider, server};

/// What one refused grant met, and whether a token request was sent.
struct Refused {
    error: CredentialsError,
    sent: bool,
}

impl Refused {
    /// The discovery error the grant met, when it met one.
    fn discovery(&self) -> Option<&DiscoveryError> {
        match Error::source(&self.error)?.downcast_ref::<Fapi2Error>()? {
            Fapi2Error::Discovery(error) => Some(error),
            _ => None,
        }
    }

    /// Asserts the grant met a discovery error `expected` admits, and sent
    /// no token request.
    fn assert(&self, expected: impl Fn(&DiscoveryError) -> bool) {
        let met = self.discovery();
        assert!(met.is_some_and(expected), "{:?}", self.error);
        assert!(!self.sent, "no token request was sent");
    }
}

/// What `grant`, at `server`, meets.
async fn refusal_of(
    server: &AuthorizationServer,
    grant: Fapi2Grant,
) -> Result<Refused, Box<dyn Error>> {
    let error = provider(grant)?
        .credentials()
        .await
        .err()
        .ok_or("the grant is refused")?;
    Ok(Refused {
        error,
        sent: !server.endpoint().forms().is_empty(),
    })
}

/// What the default grant meets once a fresh server publishes `edit` of its
/// default metadata.
async fn refused_after(edit: impl FnOnce(&mut Metadata)) -> Result<Refused, Box<dyn Error>> {
    let key = client_key()?;
    let server = server(&key).await;
    let mut metadata = server.metadata();
    edit(&mut metadata);
    server.publish(&metadata);
    let grant = grant(&server, key, &prover()?)?;
    refusal_of(&server, grant).await
}

/// A list of `items`.
fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// The metadata must name the issuer the grant asked of, identically (RFC
/// 8414 §3.3; FAPI 2.0 §5.3.3.1).
#[tokio::test]
async fn another_issuer_is_refused() -> TestResult {
    for named in [None, Some("https://elsewhere.example.org".to_owned())] {
        refused_after(|metadata| metadata.issuer = named)
            .await?
            .assert(|error| matches!(error, DiscoveryError::Issuer));
    }
    refused_after(|metadata| {
        metadata.issuer = metadata.issuer.as_ref().map(|issuer| format!("{issuer}/"));
    })
    .await?
    .assert(|error| matches!(error, DiscoveryError::Issuer));
    Ok(())
}

/// The token endpoint must be named, a URL, and on the issuer's origin.
#[tokio::test]
async fn a_token_endpoint_the_gateway_does_not_send_to_is_refused() -> TestResult {
    refused_after(|metadata| metadata.token_endpoint = None)
        .await?
        .assert(|error| matches!(error, DiscoveryError::TokenEndpoint));
    refused_after(|metadata| {
        metadata.token_endpoint = Some("https://elsewhere.example.org/token".to_owned());
    })
    .await?
    .assert(|error| matches!(error, DiscoveryError::Endpoint(EndpointError::OtherOrigin)));
    refused_after(|metadata| {
        metadata.token_endpoint = metadata
            .token_endpoint
            .as_ref()
            .map(|url| format!("{url}#f"));
    })
    .await?
    .assert(|error| matches!(error, DiscoveryError::Endpoint(EndpointError::Insecure)));
    Ok(())
}

/// `private_key_jwt` must be offered, and an omitted list means
/// `client_secret_basic` alone (RFC 8414 §2; FAPI 2.0 §5.3.2.1).
#[tokio::test]
async fn a_server_without_private_key_jwt_is_refused() -> TestResult {
    for methods in [
        None,
        Some(list(&["client_secret_basic", "tls_client_auth"])),
    ] {
        refused_after(|metadata| metadata.token_endpoint_auth_methods_supported = methods)
            .await?
            .assert(|error| matches!(error, DiscoveryError::ClientAuthentication));
    }
    Ok(())
}

/// The signing algorithms must be listed beside `private_key_jwt`, and name
/// `ES256` (RFC 8414 §2).
#[tokio::test]
async fn a_server_without_es256_assertions_is_refused() -> TestResult {
    for algorithms in [None, Some(list(&["PS256", "ES384"]))] {
        refused_after(|metadata| {
            metadata.token_endpoint_auth_signing_alg_values_supported = algorithms;
        })
        .await?
        .assert(|error| matches!(error, DiscoveryError::SigningAlgorithm));
    }
    Ok(())
}

/// The client-credentials grant must be listed, and an omitted list means
/// `authorization_code` and `implicit` alone (RFC 8414 §2).
#[tokio::test]
async fn a_server_without_the_client_credentials_grant_is_refused() -> TestResult {
    for grants in [None, Some(list(&["authorization_code"]))] {
        refused_after(|metadata| metadata.grant_types_supported = grants)
            .await?
            .assert(|error| {
                matches!(
                    error,
                    DiscoveryError::GrantType {
                        grant_type: "client_credentials"
                    }
                )
            });
    }
    Ok(())
}

/// A grant that exchanges needs token exchange listed (RFC 8414 §2, RFC
/// 8693 §2.1).
#[tokio::test]
async fn an_exchanging_grant_needs_token_exchange_listed() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let mut metadata = server.metadata();
    metadata.grant_types_supported = Some(list(&["client_credentials"]));
    server.publish(&metadata);
    let grant = grant(&server, key, &prover()?)?
        .with_resource("https://cdr-a.example.org/openehr")?
        .with_token_exchange()?;
    refusal_of(&server, grant).await?.assert(|error| {
        matches!(
            error,
            DiscoveryError::GrantType {
                grant_type: "urn:ietf:params:oauth:grant-type:token-exchange"
            }
        )
    });
    Ok(())
}

/// `DPoP` algorithms, when listed, must name `ES256` (RFC 9449 §5.1).
#[tokio::test]
async fn a_server_without_es256_proofs_is_refused() -> TestResult {
    refused_after(|metadata| metadata.dpop_signing_alg_values_supported = Some(list(&["ES384"])))
        .await?
        .assert(|error| matches!(error, DiscoveryError::ProofAlgorithm));
    Ok(())
}

/// Every authorization details type the grant asks for must be listed, and
/// an omitted list lists none (RFC 9396 §10).
#[tokio::test]
async fn an_unlisted_authorization_details_type_is_refused() -> TestResult {
    for types in [None, Some(list(&["another-type"]))] {
        refused_after(|metadata| metadata.authorization_details_types_supported = types)
            .await?
            .assert(|error| {
                matches!(
                    error,
                    DiscoveryError::AuthorizationDetailsType { kind } if kind == "nl-gis-v1"
                )
            });
    }
    Ok(())
}

/// A grant that asks for no details takes metadata that lists none.
#[tokio::test]
async fn a_grant_without_details_needs_no_listed_type() -> TestResult {
    let key = client_key()?;
    let server = AuthorizationServer::start(CLIENT_ID, Some(300)).await;
    server.endpoint().trust(jsonwebtoken::jwk::JwkSet {
        keys: vec![key.public().clone()],
    });
    let mut metadata = server.metadata();
    metadata.authorization_details_types_supported = None;
    server.publish(&metadata);
    let grant = Fapi2Grant::new(
        Issuer::parse(&server.issuer())?,
        CLIENT_ID,
        (key, prover()?),
        (Some(Scope::parse(SCOPE)?), None),
    )?;
    provider(grant)?.credentials().await?;
    assert_eq!(1, server.endpoint().issued());
    Ok(())
}

/// An answer that is no metadata document is refused: another media type,
/// a repeated name, and a body past the limit (RFC 8414 §3.2).
#[tokio::test]
async fn an_answer_that_is_no_metadata_document_is_refused() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let body = serde_json::to_string(&server.metadata())?;
    server.publish_raw(&body, "text/plain");
    refusal_of(&server, grant(&server, key.clone(), &prover()?)?)
        .await?
        .assert(|error| matches!(error, DiscoveryError::MediaType));

    let repeated = format!(
        "{{\"issuer\":\"https://elsewhere.example.org\",{}",
        body.trim_start_matches('{')
    );
    server.publish_raw(&repeated, "application/json");
    refusal_of(&server, grant(&server, key.clone(), &prover()?)?)
        .await?
        .assert(|error| matches!(error, DiscoveryError::Body(_)));

    let padded = format!("{body}{}", " ".repeat(RESPONSE_LIMIT));
    server.publish_raw(&padded, "application/json");
    refusal_of(&server, grant(&server, key, &prover()?)?)
        .await?
        .assert(|error| matches!(error, DiscoveryError::TooLarge { .. }));
    Ok(())
}

/// A grant whose keys do not sign `ES256`, or that asks for nothing, cannot
/// be built (FAPI 2.0 §5.4.1, §5.3.3.1).
#[test]
fn a_grant_outside_the_profile_cannot_be_built() -> TestResult {
    let issuer = || Issuer::parse("https://as.example.org");
    let request = || -> Result<_, Box<dyn Error>> {
        Ok((
            Some(Scope::parse(SCOPE)?),
            Some(AuthorizationDetails::parse(DETAILS)?),
        ))
    };
    let es384 = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    assert!(matches!(
        Fapi2Grant::new(issuer()?, CLIENT_ID, (es384, prover()?), request()?),
        Err(Fapi2GrantError::ClientKey)
    ));
    let p384 = Arc::new(Prover::from_pem(&SecretString::from(oauth::es384_pem()?))?);
    assert!(matches!(
        Fapi2Grant::new(issuer()?, CLIENT_ID, (client_key()?, p384), request()?),
        Err(Fapi2GrantError::DpopKey)
    ));
    assert!(matches!(
        Fapi2Grant::new(
            issuer()?,
            CLIENT_ID,
            (client_key()?, prover()?),
            (None, None)
        ),
        Err(Fapi2GrantError::Unrequested)
    ));
    assert!(matches!(
        Fapi2Grant::new(issuer()?, "", (client_key()?, prover()?), request()?),
        Err(Fapi2GrantError::ClientId)
    ));
    let untargeted = Fapi2Grant::new(issuer()?, CLIENT_ID, (client_key()?, prover()?), request()?)?
        .with_token_exchange();
    assert!(matches!(untargeted, Err(Fapi2GrantError::Untargeted)));
    Ok(())
}

/// The client key and the grant's `Debug` show no key material.
#[test]
fn debug_shows_no_key() -> TestResult {
    let pem = oauth::p256_pem()?;
    let key = SigningKey::from_p256_pem(&SecretString::from(pem.clone()))?;
    let grant = Fapi2Grant::new(
        Issuer::parse("https://as.example.org")?,
        CLIENT_ID,
        (key, prover()?),
        (
            Some(Scope::parse(SCOPE)?),
            Some(AuthorizationDetails::parse(DETAILS)?),
        ),
    )?;
    let shown = format!("{grant:?}");
    let body = pem
        .lines()
        .find(|line| !line.starts_with("-----"))
        .ok_or("a PEM body")?;
    assert!(!shown.contains(body), "{shown}");
    assert!(!shown.contains("TREAT"), "{shown}");
    assert!(shown.contains("nl-gis-v1"), "{shown}");
    Ok(())
}
