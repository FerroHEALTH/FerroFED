// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A FAPI 2.0 grant's client key rotated with an overlap: the previous key
//! is published beside the current one until the grant is built without
//! it, and only the current key signs (FAPI 2.0 Security Profile §5.4.1,
//! §5.4.2; RFC 7517 §5). No specification governs the rotation: our own
//! design.

use ferrofed_engine::onward::fapi2::{Fapi2Grant, Fapi2GrantError, Fapi2Security};
use ferrofed_engine::onward::keys::{KeyError, SigningKey};
use ferrofed_engine::onward::mtls::TlsClientAuth;
use ferrofed_engine::onward::{ClientAuthentication, Scope, SenderConstraint};
use ferrofed_testkit::oauth;
use openehr_its::rest::client::CredentialsProvider as _;
use secrecy::SecretString;

use super::{TestResult, client_key, grant, prover, provider, server};

/// The `kid` of every key of the grant's published set, in order.
fn published(grant: &Fapi2Grant) -> Vec<Option<String>> {
    grant
        .published_client_keys()
        .keys
        .iter()
        .map(|key| key.common.key_id.clone())
        .collect()
}

#[tokio::test]
async fn the_previous_client_key_is_published_after_the_current_one() -> TestResult {
    let (old, new) = (client_key()?, client_key()?);
    let server = server(&old).await;
    let rotated = grant(&server, new.clone(), &prover()?)?.with_previous_client_key(old.clone())?;
    assert_eq!(Some(new.kid()), rotated.client_key().map(SigningKey::kid));
    assert_eq!(
        Some(old.kid()),
        rotated.previous_client_key().map(SigningKey::kid)
    );
    assert_eq!(
        vec![Some(new.kid().to_owned()), Some(old.kid().to_owned())],
        published(&rotated)
    );
    let unrotated = grant(&server, new.clone(), &prover()?)?;
    assert_eq!(vec![Some(new.kid().to_owned())], published(&unrotated));
    Ok(())
}

/// Only the current key signs: a server that holds only the previous key
/// refuses the grant's assertion, and the same server holding the published
/// set accepts it, signed by the current key.
// conformance: CP-17
#[tokio::test]
async fn only_the_current_client_key_signs_while_the_previous_one_is_published() -> TestResult {
    let (old, new) = (client_key()?, client_key()?);
    let server = server(&old).await;
    let rotated = grant(&server, new.clone(), &prover()?)?.with_previous_client_key(old)?;
    let published_set = rotated.published_client_keys();
    let provider = provider(rotated)?;
    assert!(
        provider.credentials().await.is_err(),
        "a server holding only the previous key refuses the current key's assertion"
    );
    server.endpoint().trust(published_set);
    provider.credentials().await?;
    for assertion in server.endpoint().assertions() {
        let header = jsonwebtoken::decode_header(&assertion)?;
        assert_eq!(
            Some(new.kid()),
            header.kid.as_deref(),
            "only the new key signs"
        );
    }
    assert_eq!(1, server.endpoint().issued());
    Ok(())
}

#[tokio::test]
async fn a_previous_client_key_that_is_the_current_one_or_no_es256_key_is_refused() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let same = grant(&server, key.clone(), &prover()?)?.with_previous_client_key(key.clone());
    assert!(
        matches!(same, Err(Fapi2GrantError::Keys(KeyError::SameKey { .. }))),
        "{same:?}"
    );
    let p384 = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    let es384 = grant(&server, key, &prover()?)?.with_previous_client_key(p384);
    assert!(
        matches!(es384, Err(Fapi2GrantError::PreviousClientKey)),
        "{es384:?}"
    );
    Ok(())
}

/// A grant authenticated by mutual TLS with no client key publishes no key,
/// and a previous client key has no rotation to overlap there (RFC 8705 §2).
#[tokio::test]
async fn a_grant_with_no_client_key_publishes_none_and_takes_no_previous_one() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let by_certificate = Fapi2Grant::secured(
        oauth_server_metadata::Issuer::parse(&server.issuer())?,
        super::CLIENT_ID,
        Fapi2Security {
            client_auth: ClientAuthentication::Tls(TlsClientAuth::SelfSigned),
            sender: SenderConstraint::Dpop(prover()?),
            client_key: None,
        },
        (Some(Scope::parse(super::SCOPE)?), None),
    )?;
    assert!(by_certificate.published_client_keys().keys.is_empty());
    assert!(by_certificate.previous_client_key().is_none());
    let refused = by_certificate.with_previous_client_key(key);
    assert!(
        matches!(refused, Err(Fapi2GrantError::PreviousWithoutClientKey)),
        "{refused:?}"
    );
    Ok(())
}
