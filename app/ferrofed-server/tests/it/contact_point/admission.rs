// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a national contact point's token needs at the gate: every attribute
//! of Implementing Regulation (EU) 2026/2099 Annex Tables 1 and 2 (Art 7)
//! and a purpose of use (§13.4 authn-purpose-of-use) for patient data, and
//! a correlation header in its declared form; a request that lacks one is
//! refused and reaches no node.

use axum::body::Body;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_testkit::issuer::Claims;
use http::{Method, Request, StatusCode};

use super::{CORRELATION, CORRELATION_HEADER, claim_names, declared, relaying};
use crate::auth::{
    Gateway, TestResult, assert_admitted, assert_refused, bearing, minted, query, sent,
};

/// Every way a token can fall short of the Annex: each attribute removed,
/// emptied, or, for the country, not in the ISO 3166-1 alpha-2 form.
fn short_of_the_annex() -> Vec<(&'static str, Claims)> {
    let names = claim_names();
    let without = |claim: String| {
        let mut claims = relaying();
        claims.other.remove(&claim);
        claims
    };
    let iua = |change: fn(&mut ferrofed_testkit::issuer::IheIua)| {
        let mut claims = relaying();
        if let Some(extensions) = claims.extensions.as_mut() {
            change(&mut extensions.ihe_iua);
        }
        claims
    };
    let mut lowercase_country = relaying();
    lowercase_country
        .other
        .insert(names.country_code.clone(), String::from("xa"));
    let mut empty_family = relaying();
    empty_family
        .other
        .insert(names.family_name.clone(), String::new());
    vec![
        ("family_name", without(names.family_name)),
        ("given_name", without(names.given_name)),
        ("country_code", without(names.country_code)),
        (
            "professional issuing_authority_name",
            without(names.professional_issuing_authority),
        ),
        (
            "provider issuing_authority_name",
            without(names.provider_issuing_authority),
        ),
        (
            "healthcare_provider_address",
            without(names.provider_address),
        ),
        (
            "hp_identifier",
            iua(|iua| iua.national_provider_identifier = None),
        ),
        ("hp_professional_role", iua(|iua| iua.subject_role.clear())),
        (
            "healthcare_provider_identifier",
            iua(|iua| iua.subject_organization_id = None),
        ),
        (
            "healthcare_provider_name",
            iua(|iua| iua.subject_organization = None),
        ),
        ("a country code not in the alpha-2 form", lowercase_country),
        ("an empty family_name", empty_family),
    ]
}

/// 2026/2099 Art 7, Annex Tables 1 and 2: a contact point's token that
/// carries every attribute and a purpose of use reaches patient data.
#[tokio::test]
async fn a_token_with_every_annex_attribute_is_admitted() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    assert_admitted(&gateway, bearing(query()?, &minted(&relaying())?)?).await
}

/// 2026/2099 Art 7: a patient-data request missing any Annex attribute is
/// `403 contact-point-attributes-required`, and no node is asked anything.
#[tokio::test]
async fn a_request_missing_any_annex_attribute_is_403_with_nothing_sent() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    for (missing, claims) in short_of_the_annex() {
        let request = bearing(query()?, &minted(&claims)?)?;
        let (status, _, text) = sent(&gateway.app, request).await?;
        assert_eq!(StatusCode::FORBIDDEN, status, "{missing}: {text}");
        assert!(
            text.contains("contact-point-attributes-required"),
            "{missing}: {text}"
        );
    }
    gateway.nobody_asked().await
}

/// The refusal of a short token carries its challenge, as every `403` of the
/// gate does (RFC 6750 §3).
#[tokio::test]
async fn the_refusal_names_its_reason_in_the_challenge() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let mut claims = relaying();
    claims.other.clear();
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&claims)?)?,
        Refusal::ContactPoint,
    )
    .await
}

/// §13.4 authn-purpose-of-use: a contact point's patient-data request with
/// no purpose of use is `403 purpose-of-use-required`, even where the
/// deployment relaxed the rule for its own callers.
#[tokio::test]
async fn a_request_with_no_purpose_of_use_is_403_whatever_the_deployment_relaxed() -> TestResult {
    let mut auth = declared()?;
    auth.purpose_required = false;
    let gateway = Gateway::with(auth).await?;
    let mut claims = relaying();
    if let Some(extensions) = claims.extensions.as_mut() {
        extensions.ihe_iua.purpose_of_use.clear();
    }
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&claims)?)?,
        Refusal::PurposeOfUse,
    )
    .await
}

/// The rule holds on every patient-data route: an EHR read is refused as a
/// query is.
#[tokio::test]
async fn an_ehr_read_missing_an_attribute_is_refused() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let mut claims = relaying();
    claims.other.clear();
    let read = Request::get(format!("/v1/ehr/{}", crate::facade::EHR_A)).body(Body::empty())?;
    assert_refused(
        &gateway,
        bearing(read, &minted(&claims)?)?,
        Refusal::ContactPoint,
    )
    .await
}

/// The self-description reaches no patient data, so a contact point's
/// token needs no Annex attribute for it.
#[tokio::test]
async fn the_self_description_needs_no_annex_attribute() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let mut claims = relaying();
    claims.other.clear();
    let options = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())?;
    let (status, _, text) = sent(&gateway.app, bearing(options, &minted(&claims)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    gateway.nobody_asked().await
}

/// A correlation identifier in its declared form is admitted; one that is
/// repeated, empty, too long or not visible ASCII is `400
/// correlation-invalid`, with nothing sent.
#[tokio::test]
async fn a_correlation_header_out_of_its_form_is_400_with_nothing_sent() -> TestResult {
    let gateway = Gateway::with(declared()?).await?;
    let token = minted(&relaying())?;
    let mut fine = bearing(query()?, &token)?;
    fine.headers_mut()
        .insert(CORRELATION_HEADER, CORRELATION.parse()?);
    assert_admitted(&gateway, fine).await?;

    let gateway = Gateway::with(declared()?).await?;
    let too_long = "a".repeat(129);
    for values in [
        vec![CORRELATION, CORRELATION],
        vec![""],
        vec![too_long.as_str()],
        vec!["with space"],
    ] {
        let mut request = bearing(query()?, &token)?;
        for value in &values {
            request
                .headers_mut()
                .append(CORRELATION_HEADER, value.parse()?);
        }
        assert_refused(&gateway, request, Refusal::Correlation).await?;
    }
    Ok(())
}

/// An issuer not declared a contact point is held to none of this: the
/// suite's default token reaches patient data with no Annex attribute.
#[tokio::test]
async fn an_issuer_not_declared_a_contact_point_needs_no_annex_attribute() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    assert_admitted(
        &gateway,
        bearing(query()?, &minted(&crate::support::claims())?)?,
    )
    .await
}
