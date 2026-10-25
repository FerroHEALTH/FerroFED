// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness national connector, offline: its token carries every
//! attribute of Implementing Regulation (EU) 2026/2099 Annex Tables 1 and 2
//! and a purpose of use, synthetic and inside the `urn:oid:2.999` example
//! arc; it confirms a national identifier at a stub PDQm Supplier over
//! ITI-78; and each call reaches the FHIR face's route with the token and a
//! correlation identifier of its own.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: a token's claims are read as JSON values"
)]

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_testkit::issuer::Issuer;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::national_connector::{
    CLAIMS, CLIENT_ID, CORRELATION_HEADER, COUNTRY, FAMILY, GIVEN, HCP_ADDRESS, HCP_AUTHORITY,
    HCP_ID, HCP_NAME, HP_AUTHORITY, HP_ID, NationalConnector, NationalId, PERSON_ID, ROLE,
    ROLE_SYSTEM,
};
use ferrofed_testkit::pdqm_stub::{self, Entry};
use http::StatusCode;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The audience the connector's tokens name.
const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// The synthetic patient, in the example arc.
const PATIENT: NationalId<'static> = NationalId {
    system: "urn:oid:2.999.1.1",
    value: "Qz9-national-38",
};

/// A connector over the face at `base`.
fn connector(base: &str) -> Result<NationalConnector, Box<dyn Error>> {
    Ok(NationalConnector::new(
        Issuer::new("https://ncp.example.test")?,
        AUDIENCE,
        format!("{base}/fhir/"),
    )?)
}

/// The claims of `token`, read without verifying it.
fn claims_of(token: &str) -> Result<serde_json::Map<String, serde_json::Value>, Box<dyn Error>> {
    Ok(jsonwebtoken::dangerous::insecure_decode_claims::<
        serde_json::Map<String, serde_json::Value>,
    >(token)?)
}

#[test]
fn the_token_carries_every_annex_attribute_and_a_purpose_of_use() -> TestResult {
    let connector = connector("http://127.0.0.1:9")?;
    let claims = claims_of(&connector.token()?)?;
    assert_eq!(claims["client_id"], CLIENT_ID);
    assert_eq!(claims["sub"], CLIENT_ID, "a client token");
    assert_eq!(claims["aud"], AUDIENCE);
    let iua = &claims["extensions"]["ihe_iua"];
    assert_eq!(iua["national_provider_identifier"], HP_ID);
    assert_eq!(iua["subject_organization_id"], HCP_ID);
    assert_eq!(iua["subject_organization"], HCP_NAME);
    assert_eq!(iua["subject_role"][0]["system"], ROLE_SYSTEM);
    assert_eq!(iua["subject_role"][0]["code"], ROLE);
    assert_eq!(iua["purpose_of_use"][0]["code"], "TREAT");
    assert_eq!(iua["person_id"], PERSON_ID);
    for (claim, value) in [
        (CLAIMS.family_name, FAMILY),
        (CLAIMS.given_name, GIVEN),
        (CLAIMS.country_code, COUNTRY),
        (CLAIMS.professional_issuing_authority, HP_AUTHORITY),
        (CLAIMS.provider_issuing_authority, HCP_AUTHORITY),
        (CLAIMS.provider_address, HCP_ADDRESS),
    ] {
        assert_eq!(claims[claim], value, "{claim}");
    }
    Ok(())
}

#[test]
fn every_identifier_the_token_relays_is_in_the_example_arc() {
    for identifier in [HP_ID, HCP_ID, ROLE_SYSTEM] {
        assert!(identifier.starts_with("urn:oid:2.999."), "{identifier}");
    }
}

#[test]
fn the_declared_claim_names_are_the_ones_the_token_carries() -> TestResult {
    let table: toml::Table = toml::from_str(&CLAIMS.toml())?;
    let declared = &table["trust"]["issuer"]["national_contact_point"];
    assert_eq!(
        declared["correlation_header"].as_str(),
        Some(CORRELATION_HEADER)
    );
    let claims = claims_of(&connector("http://127.0.0.1:9")?.token()?)?;
    for key in [
        "family_name",
        "given_name",
        "country_code",
        "professional_issuing_authority",
        "provider_issuing_authority",
        "provider_address",
    ] {
        let name = declared[key].as_str().ok_or(key)?;
        assert!(claims.contains_key(name), "{key}: {name}");
    }
    Ok(())
}

