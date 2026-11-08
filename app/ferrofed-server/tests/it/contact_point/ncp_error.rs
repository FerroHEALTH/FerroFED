// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every refusal to a national contact point names its MyHealth@EU
//! Requirements Catalogue 10.0.0 §1.25 exception code in the error body's
//! `ncp_error` member and in the access record's outcome description, so its
//! national connector maps the refusal one to one; no legacy numeric code of
//! the `NCPeH` Components Specification 10.0.0 appendix 6.4 is issued.
//!
//! A refused token takes the code `ferrohealth-smart`'s refusal carries
//! (`Denied::ehdsi`, `EhdsiCode::of` for any other invalid token); the
//! gateway holds no table for it, and each test compares with the crate's
//! own code. The gateway's own refusals take the code of their catalogue
//! row. A caller that is no contact point gets no code.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the error body and the access record are read as JSON"
)]

use std::error::Error;
use std::sync::Arc;

use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::unreachable;
use ferrohealth_smart::federation::contact::EhdsiCode;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::Rejection;
use http::StatusCode;
use serde_json::{Value, json};

use super::emergency::{asked, contact_point_router, emergency_settings, records, relayed_for};
use super::{declared, presenting, relaying, with, without};
use crate::auth::{Gateway, TestResult, bearing, claims, minted, query, sent};

/// The `ncp_error` member of the error body `text`, when it has one.
fn ncp_error(text: &str) -> Result<Option<String>, Box<dyn Error>> {
    let body: Value = serde_json::from_str(text)?;
    Ok(body
        .get("ncp_error")
        .and_then(Value::as_str)
        .map(str::to_owned))
}

/// Asserts that `code` is a §1.25 code name and no appendix 6.4 number.
fn named(code: &str) {
    assert!(code.starts_with("ERROR_"), "{code}");
    assert!(
        code.bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte == b'_'),
        "a code name, never a number: {code}"
    );
}

/// Sends the patient query with a contact point's token over `claims` and
/// asserts it is refused `status` with `code` in the body.
async fn refused_as(claims: &Claims, status: StatusCode, code: EhdsiCode) -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let (answered, _, text) = sent(&gateway.app, presenting(query()?, claims)?).await?;
    assert_eq!(status, answered, "{text}");
    let carried = ncp_error(&text)?.ok_or("the refusal names its code")?;
    assert_eq!(code.as_str(), carried, "{text}");
    named(&carried);
    gateway.nobody_asked().await
}

/// RC §1.25, the national token row: an expired contact-point token takes
/// the code the shared refusal gives an invalid token.
#[tokio::test]
async fn an_invalid_token_carries_the_code_the_shared_refusal_gives() -> TestResult {
    let mut expired = relaying();
    expired.exp = jiff::Timestamp::now().as_second() - 3_600;
    expired.iat = expired.exp - 600;
    let shared = EhdsiCode::of(&Rejection::Invalid {
        reason: Reason::Expired,
    });
    refused_as(&expired, StatusCode::UNAUTHORIZED, shared).await
}

/// RC §1.25 requirement 01: a token that names no professional.
#[tokio::test]
async fn no_professional_is_hpi_no_information() -> TestResult {
    let mut claims = relaying();
    claims
        .ehdsi
        .retain(|field, _| !field.starts_with("professional."));
    refused_as(&claims, StatusCode::FORBIDDEN, EhdsiCode::HpiNoInformation).await
}

/// RC §1.25 requirement 01: the `NameID` missing.
#[tokio::test]
async fn a_missing_hp_identifier_is_hpi_insufficient_information() -> TestResult {
    refused_as(
        &without("professional.hp_identifier"),
        StatusCode::FORBIDDEN,
        EhdsiCode::HpiInsufficientInformation,
    )
    .await
}

/// RC §1.25 requirement 01: `organization-id` missing.
#[tokio::test]
async fn a_missing_provider_is_hpi_hai_no_information() -> TestResult {
    refused_as(
        &without("provider.healthcare_provider_identifier"),
        StatusCode::FORBIDDEN,
        EhdsiCode::HpiHaiNoInformation,
    )
    .await
}

