// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `[signing]` key's algorithm read from its curve: a P-256 key signs
//! ES256 and a P-384 key ES384 (RFC 7518 §3.4), each published as such in
//! the JWK Set (RFC 7517 §5), and both the client assertion a node's
//! authorization server verifies (§13.1, N25, CP-17) and the conveyance a
//! node verifies (§13.1, N24, CP-16) are signed with it. ES256 is on the
//! FAPI 2.0 Security Profile's list for a JWT (§5.4.1), so a deployment
//! whose nodes hold to it signs with a P-256 key.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::onward::conveyance::{Conveyance, Principal, Signer};
use ferrofed_engine::onward::keys::{KeyError, KeyRing, SigningKey};
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{Grant, Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use jsonwebtoken::Algorithm;
use jsonwebtoken::jwk::KeyAlgorithm;
use openehr_its::rest::client::{CredentialsProvider, ReqwestTransport};
use secrecy::SecretString;

use crate::conveyed;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The node every conveyance in this file is signed for.
const NODE: &str = "node-a-pub";

/// A `[signing]` key read from `pem` by its curve.
fn read(pem: String) -> Result<SigningKey, KeyError> {
    SigningKey::from_ec_pem(&SecretString::from(pem))
}

/// A ring of `current` alone, published for no overlap.
fn ring(current: SigningKey) -> Result<Arc<KeyRing>, KeyError> {
    Ok(Arc::new(KeyRing::new(
        current,
        None,
        Duration::ZERO,
        Arc::new(SystemClock),
    )?))
}

/// The algorithm a published key names.
fn published_algorithm(key: &SigningKey) -> Option<KeyAlgorithm> {
    key.public().common.key_algorithm
}

#[test]
fn a_p256_key_signs_es256_and_a_p384_key_es384() -> TestResult {
    let p256 = read(oauth::p256_pem()?)?;
    assert_eq!(Algorithm::ES256, p256.algorithm());
    assert_eq!(Some(KeyAlgorithm::ES256), published_algorithm(&p256));
    let p384 = read(oauth::es384_pem()?)?;
    assert_eq!(Algorithm::ES384, p384.algorithm());
    assert_eq!(Some(KeyAlgorithm::ES384), published_algorithm(&p384));
    Ok(())
}

#[test]
fn a_key_on_neither_curve_is_refused_and_quotes_nothing() -> TestResult {
    let p521 = read(oauth::p521_pem()?);
    assert!(
        matches!(p521, Err(KeyError::CurveUnsupported(_))),
        "{p521:?}"
    );
    let garbage = read(String::from("Qz7-not-a-key"));
    assert!(matches!(garbage, Err(KeyError::Pem(_))), "{garbage:?}");
    let shown = format!("{garbage:?}");
    assert!(!shown.contains("Qz7"), "{shown}");
    Ok(())
}

/// A P-256 `[signing]` key signs the client assertion ES256, and the token
/// endpoint verifies it against the published set (RFC 7523 §3).
// conformance: CP-17
#[tokio::test]
async fn an_es256_signing_key_signs_the_client_assertion_es256() -> TestResult {
    let endpoint = TokenEndpoint::start(CLIENT_ID, None).await;
    let keys = ring(read(oauth::p256_pem()?)?)?;
    endpoint.trust(keys.published());
    endpoint.expect_assertion(Algorithm::ES256, &endpoint.token_url());
    let grant = Grant::new(
        &SecretUrl::new(endpoint.token_url()),
        CLIENT_ID,
        Scope::parse("system/aql-*.s")?,
    )?;
    let provider = ClientCredentials::new(
        EndpointId::new(NODE)?,
        grant,
        Arc::clone(&keys),
        (Duration::from_secs(300), Duration::from_secs(5)),
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
        Arc::new(SystemClock),
    );
    provider.credentials().await?;
    let [assertion] = endpoint
        .assertions()
        .try_into()
        .map_err(|_more| "one assertion")?;
    assert_eq!(
        Algorithm::ES256,
        jsonwebtoken::decode_header(&assertion)?.alg
    );
    oauth::verify_signed(
        &assertion,
        &keys.published(),
        (CLIENT_ID, &endpoint.token_url()),
        Some(Algorithm::ES256),
    )?;
    Ok(())
}

/// The conveyance is signed with the algorithm of the current key, and a
/// node verifies it against the published set by `kid`, ES256 and ES384
/// alike.
// conformance: CP-16
#[test]
fn the_conveyance_is_signed_with_the_current_keys_algorithm() -> TestResult {
    for (pem, algorithm) in [
        (oauth::p256_pem()?, Algorithm::ES256),
        (oauth::es384_pem()?, Algorithm::ES384),
    ] {
        let keys = ring(read(pem)?)?;
        let signer = Arc::new(Signer::new(Arc::clone(&keys), conveyed::GATEWAY));
        let conveyance = Conveyance::new(signer, Principal::Caller(conveyed::caller()));
        let token = conveyance.signed_for(&EndpointId::new(NODE)?)?;
        assert_eq!(algorithm, jsonwebtoken::decode_header(&token)?.alg);
        let read = conveyed::verified(&token, &keys, (conveyed::GATEWAY, NODE))?;
        assert_eq!(conveyed::SUBJECT, read.sub);
    }
    Ok(())
}

/// A rotation from an ES384 key to an ES256 key publishes both for the
/// overlap window, each with its own algorithm; the new key signs ES256,
/// and a conveyance the old key signed before the rotation still verifies.
// conformance: CP-16
#[test]
fn a_rotation_across_curves_publishes_both_and_signs_with_the_new() -> TestResult {
    let old = read(oauth::es384_pem()?)?;
    let new = read(oauth::p256_pem()?)?;
    let node = EndpointId::new(NODE)?;
    let before = ring(old.clone())?;
    let signed_by_old = Conveyance::new(
        Arc::new(Signer::new(before, conveyed::GATEWAY)),
        Principal::Caller(conveyed::caller()),
    )
    .signed_for(&node)?;

    let rotated = Arc::new(KeyRing::new(
        new.clone(),
        Some(old.clone()),
        Duration::from_mins(65),
        Arc::new(SystemClock),
    )?);
    let published: Vec<_> = rotated
        .published()
        .keys
        .iter()
        .map(|jwk| (jwk.common.key_id.clone(), jwk.common.key_algorithm))
        .collect();
    assert_eq!(
        vec![
            (Some(new.kid().to_owned()), Some(KeyAlgorithm::ES256)),
            (Some(old.kid().to_owned()), Some(KeyAlgorithm::ES384)),
        ],
        published
    );
    conveyed::verified(&signed_by_old, &rotated, (conveyed::GATEWAY, NODE))?;
    let signed_by_new = Conveyance::new(
        Arc::new(Signer::new(Arc::clone(&rotated), conveyed::GATEWAY)),
        Principal::Caller(conveyed::caller()),
    )
    .signed_for(&node)?;
    let header = jsonwebtoken::decode_header(&signed_by_new)?;
    assert_eq!(Algorithm::ES256, header.alg);
    assert_eq!(Some(new.kid()), header.kid.as_deref());
    conveyed::verified(&signed_by_new, &rotated, (conveyed::GATEWAY, NODE))?;
    Ok(())
}