#[tokio::test]
async fn a_national_identifier_is_confirmed_only_for_one_active_patient() -> TestResult {
    let supplier = pdqm_stub::by_identifier(&[
        (
            PATIENT.value,
            &[Entry::new(&[(PATIENT.system, PATIENT.value)])],
        ),
        (
            "Qz9-deprecated-39",
            &[Entry::new(&[(PATIENT.system, "Qz9-deprecated-39")]).deprecated()],
        ),
    ])
    .await;
    let connector = connector("http://127.0.0.1:9")?;
    assert!(
        connector
            .confirm(&pdqm_stub::base_url(&supplier), PATIENT)
            .await?
    );
    for unconfirmed in ["Qz9-deprecated-39", "Qz9-unknown-40"] {
        let patient = NationalId {
            system: PATIENT.system,
            value: unconfirmed,
        };
        assert!(
            !connector
                .confirm(&pdqm_stub::base_url(&supplier), patient)
                .await?,
            "{unconfirmed}"
        );
    }
    assert_eq!(
        3,
        pdqm_stub::searches(&supplier).await,
        "one ITI-78 search per confirmation"
    );
    Ok(())
}

#[tokio::test]
async fn each_call_reaches_its_route_with_the_token_and_its_own_correlation() -> TestResult {
    let face = Server::start().await;
    for (verb, route, status) in [
        ("GET", "/fhir/Patient/$summary", 200),
        ("GET", "/fhir/DocumentReference", 200),
        ("GET", "/fhir/MedicationRequest", 200),
        ("GET", "/fhir/MedicationRequest/rx-1", 200),
        ("POST", "/fhir/MedicationDispense", 201),
    ] {
        Mock::given(method(verb))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status).insert_header(
                "location",
                "https://gw.example.org/fhir/MedicationDispense/md-1/_history/1",
            ))
            .mount(&face)
            .await;
    }
    let connector = connector(&face.uri())?;
    let answers = [
        connector.summary(PATIENT).await?,
        connector.documents(PATIENT).await?,
        connector.prescriptions(PATIENT).await?,
        connector.prescription("rx-1").await?,
        connector
            .dispense(r#"{"resourceType":"MedicationDispense"}"#)
            .await?,
    ];
    let statuses: Vec<StatusCode> = answers.iter().map(|answer| answer.status).collect();
    assert_eq!(
        vec![
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::CREATED
        ],
        statuses
    );
    assert!(answers[4].location().is_some_and(|at| at.contains("md-1")));

    let requests = face.received_requests().await.ok_or("recording is on")?;
    assert_eq!(5, requests.len());
    let token = format!("{}|{}", PATIENT.system, PATIENT.value);
    let mut correlations = BTreeSet::new();
    for (request, answer) in requests.iter().zip(&answers) {
        let bearer = request
            .headers
            .get("authorization")
            .ok_or("a token")?
            .to_str()?;
        let claims = claims_of(bearer.strip_prefix("Bearer ").ok_or("the Bearer scheme")?)?;
        assert_eq!(claims[CLAIMS.family_name], FAMILY);
        let correlation = request
            .headers
            .get(CORRELATION_HEADER)
            .ok_or("a correlation identifier")?
            .to_str()?;
        assert_eq!(answer.correlation, correlation);
        correlations.insert(correlation.to_owned());
        if request.url.path() != "/fhir/MedicationRequest/rx-1"
            && request.method == http::Method::GET
        {
            let searched: Vec<String> = request
                .url
                .query_pairs()
                .map(|(_, value)| value.into_owned())
                .collect();
            assert_eq!(vec![token.clone()], searched, "{}", request.url);
        }
    }
    assert_eq!(5, correlations.len(), "one correlation identifier per call");
    Ok(())
}