/// RC §1.25 requirement 01: the point of care missing.
#[tokio::test]
async fn a_missing_point_of_care_is_hpi_poc_no_information() -> TestResult {
    refused_as(
        &without("point_of_care"),
        StatusCode::FORBIDDEN,
        EhdsiCode::HpiPocNoInformation,
    )
    .await
}

/// RC §1.25 requirement 01.01: the authentication class missing.
#[tokio::test]
async fn a_missing_authentication_class_is_hpi_authentication_not_received() -> TestResult {
    refused_as(
        &without("professional.authentication_method"),
        StatusCode::FORBIDDEN,
        EhdsiCode::HpiAuthenticationNotReceived,
    )
    .await
}

/// RC §1.25 requirement 01: a purpose of use other than `TREATMENT` or
/// `EMERGENCY`, the code the shared profile gives it.
#[tokio::test]
async fn another_purpose_of_use_is_hpi_insufficient_information() -> TestResult {
    refused_as(
        &with("purpose_of_use", json!("TREAT")),
        StatusCode::FORBIDDEN,
        EhdsiCode::HpiInsufficientInformation,
    )
    .await
}

/// RC §1.25 requirement 11.01.02: the identity assertion's id missing.
#[tokio::test]
async fn a_missing_assertion_is_sec_data_integrity_not_ensured() -> TestResult {
    refused_as(
        &without("assertion.id"),
        StatusCode::FORBIDDEN,
        EhdsiCode::SecDataIntegrityNotEnsured,
    )
    .await
}

/// RC §1.25 requirement 11.01.02: a patient-data request whose token names
/// no TRC patient misses an assertion attribute, and is refused
/// `trc-missing` with `ERROR_SEC_DATA_INTEGRITY_NOT_ENSURED` before any
/// member is asked (eHDSI SAML Profile 10.0.0 §4.3).
#[tokio::test]
async fn no_trc_patient_is_the_data_integrity_code() -> TestResult {
    refused_as(
        &without("patient.id"),
        StatusCode::FORBIDDEN,
        EhdsiCode::SecDataIntegrityNotEnsured,
    )
    .await
}

/// RC §1.25 requirement 11: a scope refusal to a contact point is
/// `ERROR_SEC_GENERIC` (eHDSI XCA Profile 8.0.1 §2.2.3).
#[tokio::test]
async fn a_scope_refusal_is_the_generic_security_code() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let mut unscoped = relaying();
    unscoped.scope = Some(String::from("user/template-*.r"));
    let (status, _, text) = sent(&gateway.app, presenting(query()?, &unscoped)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!(Some(String::from("ERROR_SEC_GENERIC")), ncp_error(&text)?);
    Ok(())
}

/// Off the contact-point path a refusal keeps its own code and names no
/// §1.25 code.
#[tokio::test]
async fn a_refusal_to_another_caller_names_no_exception_code() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let mut other = claims();
    other.scope = Some(String::from("user/template-*.r"));
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&other)?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!(None, ncp_error(&text)?, "{text}");
    Ok(())
}

/// RC §1.25 requirements 05.01, 06.01 and 07.01: an emergency access the
/// gateway's own store cannot count is the gateway's own failure,
/// `ERROR_INTERNAL_ERROR`, in the body and in the access record.
#[tokio::test]
async fn an_uncounted_emergency_access_is_an_internal_error_in_the_body_and_the_record()
-> TestResult {
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = emergency_settings(
        dir.path(),
        (
            &format!("{}/a", unreachable::BASE),
            &format!("{}/b", unreachable::BASE),
        ),
        &repository,
        "",
    )?;
    let app = contact_point_router(Arc::new(AppState::build(&settings)?));
    let (status, text) = asked(&app, &relayed_for("Qz7-hp-91", "EMERGENCY")).await?;
    assert_eq!(StatusCode::SERVICE_UNAVAILABLE, status, "{text}");
    assert_eq!(
        Some(String::from("ERROR_INTERNAL_ERROR")),
        ncp_error(&text)?,
        "{text}"
    );
    let [record] = records(&repository, 1)
        .await?
        .try_into()
        .map_err(|_unexpected| "one record")?;
    let described = record["outcomeDesc"].as_str().unwrap_or_default();
    assert!(described.starts_with("ERROR_INTERNAL_ERROR"), "{record}");
    Ok(())
}
