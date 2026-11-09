// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FAPI 2.0 grant's client key rotated with an overlap: the previous key
//! is published beside the current one until the grant is built without
//! it, and only the current key signs (FAPI 2.0 Security Profile §5.4.1,
//! §5.4.2; RFC 7517 §5). No specification governs the rotation: our own
//! design.

use std::sync::Arc;

use ferrofed_testkit::oauth;
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::fapi2::Fapi2Error;
use ferrohealth_smart::client::keys::{KeyRing, KeyRingError};
use ferrohealth_smart::client::{ClientCredentials, TokenError};
use ferrohealth_smart::config::credentials::OauthSection;
use jsonwebtoken::Algorithm;
use jsonwebtoken::jwk::JwkSet;
use openehr_its::rest::client::CredentialsProvider as _;

use super::{TestResult, client_key, grant, prover, provider, server};

/// `grant`, its assertions signed by its current key with `previous`
/// published beside it until the grant is built without it.
fn rotated(
    mut grant: ClientCredentials,
    previous: AssertionKey,
) -> Result<ClientCredentials, Box<dyn std::error::Error>> {
    let current = grant
        .assertion_keys
        .as_deref()
        .map(KeyRing::current)
        .ok_or("a client key")?
        .clone();
    grant.assertion_keys = Some(Arc::new(
        KeyRing::new(current).with_previous_until_removed(previous)?,
    ));
    Ok(grant)
}

/// The `kid` of every key of the grant's published set, in order.
fn published(grant: &ClientCredentials) -> Result<Vec<Option<String>>, Box<dyn std::error::Error>> {
    let ring = grant.assertion_keys.as_deref().ok_or("a set")?;
    let set: JwkSet = serde_json::from_value(ring.published())?;
    Ok(set
        .keys
        .iter()
        .map(|key| key.common.key_id.clone())
        .collect())
}

#[tokio::test]
async fn the_previous_client_key_is_published_after_the_current_one() -> TestResult {
    let (old, new) = (client_key()?, client_key()?);
    let server = server(&old).await?;
    let rotated = rotated(grant(&server, new.clone(), &prover()?)?, old.clone())?;
    let ring = rotated.assertion_keys.as_deref().ok_or("a ring")?;
    assert_eq!(new.kid(), ring.current().kid());
    assert_eq!(Some(old.kid()), ring.retiring().map(AssertionKey::kid));
    assert_eq!(
        vec![Some(new.kid().to_owned()), Some(old.kid().to_owned())],
        published(&rotated)?
    );
    let unrotated = grant(&server, new.clone(), &prover()?)?;
    assert_eq!(vec![Some(new.kid().to_owned())], published(&unrotated)?);
    Ok(())
}

/// Only the current key signs: a server that holds only the previous key
/// refuses the grant's assertion, and the same server holding the published
/// set accepts it, signed by the current key.
// conformance: CP-17
#[tokio::test]
async fn only_the_current_client_key_signs_while_the_previous_one_is_published() -> TestResult {
    let (old, new) = (client_key()?, client_key()?);
    let server = server(&old).await?;
    let rotated = rotated(grant(&server, new.clone(), &prover()?)?, old)?;
    let published_set: JwkSet = serde_json::from_value(
        rotated
            .assertion_keys
            .as_deref()
            .ok_or("a set")?
            .published(),
    )?;
    let provider = provider(rotated)?;
    assert!(
        provider.credentials().await.is_err(),
        "a server holding only the previous key refuses the current key's assertion"
    );
    server.endpoint().trust(published_set);
    provider.refused();
    provider.credentials().await?;
    for assertion in server.endpoint().assertions() {
        let header = jsonwebtoken::decode_header(&assertion)?;
        assert_eq!(
            Some(new.kid()),
            header.kid.as_deref(),
            "only the new key signs"
        );
    }
    assert_eq!(1, server.endpoint().issuance().issued());
    Ok(())
}

#[tokio::test]
async fn a_previous_client_key_that_is_the_current_one_or_no_es256_key_is_refused() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await?;
    let same = rotated(grant(&server, key.clone(), &prover()?)?, key.clone());
    assert!(
        same.as_ref()
            .err()
            .and_then(|error| error.downcast_ref::<KeyRingError>())
            .is_some_and(|error| matches!(error, KeyRingError::SameKey { .. })),
        "{same:?}"
    );
    let p384 = AssertionKey::from_pem(
        oauth::Curve::P384.pem()?.as_bytes(),
        Some(Algorithm::ES384),
        None,
    )?;
    let es384 = rotated(grant(&server, key, &prover()?)?, p384)?.validate();
    assert!(
        matches!(
            es384,
            Err(TokenError::Fapi2 {
                source: Fapi2Error::Algorithm {
                    role: "previous assertion",
                    ..
                }
            })
        ),
        "{es384:?}"
    );
    Ok(())
}

/// A grant authenticated by mutual TLS with no client key publishes no key,
/// and a previous client key has no rotation to overlap there (RFC 8705 §2).
#[tokio::test]
async fn a_grant_with_no_client_key_publishes_none_and_takes_no_previous_one() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await?;
    let mut by_certificate = grant(&server, key, &prover()?)?;
    by_certificate.client_auth = ferrohealth_smart::client::ClientAuth::SelfSignedTlsClientAuth;
    by_certificate.assertion_keys = None;
    assert!(by_certificate.assertion_keys.is_none(), "no key to publish");
    let table: OauthSection = serde_json::from_value(serde_json::json!({
        "issuer": server.issuer(),
        "client_id": super::CLIENT_ID,
        "client_auth": "self_signed_tls_client_auth",
        "previous_private_key_file": "/nonexistent/previous.pem",
    }))?;
    let problems = table.problems("credentials.node-a-pub.oauth");
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("previous_private_key_file")),
        "{problems:?}"
    );
    Ok(())
}
