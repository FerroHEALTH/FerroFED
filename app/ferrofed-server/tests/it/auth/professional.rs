// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Who acts behind a token, and how they authenticated, for a request that
//! reaches patient data: Regulation (EU) 2025/327 Annex II 3.1 ("reliable
//! mechanisms for the identification and authentication of health
//! professionals"), the assurance levels of Regulation (EU) No 910/2014
//! Art 8(2) that Implementing Regulation (EU) 2026/2099 Art 6(3) cites, and
//! the RFC 9470 §3 challenge.
//!
//! A token names no natural person when its `sub` is its `client_id` (IHE
//! IUA ITI TF-2 3.71.4.2.2.1) or when only `system/` scopes cover the
//! operation (SMART on openEHR master08 §Resource Scopes). Which claim
//! values stand for which level, and the least level, are the deployment's
//! per issuer.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeMap;

use axum::body::Body;
use ferrofed_engine::conveyance::AssuranceLevel;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::auth::AuthSettings;
use ferrofed_server::config::auth::assurance::Assurance;
use ferrofed_testkit::issuer::Claims;
use http::{Request, StatusCode};

use super::{
    Gateway, TestResult, assert_admitted, assert_refused, bearing, claims, minted, query, sent,
};
use crate::conveyance::published;
use crate::support::{self, error_body};

/// The `acr` value the test issuer states for level low.
const LOW: &str = "urn:example:loa:low";

/// The `acr` value the test issuer states for level substantial.
const SUBSTANTIAL: &str = "urn:example:loa:substantial";

/// The `acr` value the test issuer states for level high.
const HIGH: &str = "urn:example:loa:high";

/// A synthetic professional identifier (IHE IUA
/// `national_provider_identifier`).
const PROFESSIONAL: &str = "urn:oid:2.999.7.1|hp-0042";

/// The suite's `[auth]`, its issuer reading `acr` with the three values
/// above, `minimum` the least level for patient data, and declaring its
/// client tokens as acting for a professional when `clients` is set.
fn requiring(minimum: AssuranceLevel, clients: bool) -> AuthSettings {
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer.assurance = Some(Assurance {
            claim: String::from("acr"),
            minimum,
            values: BTreeMap::from([
                (LOW.to_owned(), AssuranceLevel::Low),
                (SUBSTANTIAL.to_owned(), AssuranceLevel::Substantial),
                (HIGH.to_owned(), AssuranceLevel::High),
            ]),
        });
        issuer.client_tokens_act_for_professional = clients;
    }
    auth
}

/// The suite's default claims stating `acr`.
fn stating(acr: &str) -> Claims {
    let mut stated = claims();
    stated.other.insert(String::from("acr"), acr.to_owned());
    stated
}

/// `claims` as a client token: its `sub` is its `client_id`.
fn as_client(mut claims: Claims) -> Claims {
    claims.sub.clone_from(&claims.client_id);
    claims
}

/// `claims` naming [`PROFESSIONAL`] in the IUA extension.
fn naming_the_professional(mut claims: Claims) -> Claims {
    if let Some(extensions) = claims.extensions.as_mut() {
        extensions.ihe_iua.national_provider_identifier = Some(PROFESSIONAL.to_owned());
        extensions.ihe_iua.subject_name = Some(String::from("Example Clinician"));
    }
    claims
}

/// Annex II 3.1, 2026/2099 Art 6(3): a token below the issuer's least level
/// is a `401` with the RFC 9470 challenge, and reaches no node.
#[tokio::test]
async fn a_token_below_the_least_level_is_401_and_reaches_no_node() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::Substantial, false)).await?;
    let request = bearing(query()?, &minted(&stating(LOW))?)?;
    let (status, headers, text) = sent(&gateway.app, request).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    assert_eq!(
        "authentication-assurance-insufficient",
        error_body(&text)?.code
    );
    let challenge = headers
        .get(http::header::WWW_AUTHENTICATE)
        .ok_or("a challenge")?
        .to_str()?;
    assert!(
        challenge.contains("error=\"insufficient_user_authentication\""),
        "RFC 9470 §3: {challenge}"
    );
    gateway.nobody_asked().await
}

/// A token that states no assurance, or a value the issuer declares at no
/// level, states no level, which no minimum admits.
#[tokio::test]
async fn a_token_without_the_claim_or_with_an_undeclared_value_is_refused() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::Low, false)).await?;
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&claims())?)?,
        Refusal::Assurance,
    )
    .await?;
    let undeclared = stating("urn:example:loa:unknown");
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&undeclared)?)?,
        Refusal::Assurance,
    )
    .await
}

/// A token at the least level and one above it are admitted.
#[tokio::test]
async fn a_token_at_or_above_the_least_level_is_admitted() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::Substantial, false)).await?;
    assert_admitted(
        &gateway,
        bearing(query()?, &minted(&stating(SUBSTANTIAL))?)?,
    )
    .await?;
    assert_admitted(&gateway, bearing(query()?, &minted(&stating(HIGH))?)?).await
}

