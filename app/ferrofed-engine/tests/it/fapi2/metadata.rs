// SPDX-FileCopyrightText: Cadasto B.V.
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

use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::lane::LaneError;
use ferrofed_engine::onward::grant::on_behalf::OnBehalf as _;
use ferrofed_testkit::fapi::{AuthorizationServer, Metadata};
use ferrofed_testkit::oauth;
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::dpop::DpopKey;
use ferrohealth_smart::client::fapi2::Fapi2Error;
use ferrohealth_smart::client::onward::OnwardError;
use ferrohealth_smart::client::{ClientCredentials, TokenError};
use ferrohealth_smart::config::credentials::OauthSection;
use ferrohealth_smart::metadata::{DiscoveryError, MAX_DOCUMENT_BYTES};
use jsonwebtoken::Algorithm;
use openehr_its::rest::client::{CredentialsError, CredentialsProvider as _};
use secrecy::SecretString;

use super::{
    CLIENT_ID, RESOURCE, TestResult, client_key, exchange, grant, prover, provider, server,
    verified_caller,
};

/// What one refused grant met, and whether a token request was sent.
struct Refused {
    error: CredentialsError,
    sent: bool,
}

impl Refused {
    /// The token error the grant met, read through an onward lane's refusal
    /// where the grant exchanges.
    fn token(&self) -> Option<&TokenError> {
        let source = Error::source(&self.error)?;
        if let Some(error) = source.downcast_ref::<Arc<TokenError>>() {
            return Some(error.as_ref());
        }
        match source.downcast_ref::<LaneError>()? {
            LaneError::Onward(OnwardError::Token(error)) => Some(error),
            _ => None,
        }
    }

    /// Asserts the grant met a token error `expected` admits, and sent no
    /// token request.
    fn assert(&self, expected: impl Fn(&TokenError) -> bool) {
        let met = self.token();
        assert!(met.is_some_and(expected), "{:?}", self.error);
        assert!(!self.sent, "no token request was sent");
    }
}

/// Whether `error` is the FAPI 2.0 refusal `expected` admits.
fn fapi2(error: &TokenError, expected: impl Fn(&Fapi2Error) -> bool) -> bool {
    matches!(error, TokenError::Fapi2 { source } if expected(source))
}

/// What `grant`, at `server`, meets.
async fn refusal_of(
    server: &AuthorizationServer,
    grant: ClientCredentials,
) -> Result<Refused, Box<dyn Error>> {
    let error = provider(grant)?
        .credentials()
        .await
        .err()
        .ok_or("the grant is refused")?;
    Ok(Refused {
        error,
        sent: !server.endpoint().issuance().forms().is_empty(),
    })
}

/// What the default grant meets once a fresh server publishes `edit` of its
/// default metadata.
async fn refused_after(edit: impl FnOnce(&mut Metadata)) -> Result<Refused, Box<dyn Error>> {
    let key = client_key()?;
    let server = server(&key).await?;
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
    let mismatch = |error: &TokenError| {
        matches!(
            error,
            TokenError::Discovery(DiscoveryError::IssuerMismatch { .. })
        )
    };
    for named in [None, Some("https://elsewhere.example.org".to_owned())] {
        refused_after(|metadata| metadata.issuer = named)
            .await?
            .assert(mismatch);
    }
    refused_after(|metadata| {
        metadata.issuer = metadata.issuer.as_ref().map(|issuer| format!("{issuer}/"));
    })
    .await?
    .assert(mismatch);
    Ok(())
}

/// The token endpoint must be named, a URL, and on the issuer's origin.
#[tokio::test]
async fn a_token_endpoint_the_gateway_does_not_send_to_is_refused() -> TestResult {
    let endpoint = |error: &TokenError| {
        fapi2(error, |refused| {
            matches!(
                refused,
                Fapi2Error::Endpoint {
                    member: "token_endpoint"
                }
            )
        })
    };
    refused_after(|metadata| metadata.token_endpoint = None)
        .await?
        .assert(endpoint);
    refused_after(|metadata| {
        metadata.token_endpoint = Some("https://elsewhere.example.org/token".to_owned());
    })
    .await?
    .assert(endpoint);
    refused_after(|metadata| {
        metadata.token_endpoint = metadata
            .token_endpoint
            .as_ref()
            .map(|url| format!("{url}#f"));
    })
    .await?
    .assert(endpoint);
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
            .assert(|error| {
                fapi2(error, |refused| {
                    matches!(
                        refused,
                        Fapi2Error::AuthenticationMethod {
                            method: "private_key_jwt"
                        }
                    )
                })
            });
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
        .assert(|error| {
            fapi2(error, |refused| {
                matches!(
                    refused,
                    Fapi2Error::SigningAlgorithm {
                        algorithm: Algorithm::ES256
                    }
                )
            })
        });
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
                fapi2(error, |refused| {
                    matches!(
                        refused,
                        Fapi2Error::GrantType {
                            grant_type: "client_credentials"
                        }
                    )
                })
            });
    }
    Ok(())
}

/// A grant that exchanges needs token exchange listed (RFC 8414 §2, RFC
/// 8693 §2.1).
#[tokio::test]
async fn an_exchanging_grant_needs_token_exchange_listed() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await?;
    let mut metadata = server.metadata();
    metadata.grant_types_supported = Some(list(&["client_credentials"]));
    server.publish(&metadata);
    let lane = exchange(&server, grant(&server, key, &prover()?)?)?;
    let withheld = Arc::new(Withheld::new(Vec::<SecretString>::new()));
    let error = lane
        .provider(&verified_caller("clinician-0042")?, &withheld)
        .credentials()
        .await
        .err()
        .ok_or("the exchange is refused")?;
    let refused = Refused {
        error,
        sent: !server.endpoint().issuance().forms().is_empty(),
    };
    refused.assert(|error| {
        fapi2(error, |refused| {
            matches!(
                refused,
                Fapi2Error::GrantType {
                    grant_type: "urn:ietf:params:oauth:grant-type:token-exchange"
                }
            )
        })
    });
    Ok(())
}

