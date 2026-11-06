// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every refusal to a national contact point names its MyHealth@EU
//! Requirements Catalogue 10.0.0 §1.25 exception code in the error body's
//! `ncp_error` member and in the access record's outcome description, so its
//! national connector maps the refusal one to one; no legacy numeric code of
//! the `NCPeH` Components Specification 10.0.0 appendix 6.4 is issued.
//!
//! A refused token takes the code `ferrohealth-smart` gives its rejection
//! (`EhdsiCode::of`); the gateway's own refusals take the code of their
//! catalogue row. A caller that is no contact point gets no code.

use std::error::Error;
use std::sync::Arc;

use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use ferrofed_testkit::unreachable;
use ferrohealth_smart::federation::contact::EhdsiCode;
use ferrohealth_smart::refusal::Reason;
use ferrohealth_smart::resource::Rejection;
use http::StatusCode;
use serde_json::Value;

use super::emergency::{asked, contact_point_router, emergency_settings, records, relayed_for};
use super::{declared, relaying};
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

/// RC §1.25, the token row: an expired contact-point token is refused with
/// the code `ferrohealth-smart` gives its rejection, which the gateway holds
/// no table for.
#[tokio::test]
async fn a_refused_token_carries_the_code_the_shared_refusal_gives() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let mut expired = relaying();
    expired.exp = jiff::Timestamp::now().as_second() - 3_600;
    expired.iat = expired.exp - 600;
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&expired)?)?).await?;
    assert_eq!(StatusCode::UNAUTHORIZED, status, "{text}");
    let shared = EhdsiCode::of(&Rejection::Invalid {
        reason: Reason::Expired,
    });
    let carried = ncp_error(&text)?.ok_or("the refusal names its code")?;
    assert_eq!(shared.as_str(), carried, "{text}");
    named(&carried);
    Ok(())
}

/// RC §1.25 requirement 11: a scope refusal to a contact point is
/// `ERROR_SEC_GENERIC` (eHDSI XCA Profile 8.0.1 §2.2.3).
#[tokio::test]
async fn a_scope_refusal_is_the_generic_security_code() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let mut unscoped = relaying();
    unscoped.scope = Some(String::from("user/template-*.r"));
    let (status, _, text) = sent(&gateway.app, bearing(query()?, &minted(&unscoped)?)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!(Some(String::from("ERROR_SEC_GENERIC")), ncp_error(&text)?);
    Ok(())
}

/// Off the contact-point path a refusal keeps its own code and names no
/// §1.25 code.
#[tokio::test]
async fn a_refusal_to_another_caller_names_no_exception_code() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
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
    let app = contact_point_router(Arc::new(AppState::build(&settings)?))?;
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
