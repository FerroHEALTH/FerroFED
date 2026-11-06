// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The harness national connector, offline: its token carries what a
//! conforming eHDSI identity assertion provides and a purpose of use, bound
//! to its client certificate, synthetic and inside the `urn:oid:2.999`
//! example arc; it confirms a national identifier at a stub PDQm Supplier over
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
    AUTHENTICATION_CLASS, CLIENT_ID, CORRELATION_HEADER, COUNTRY, HCP_AUTHORITY, HCP_ID,
    HP_AUTHORITY, HP_ID, NationalConnector, NationalId, PERSON_ID, ROLE, ROLE_SYSTEM, cnf,
    contact_point_toml,
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
fn the_token_carries_what_an_ehdsi_assertion_provides_bound_to_the_certificate() -> TestResult {
    let connector = connector("http://127.0.0.1:9")?;
    let claims = claims_of(&connector.token()?)?;
    assert_eq!(claims["client_id"], CLIENT_ID);
    assert_eq!(claims["sub"], CLIENT_ID, "a client token");
    assert_eq!(claims["aud"], AUDIENCE);
    assert_eq!(claims["extensions"]["ihe_iua"]["person_id"], PERSON_ID);
    let ehdsi = &claims["ehdsi"];
    let professional = &ehdsi["professional"];
    assert_eq!(professional["hp_identifier"], HP_ID);
    assert_eq!(
        professional["hp_professional_role"][0]["system"],
        ROLE_SYSTEM
    );
    assert_eq!(professional["hp_professional_role"][0]["code"], ROLE);
    assert_eq!(professional["authentication_method"], AUTHENTICATION_CLASS);
    assert_eq!(ehdsi["provider"]["healthcare_provider_identifier"], HCP_ID);
    for (member, value) in [
        ("facility_type", &ehdsi["provider"]["facility_type"]),
        ("home_community_id", &ehdsi["provider"]["home_community_id"]),
        ("point_of_care", &ehdsi["point_of_care"]),
        ("assertion.id", &ehdsi["assertion"]["id"]),
        ("trc.id", &ehdsi["trc"]["id"]),
    ] {
        assert!(value.is_string(), "{member}");
    }
    assert_eq!(ehdsi["purpose_of_use"], "TREATMENT");
    assert_eq!(claims["cnf"], cnf(connector.certificate()), "RFC 8705 §3.1");
    Ok(())
}

#[test]
fn every_identifier_the_token_relays_is_in_the_example_arc() {
    for identifier in [HP_ID, HCP_ID, ROLE_SYSTEM] {
        assert!(identifier.starts_with("urn:oid:2.999."), "{identifier}");
    }
}

#[test]
fn the_declared_contact_point_configures_what_no_assertion_carries() -> TestResult {
    let table: toml::Table = toml::from_str(&contact_point_toml(
        "https://ncp.example.test",
        AUDIENCE,
        "/dev/null",
    ))?;
    let issuer = &table["auth"]["issuer"][0];
    assert_eq!(issuer["kind"].as_str(), Some("federation"));
    assert_eq!(issuer["sender_constraint"].as_str(), Some("required"));
    let federation = &table["auth"]["federation"][0];
    assert_eq!(federation["contact_point"].as_bool(), Some(true));
    assert_eq!(federation["mapping"].as_str(), Some("ehdsi"));
    for (field, value) in [
        ("professional.country_code", COUNTRY),
        ("professional.issuing_authority_name", HP_AUTHORITY),
        ("provider.issuing_authority_name", HCP_AUTHORITY),
    ] {
        assert_eq!(
            federation["fields"][field][0]["constant"].as_str(),
            Some(value),
            "{field}"
        );
    }
    let declared = &table["federation"]["contact_point"][0];
    assert_eq!(
        declared["correlation_header"].as_str(),
        Some(CORRELATION_HEADER)
    );
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
        assert_eq!(claims["ehdsi"]["professional"]["hp_identifier"], HP_ID);
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
