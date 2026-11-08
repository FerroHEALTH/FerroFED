// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A cross-border request through the harness authorization server acting
//! for NCPeH-A: the HP Identity Assertion exchanged as an RFC 8693
//! `saml2` `subject_token` and the Treatment Relationship Confirmation in
//! `trc_token`, both checked by `ferrohealth-smart`'s validator, and the
//! token it issues presented at the gateway (FerroHEALTH/FerroFED#1620).
//!
//! A client other than the national connector is refused before any
//! assertion is read (SAML Profile 10.0.0 §2.1: `sender-vouches` names no
//! presenter); a TRC naming another identity assertion, an assertion signed
//! by an RSA key below 3000 bits and a second exchange of the same
//! assertions are refused (eHDSI SAML Profile 10.0.0 §4.1; Cryptographic
//! Algorithms 9.0.0 §2; RFC 7522 §3); an exchange with no TRC gives a
//! discovery-phase token that patient data refuses as `trc-missing`; and a
//! full exchange reaches the TRC patient's member under a token that lives
//! within both assertion windows.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ferrofed_testkit::ehdsi_exchange::{
    Assertion, Exchange, ExchangeServer, HP_ID, HP_NAME, ORGANIZATION_ID, SAML2, Seal, SealKey,
    TOKEN_EXCHANGE, connector_authorization, token_of,
};
use ferrohealth_smart::federation::assertion::signer::{SignerError, TrustedSigner};
use http::{Method, StatusCode};
use serde_json::Value;

use super::ehr::by_subject;
use super::{answered, assert_refused_as, gateway};
use crate::auth::TestResult;
use crate::contact_point::{CERTIFICATE, NCP_ISSUER, bound};
use crate::facade::{EHR_A, NAMESPACE, PATIENT};
use crate::support::{asked, caller};

/// The identity assertion's `@ID`.
const IDA: &str = "_6f1d2c3b-4a5e-4f60-8a7b-9c0d1e2f3a4b";

/// Another identity assertion's `@ID`, which no exchange presents.
const OTHER_IDA: &str = "_0a1b2c3d-4e5f-4a6b-8c7d-8e9f0a1b2c3d";

/// The treatment confirmation's `@ID`.
const TRC: &str = "_9e8d7c6b-5a4f-4e3d-8c2b-1a0f9e8d7c6b";

/// The seconds since the Unix epoch.
fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// The harness authorization server trusting `seal` as NCP-B's, issuing as
/// the test contact point for the gateway, bound to the test connector's
/// certificate.
async fn trusting(seal: TrustedSigner) -> ExchangeServer {
    ExchangeServer::start(Exchange {
        seals: vec![seal],
        signer: caller::issuer(),
        issuer: NCP_ISSUER.to_owned(),
        audience: caller::AUDIENCE.to_owned(),
        certificate: CERTIFICATE.clone(),
    })
    .await
}

/// The status and the JSON body of the exchange of the signed identity
/// assertion `identity` and, when given, the signed TRC `trc`, presented by
/// the national connector.
async fn exchange(
    server: &ExchangeServer,
    identity: &str,
    trc: Option<&str>,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    presented(server, &connector_authorization(), identity, trc).await
}

/// The status and the JSON body of the exchange of `identity` and `trc`,
/// the client authenticating with the `Authorization` value `client`.
async fn presented(
    server: &ExchangeServer,
    client: &str,
    identity: &str,
    trc: Option<&str>,
) -> Result<(StatusCode, Value), Box<dyn Error>> {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    form.append_pair("grant_type", TOKEN_EXCHANGE)
        .append_pair("subject_token", &token_of(identity))
        .append_pair("subject_token_type", SAML2);
    if let Some(trc) = trc {
        form.append_pair("trc_token", &token_of(trc))
            .append_pair("trc_token_type", SAML2);
    }
    let answer = reqwest::Client::new()
        .post(server.token_url())
        .header(http::header::AUTHORIZATION, client)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form.finish())
        .send()
        .await?;
    let status = StatusCode::from_u16(answer.status().as_u16())?;
    Ok((status, answer.json().await?))
}