/// `DPoP` algorithms, when listed, must name `ES256` (RFC 9449 §5.1).
#[tokio::test]
async fn a_server_without_es256_proofs_is_refused() -> TestResult {
    refused_after(|metadata| metadata.dpop_signing_alg_values_supported = Some(list(&["ES384"])))
        .await?
        .assert(|error| {
            fapi2(error, |refused| {
                matches!(
                    refused,
                    Fapi2Error::ProofAlgorithm {
                        algorithm: Algorithm::ES256
                    }
                )
            })
        });
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
                    TokenError::AuthorizationDetailsType { kind } if kind == "nl-gis-v1"
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
        keys: vec![serde_json::from_value(key.public_jwk())?],
    });
    let mut metadata = server.metadata();
    metadata.authorization_details_types_supported = None;
    server.publish(&metadata);
    let mut grant = grant(&server, key, &prover()?)?;
    grant.authorization_details = None;
    provider(grant)?.credentials().await?;
    assert_eq!(1, server.endpoint().issuance().issued());
    Ok(())
}

/// An answer that is no metadata document is refused: another media type,
/// a repeated name, and a body past the limit (RFC 8414 §3.2).
#[tokio::test]
async fn an_answer_that_is_no_metadata_document_is_refused() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await?;
    let body = serde_json::to_string(&server.metadata())?;
    server.publish_raw(&body, "text/plain");
    refusal_of(&server, grant(&server, key.clone(), &prover()?)?)
        .await?
        .assert(|error| fapi2(error, |refused| matches!(refused, Fapi2Error::MediaType)));

    let repeated = format!(
        "{{\"issuer\":\"https://elsewhere.example.org\",{}",
        body.trim_start_matches('{')
    );
    server.publish_raw(&repeated, "application/json");
    refusal_of(&server, grant(&server, key.clone(), &prover()?)?)
        .await?
        .assert(|error| {
            matches!(
                error,
                TokenError::Discovery(DiscoveryError::RepeatedName { .. })
            )
        });

    let padded = format!("{body}{}", " ".repeat(MAX_DOCUMENT_BYTES));
    server.publish_raw(&padded, "application/json");
    refusal_of(&server, grant(&server, key, &prover()?)?)
        .await?
        .assert(|error| {
            matches!(
                error,
                TokenError::Discovery(DiscoveryError::TooLarge { .. })
            )
        });
    Ok(())
}

/// A grant whose keys sign no algorithm FAPI 2.0 admits, that asks for
/// nothing, or that names no client cannot be built, and an exchanging lane
/// that names no target cannot load (FAPI 2.0 §5.4.1, §5.3.3.1; RFC 8707
/// §2).
#[tokio::test]
async fn a_grant_outside_the_profile_cannot_be_built() -> TestResult {
    let server = AuthorizationServer::start(CLIENT_ID, Some(300)).await;
    let refused = |grant: Result<ClientCredentials, Box<dyn Error>>,
                   expected: &dyn Fn(&Fapi2Error) -> bool| {
        let refused = grant.err().and_then(|error| {
            error
                .downcast_ref::<TokenError>()
                .map(|error| fapi2(error, expected))
        });
        assert_eq!(Some(true), refused);
    };
    let es384 = AssertionKey::from_pem(
        oauth::Curve::P384.pem()?.as_bytes(),
        Some(Algorithm::ES384),
        None,
    )?;
    refused(grant(&server, es384, &prover()?), &|error| {
        matches!(
            error,
            Fapi2Error::Algorithm {
                role: "assertion",
                ..
            }
        )
    });
    let p384 = Arc::new(DpopKey::from_pem(oauth::Curve::P384.pem()?.as_bytes())?);
    refused(grant(&server, client_key()?, &p384), &|error| {
        matches!(error, Fapi2Error::Algorithm { role: "DPoP", .. })
    });
    let mut unrequested = grant(&server, client_key()?, &prover()?)?;
    unrequested.scope = None;
    unrequested.authorization_details = None;
    assert!(matches!(
        unrequested.validate(),
        Err(TokenError::Fapi2 {
            source: Fapi2Error::Unrequested
        })
    ));
    let nameless: OauthSection = serde_json::from_value(serde_json::json!({
        "issuer": server.issuer(),
        "client_id": "",
        "client_secret": "synthetic",
    }))?;
    assert!(nameless.resolve().is_err(), "a grant names its client");
    let untargeted: ferrohealth_smart::config::onward::OnwardSection =
        serde_json::from_value(serde_json::json!({
            "issuer": server.issuer(),
            "kind": "persons",
            "profile": "rfc9068",
            "audiences": [],
            "indicators": [RESOURCE],
            "actor_assertion": true,
        }))?;
    assert!(
        untargeted
            .resolve(grant(&server, client_key()?, &prover()?)?)
            .is_err()
    );
    Ok(())
}

/// The client key and the grant's `Debug` show no key material, and the
/// details by their types alone.
#[tokio::test]
async fn debug_shows_no_key() -> TestResult {
    let server = AuthorizationServer::start(CLIENT_ID, Some(300)).await;
    let pem = oauth::Curve::P256.pem()?;
    let key = AssertionKey::from_pem(pem.clone().as_bytes(), Some(Algorithm::ES256), None)?;
    let grant = grant(&server, key, &prover()?)?;
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
