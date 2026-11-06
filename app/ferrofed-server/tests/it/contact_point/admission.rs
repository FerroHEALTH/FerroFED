// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What a national contact point's token needs at the gate: what a
//! conforming eHDSI HP Identity Assertion and Treatment Relationship
//! Confirmation provide (eHDSI SAML Profile 10.0.0 §2.1, §2.3), as the
//! shared contact-point profile of `ferrohealth-smart` checks it, bound to
//! the client certificate of its connection (RFC 8705 §3), and a
//! correlation header in its declared form; a request that lacks one is
//! refused and reaches no node. The gateway checks no field itself: each
//! test here holds FerroFED's use of the shared refusal.

use axum::body::Body;
use ferrofed_server::auth::refusal::Refusal;
use http::{Method, Request, StatusCode};
use serde_json::json;

use super::{
    CORRELATION, CORRELATION_HEADER, HP_ID, declared, presenting, relaying, told, with, without,
};
use crate::auth::{
    Gateway, TestResult, assert_admitted, assert_refused, bearing, minted, query, sent,
};

/// SAML Profile 10.0.0 §2.1, §2.3: a token carrying exactly what a
/// conforming assertion provides, and no Annex field it does not, reaches
/// patient data.
#[tokio::test]
async fn a_token_with_what_a_conforming_assertion_provides_is_admitted() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    assert_admitted(&gateway, presenting(query()?, &relaying())?).await
}

/// SAML Profile 10.0.0 §2.1: the professional is named by the assertion's
/// `NameID`, which is the identifier a node is told.
#[tokio::test]
async fn the_hp_identifier_is_the_name_id() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    assert_admitted(&gateway, presenting(query()?, &relaying())?).await?;
    let conveyed = told(&gateway.a)
        .await?
        .national_contact_point
        .ok_or("the relayed attributes")?;
    assert_eq!(HP_ID, conveyed.health_professional.hp_identifier);
    Ok(())
}

/// SAML Profile 10.0.0 §2.3: a purpose of use other than `TREATMENT` or
/// `EMERGENCY` is refused, and so is none, even where the deployment
/// relaxed the purpose-of-use rule for its own callers.
#[tokio::test]
async fn a_purpose_of_use_other_than_treatment_or_emergency_is_refused() -> TestResult {
    let mut auth = declared();
    auth.purpose_required = false;
    let gateway = Gateway::with(auth).await?;
    for claims in [
        with("purpose_of_use", json!("TREAT")),
        with("purpose_of_use", json!("PUBLICHEALTH")),
        without("purpose_of_use"),
    ] {
        assert_refused(
            &gateway,
            presenting(query()?, &claims)?,
            Refusal::ContactPoint,
        )
        .await?;
    }
    gateway.nobody_asked().await
}

/// SAML Profile 10.0.0 §2.3: a token missing the facility type is refused.
#[tokio::test]
async fn a_token_missing_the_facility_type_is_refused() -> TestResult {
    refused_without("provider.facility_type").await
}

/// SAML Profile 10.0.0 §2.3: a token missing the home community is refused.
#[tokio::test]
async fn a_token_missing_the_home_community_is_refused() -> TestResult {
    refused_without("provider.home_community_id").await
}

/// SAML Profile 10.0.0 §2.3: a token missing the point of care is refused.
#[tokio::test]
async fn a_token_missing_the_point_of_care_is_refused() -> TestResult {
    refused_without("point_of_care").await
}

/// SAML Profile 10.0.0 §2.1: a token missing the authentication class is
/// refused.
#[tokio::test]
async fn a_token_missing_the_authentication_class_is_refused() -> TestResult {
    refused_without("professional.authentication_method").await
}

/// SAML Profile 10.0.0 §2.3: `on-behalf-of` is mandatory for a clerical
/// support worker, ISCO 44, so a token with that role and none is refused,
/// and one that names it is admitted.
#[tokio::test]
async fn a_clerical_role_without_on_behalf_of_is_refused() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let clerical = json!([{ "system": "2.16.840.1.113883.2.9.6.2.7", "code": "44" }]);
    let without_whom = with("professional.hp_professional_role", clerical);
    assert_refused(
        &gateway,
        presenting(query()?, &without_whom)?,
        Refusal::ContactPoint,
    )
    .await?;
    gateway.nobody_asked().await?;
    let mut acting_for = without_whom;
    acting_for.ehdsi.insert(
        String::from("professional.on_behalf_of"),
        json!([{ "system": "2.16.840.1.113883.2.9.6.2.7", "code": "221" }]),
    );
    assert_admitted(&gateway, presenting(query()?, &acting_for)?).await
}

/// RFC 8705 §3, the contact point's `sender_constraint = "required"`: a
/// token presented on a connection without the certificate it is bound to,
/// or bound to nothing, is refused as a token that does not hold.
#[tokio::test]
async fn a_token_not_bound_to_the_connection_is_refused() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&relaying())?)?,
        Refusal::Invalid,
    )
    .await?;
    let mut unbound = relaying();
    unbound.cnf = None;
    assert_refused(&gateway, presenting(query()?, &unbound)?, Refusal::Invalid).await?;
    gateway.nobody_asked().await
}

/// The shared profile refuses the token itself, so a short token reaches no
/// route, the self-description included.
#[tokio::test]
async fn a_short_token_is_refused_on_every_route() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let short = without("point_of_care");
    let read = Request::get(format!("/v1/ehr/{}", crate::facade::EHR_A)).body(Body::empty())?;
    let options = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())?;
    for request in [read, options] {
        assert_refused(
            &gateway,
            presenting(request, &short)?,
            Refusal::ContactPoint,
        )
        .await?;
    }
    gateway.nobody_asked().await
}

/// A correlation identifier in its declared form is admitted; one that is
/// repeated, empty, too long or not visible ASCII is `400
/// correlation-invalid`, with nothing sent.
#[tokio::test]
async fn a_correlation_header_out_of_its_form_is_400_with_nothing_sent() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let mut fine = presenting(query()?, &relaying())?;
    fine.headers_mut()
        .insert(CORRELATION_HEADER, CORRELATION.parse()?);
    assert_admitted(&gateway, fine).await?;

    let gateway = Gateway::with(declared()).await?;
    let too_long = "a".repeat(129);
    for values in [
        vec![CORRELATION, CORRELATION],
        vec![""],
        vec![too_long.as_str()],
        vec!["with space"],
    ] {
        let mut request = presenting(query()?, &relaying())?;
        for value in &values {
            request
                .headers_mut()
                .append(CORRELATION_HEADER, value.parse()?);
        }
        assert_refused(&gateway, request, Refusal::Correlation).await?;
    }
    Ok(())
}

/// An issuer that is no contact point is held to none of this: the suite's
/// default token reaches patient data with no eHDSI attribute and no
/// binding.
#[tokio::test]
async fn an_issuer_that_is_no_contact_point_needs_no_ehdsi_attribute() -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    assert_admitted(
        &gateway,
        bearing(query()?, &minted(&crate::support::caller::claims())?)?,
    )
    .await
}

/// A token of [`relaying`] without `field` is refused at the gate as the
/// contact point's shortfall, and no node is asked anything.
async fn refused_without(field: &str) -> TestResult {
    let gateway = Gateway::with(declared()).await?;
    let request = presenting(query()?, &without(field))?;
    let (status, _, text) = sent(&gateway.app, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{field}: {text}");
    assert!(
        text.contains("contact-point-attributes-required"),
        "{field}: {text}"
    );
    gateway.nobody_asked().await
}
