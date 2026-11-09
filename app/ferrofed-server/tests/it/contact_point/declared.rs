// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `OPTIONS {base}/` declares the national-contact-point setting (§7a.2,
//! N30) under `federation.national_contact_point`: that such a request is
//! served with `disclose: false` (Regulation (EU) 2025/327 Art 8, Art
//! 11(5)), that the professional is the contact point's assertion and never
//! authenticated by the gateway (Implementing Regulation (EU) 2026/2099
//! Art 6(1) and (2); §13.4 authn-end-user, CP-39), that a purpose of use is
//! required, the emergency access limit in effect (eHDSI Section II Security
//! Services 10.0.0 §2.5), and, per issuer, the claim each attribute is read
//! from. A deployment that declares no contact point declares nothing.

use std::collections::BTreeMap;
use std::error::Error;

use axum::body::Body;
use http::{Method, Request, StatusCode};
use serde::Deserialize;

use super::{CORRELATION_HEADER, NCP_ISSUER, declared};
use crate::auth::{Gateway, TestResult, bearing, minted, sent};
use crate::facade::schema;
use crate::support::caller::claims;

/// The members of `OPTIONS {base}/` this suite reads.
#[derive(Debug, Deserialize)]
struct Options {
    federation: DeclaredFederation,
}

#[derive(Debug, Deserialize)]
struct DeclaredFederation {
    national_contact_point: Option<DeclaredContactPoint>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredContactPoint {
    disclose: bool,
    professional: String,
    purpose_of_use: String,
    emergency: DeclaredEmergency,
    issuers: Vec<DeclaredIssuer>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DeclaredEmergency {
    threshold: u32,
    window_s: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredIssuer {
    issuer: String,
    claims: BTreeMap<String, String>,
    correlation_header: Option<String>,
}

/// The schema-validated `OPTIONS {base}/` body `gateway` answers its
/// default caller with, read.
async fn described(gateway: &Gateway) -> Result<Options, Box<dyn Error>> {
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())?;
    let (status, _, text) = sent(&gateway.app, bearing(request, &minted(&claims())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    Ok(serde_json::from_str(&text)?)
}

// conformance: CP-23
#[tokio::test]
async fn a_declared_contact_point_is_described_with_how_its_requests_are_served() -> TestResult {
    let options = described(&Gateway::with(declared()).await?).await?;
    let member = options
        .federation
        .national_contact_point
        .ok_or("the member is declared")?;
    assert!(!member.disclose, "Art 8, Art 11(5)");
    assert_eq!("asserted-by-contact-point", member.professional, "CP-39");
    assert_eq!("required", member.purpose_of_use, "§13.4");
    assert_eq!(
        DeclaredEmergency {
            threshold: 5,
            window_s: 86_400
        },
        member.emergency,
        "the §2.5 baseline, in effect when [emergency] is not written"
    );
    let [issuer] = member.issuers.as_slice() else {
        return Err("one contact point".into());
    };
    assert_eq!(NCP_ISSUER, issuer.issuer);
    assert_eq!(
        Some(CORRELATION_HEADER),
        issuer.correlation_header.as_deref()
    );
    for (field, claim) in [
        (
            "professional.hp_identifier",
            "ehdsi.professional.hp_identifier",
        ),
        (
            "provider.healthcare_provider_identifier",
            "ehdsi.provider.healthcare_provider_identifier",
        ),
        ("purpose_of_use", "ehdsi.purpose_of_use"),
    ] {
        assert_eq!(
            Some(claim),
            issuer.claims.get(field).map(String::as_str),
            "{field}"
        );
    }
    for configured in [
        "professional.country_code",
        "professional.issuing_authority_name",
        "provider.issuing_authority_name",
    ] {
        assert!(
            !issuer.claims.contains_key(configured),
            "a configured value is no claim: {configured}"
        );
    }
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn a_tightened_emergency_limit_is_declared_as_the_one_in_effect() -> TestResult {
    let gateway =
        Gateway::with_rows(declared(), "\n[emergency]\nthreshold = 2\nwindow = '1h'\n").await?;
    let member = described(&gateway)
        .await?
        .federation
        .national_contact_point
        .ok_or("the member is declared")?;
    assert_eq!(
        DeclaredEmergency {
            threshold: 2,
            window_s: 3600
        },
        member.emergency,
        "a country tightens the §2.5 baseline"
    );
    Ok(())
}

// conformance: CP-23
#[tokio::test]
async fn a_deployment_with_no_contact_point_declares_none() -> TestResult {
    let options = described(&Gateway::trusting_the_test_issuer().await?).await?;
    assert!(options.federation.national_contact_point.is_none());
    Ok(())
}