/// The claims of the compact JWS `token`, read without verifying it.
fn payload(token: &str) -> Result<Value, Box<dyn Error>> {
    let part = token.split('.').nth(1).ok_or("a compact JWS")?;
    Ok(serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part)?)?)
}

/// The access token of a `200` exchange answer.
fn access_token(status: StatusCode, answer: &Value) -> Result<String, Box<dyn Error>> {
    assert_eq!(StatusCode::OK, status, "{answer}");
    Ok(answer["access_token"]
        .as_str()
        .ok_or("an access token")?
        .to_owned())
}

/// Asserts `answer` is an `invalid_grant` refusal naming the data-integrity
/// code (MyHealth@EU Requirements Catalogue 10.0.0 §1.25).
fn assert_integrity_refused(status: StatusCode, answer: &Value) {
    assert_eq!(StatusCode::BAD_REQUEST, status, "{answer}");
    assert_eq!(answer["error"], "invalid_grant", "{answer}");
    let described = answer["error_description"].as_str().unwrap_or_default();
    assert!(
        described.starts_with("ERROR_SEC_DATA_INTEGRITY_NOT_ENSURED"),
        "{answer}"
    );
}

/// eHDSI SAML Profile 10.0.0 §4.1: a TRC whose `AssertionIDRef` names
/// another identity assertion than the `subject_token` is refused.
#[tokio::test]
async fn a_trc_naming_another_identity_assertion_is_refused() -> TestResult {
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let now = now();
    let identity = seal.sign(&Assertion::identity(IDA, now))?;
    let trc = seal.sign(&Assertion::treatment(
        TRC,
        OTHER_IDA,
        ("2.999.1", PATIENT),
        now,
    ))?;

    let (status, answer) = exchange(&server, &identity, Some(&trc)).await?;
    assert_integrity_refused(status, &answer);
    Ok(())
}

/// Cryptographic Algorithms 9.0.0 §2, SOG-IS ACM 1.3 §4.1: a seal over an
/// RSA key below 3000 bits is never trusted, and an assertion it signs is
/// refused by an authorization server that trusts NCP-B's seal.
#[tokio::test]
async fn an_assertion_signed_by_an_rsa_key_below_3000_bits_is_refused() -> TestResult {
    let short = Seal::new(SealKey::Rsa2048)?;
    assert!(
        matches!(short.trusted(), Err(SignerError::ShortKey { bits: 2048 })),
        "a 2048-bit seal is no trusted signer"
    );
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let identity = short.sign(&Assertion::identity(IDA, now()))?;

    let (status, answer) = exchange(&server, &identity, None).await?;
    assert_integrity_refused(status, &answer);
    Ok(())
}

/// eHDSI SAML Profile 10.0.0 §2.1: a `sender-vouches` assertion names no
/// presenter, so an exchange presented by any client other than the
/// national connector is refused `invalid_client` (RFC 6749 §5.2), whatever
/// it carries.
#[tokio::test]
async fn an_exchange_presented_by_another_client_is_refused() -> TestResult {
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let identity = seal.sign(&Assertion::identity(IDA, now()))?;
    let other = format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD
            .encode("synthetic-other-client:synthetic-other-secret")
    );
    for client in [other.as_str(), ""] {
        let (status, answer) = presented(&server, client, &identity, None).await?;
        assert_eq!(StatusCode::UNAUTHORIZED, status, "{answer}");
        assert_eq!(answer["error"], "invalid_client", "{answer}");
    }
    let (status, answer) = exchange(&server, &identity, None).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the refused presentations admitted no exchange: {answer}"
    );
    Ok(())
}

/// RFC 7522 §3, item 6, and SAML 2.0 Core §2.5.1.5: an exchange of the same
/// assertions inside their window is admitted once, and its replay refused
/// `invalid_grant` naming the data-integrity code.
#[tokio::test]
async fn a_second_exchange_of_the_same_assertions_is_refused() -> TestResult {
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let now = now();
    let identity = seal.sign(&Assertion::identity(IDA, now))?;
    let trc = seal.sign(&Assertion::treatment(TRC, IDA, ("2.999.1", PATIENT), now))?;

    for presented in [None, Some(trc.as_str())] {
        let (status, answer) = exchange(&server, &identity, presented).await?;
        access_token(status, &answer)?;
        let (status, answer) = exchange(&server, &identity, presented).await?;
        assert_integrity_refused(status, &answer);
    }
    Ok(())
}