/// The least level holds on every patient-data route: an EHR read is refused
/// below it as a query is.
#[tokio::test]
async fn an_ehr_read_below_the_least_level_is_refused() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::High, false)).await?;
    let read = Request::get(format!("/v1/ehr/{}", crate::facade::EHR_A)).body(Body::empty())?;
    assert_refused(
        &gateway,
        bearing(read, &minted(&stating(SUBSTANTIAL))?)?,
        Refusal::Assurance,
    )
    .await
}

/// A definition request reaches no patient data, so the least level does
/// not apply to it: the request passes the gate.
#[tokio::test]
async fn a_definition_request_needs_no_level() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::High, false)).await?;
    let read =
        Request::get("/v1/definition/template/adl1.4/org.example::vitals").body(Body::empty())?;
    let (status, _, text) = sent(&gateway.app, bearing(read, &minted(&stating(LOW))?)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "past the gate: {text}");
    assert_eq!("target-required", error_body(&text)?.code, "{text}");
    Ok(())
}

/// Annex II 3.1: a client token, whose `sub` is its `client_id` (IHE IUA
/// ITI TF-2 3.71.4.2.2.1), reaches no patient data by default, even when it
/// names a professional.
#[tokio::test]
async fn a_client_token_is_refused_patient_data_by_default() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let client = naming_the_professional(as_client(claims()));
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&client)?)?,
        Refusal::NaturalPerson,
    )
    .await
}

/// SMART on openEHR master08: a `system/` grant acts "without a user
/// context", so a token only it covers reaches no patient data by default,
/// whatever its `sub`.
#[tokio::test]
async fn a_token_only_a_system_scope_covers_is_refused_patient_data() -> TestResult {
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer
            .backend_clients
            .insert(String::from("synthetic-client"));
    }
    let gateway = Gateway::with(auth).await?;
    let mut backend = claims();
    backend.scope = Some(String::from("system/aql-*.s"));
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&backend)?)?,
        Refusal::NaturalPerson,
    )
    .await
}

/// An issuer whose client tokens the deployment declares as acting for a
/// professional admits one that names the professional, and still refuses
/// one that names none.
#[tokio::test]
async fn a_declared_issuer_admits_a_client_token_only_when_it_names_the_professional() -> TestResult
{
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer.client_tokens_act_for_professional = true;
    }
    let gateway = Gateway::with(auth).await?;
    let nameless = as_client(claims());
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&nameless)?)?,
        Refusal::NaturalPerson,
    )
    .await?;
    let named = naming_the_professional(as_client(claims()));
    assert_admitted(&gateway, bearing(query()?, &minted(&named)?)?).await
}

/// A declared client token is still held to the issuer's least level.
#[tokio::test]
async fn a_declared_client_token_is_held_to_the_least_level() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::Substantial, true)).await?;
    let low = naming_the_professional(as_client(stating(LOW)));
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&low)?)?,
        Refusal::Assurance,
    )
    .await?;
    let substantial = naming_the_professional(as_client(stating(SUBSTANTIAL)));
    assert_admitted(&gateway, bearing(query()?, &minted(&substantial)?)?).await
}

/// The node is told who acts, the professional's identification and the
/// level reached, in the conveyance the gateway signs (§13.1, N24), and in
/// nothing else of the request.
// conformance: CP-16
#[tokio::test]
async fn the_node_is_told_who_acts_and_the_level_reached() -> TestResult {
    let gateway = Gateway::with(requiring(AssuranceLevel::Substantial, true)).await?;
    let keys = published(&gateway.app).await?;
    let client = naming_the_professional(as_client(stating(HIGH)));
    assert_admitted(&gateway, bearing(query()?, &minted(&client)?)?).await?;
    let requests = gateway
        .a
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err("one query at node A".into());
    };
    let token = request
        .headers
        .get(ferrofed_engine::conveyance::HEADER)
        .ok_or("the conveyance")?
        .to_str()?;
    let read = crate::conveyance::verified(token, &keys, "node-a-pub")?;
    assert_eq!(
        (
            Some("client"),
            Some("high"),
            Some(PROFESSIONAL),
            Some("Example Clinician")
        ),
        (
            read.acting.as_deref(),
            read.assurance_level.as_deref(),
            read.national_provider_identifier.as_deref(),
            read.subject_name.as_deref()
        )
    );
    for (name, value) in &request.headers {
        if name.as_str() != ferrofed_engine::conveyance::HEADER.to_ascii_lowercase() {
            let value = value.to_str().unwrap_or_default();
            assert!(
                !value.contains(PROFESSIONAL) && !value.contains(HIGH),
                "{name} carries no caller claim: {value}"
            );
        }
    }
    Ok(())
}