/// The discovery phase: an exchange with no `trc_token` is issued a token
/// bounded by the identity assertion that binds no patient; the gateway
/// admits it, and refuses a patient-data request under it `trc-missing`
/// with nothing sent to any member.
#[tokio::test]
async fn without_a_trc_the_discovery_token_reaches_no_patient_data() -> TestResult {
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let identity_parts = Assertion::identity(IDA, now());
    let identity = seal.sign(&identity_parts)?;

    let (status, answer) = exchange(&server, &identity, None).await?;
    let token = access_token(status, &answer)?;
    let claims = payload(&token)?;
    assert!(claims["ehdsi"]["patient"].is_null(), "{claims}");
    assert!(claims["ehdsi"]["trc"].is_null(), "{claims}");
    let exp = claims["exp"].as_i64().ok_or("an exp")?;
    assert!(
        exp <= identity_parts.not_on_or_after,
        "bounded by the identity assertion: {exp}"
    );

    let bound_gateway = gateway().await?;
    let request = bound(by_subject(PATIENT)?, &token)?;
    assert_refused_as(&bound_gateway, request, "trc-missing").await
}

/// A full exchange: the token carries the field set of both assertions and
/// the TRC patient, lives within both windows and no longer than the
/// harness lifetime, and a read by subject of the TRC patient under it
/// reaches the patient's own EHR at its member.
#[tokio::test]
async fn a_full_exchange_reaches_the_trc_patients_member() -> TestResult {
    let seal = Seal::new(SealKey::Rsa3072)?;
    let server = trusting(seal.trusted()?).await;
    let now = now();
    let identity_parts = Assertion::identity(IDA, now);
    let trc_parts = Assertion::treatment(TRC, IDA, ("2.999.1", PATIENT), now);
    let window = identity_parts
        .not_on_or_after
        .min(trc_parts.not_on_or_after);
    let identity = seal.sign(&identity_parts)?;
    let trc = seal.sign(&trc_parts)?;

    let (status, answer) = exchange(&server, &identity, Some(&trc)).await?;
    let token = access_token(status, &answer)?;
    let claims = payload(&token)?;
    let ehdsi = &claims["ehdsi"];
    assert_eq!(ehdsi["professional"]["hp_identifier"], HP_ID, "{claims}");
    assert_eq!(ehdsi["professional"]["name"], HP_NAME, "{claims}");
    assert_eq!(
        ehdsi["provider"]["healthcare_provider_identifier"],
        ORGANIZATION_ID
    );
    assert_eq!(ehdsi["purpose_of_use"], "TREATMENT");
    assert_eq!(ehdsi["assertion"]["id"], IDA);
    assert_eq!(ehdsi["trc"]["id"], TRC);
    assert_eq!(
        ehdsi["patient"]["id"],
        format!("{NAMESPACE}|{PATIENT}"),
        "SAML Profile 10.0.0 §4.3: the TRC patient as a system|value token"
    );
    let exp = claims["exp"].as_i64().ok_or("an exp")?;
    assert!(exp <= window, "within both windows: {exp} > {window}");
    assert!(
        exp <= now + ferrofed_testkit::ehdsi_exchange::LIFETIME + 1,
        "within the harness lifetime: {exp}"
    );

    let bound_gateway = gateway().await?;
    let (status, text) = answered(&bound_gateway, bound(by_subject(PATIENT)?, &token)?).await?;
    assert_ne!(StatusCode::FORBIDDEN, status, "{text}");
    assert_ne!(StatusCode::UNAUTHORIZED, status, "{text}");
    let at_a = asked(&bound_gateway.a).await?;
    assert!(
        at_a.contains(&(Method::GET.to_string(), format!("/v1/ehr/{EHR_A}"))),
        "the TRC patient's EHR at node A is read: {status} {text} {at_a:?}"
    );
    Ok(())
}
