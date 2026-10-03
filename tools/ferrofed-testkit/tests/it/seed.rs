// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The synthetic seed builder: what it writes, how, and that nothing it
//! writes is a real identifier.

use ferrofed_testkit::mock::Server;
use ferrofed_testkit::seed::{
    self, CompositionSeed, DemoComposition, EXAMPLE_ARC, EhrSeed, PatientId, SeedError, SeedPlan,
    TEMPLATE_ID,
};
use http::StatusCode;
use openehr_its::json::{from_canonical_json, to_canonical_json};
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const EHR: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);

/// Returns whether `digits` is nine digits that pass the Dutch citizen
/// service number's eleven test, the shape a real national identifier has.
fn passes_eleven_test(digits: &[u8]) -> bool {
    if digits.len() != 9 || !digits.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let weights = [9_i64, 8, 7, 6, 5, 4, 3, 2, -1];
    let sum: i64 = digits
        .iter()
        .zip(weights)
        .map(|(digit, weight)| i64::from(digit - b'0') * weight)
        .sum();
    sum != 0 && sum % 11 == 0
}

/// Returns every maximal run of ASCII digits in `bytes`.
fn digit_runs(bytes: &[u8]) -> Vec<&[u8]> {
    bytes
        .split(|byte| !byte.is_ascii_digit())
        .filter(|run| !run.is_empty())
        .collect()
}

#[test]
fn the_eleven_test_recognises_the_shape_it_guards() {
    assert!(
        passes_eleven_test(b"111222333"),
        "a number that passes the eleven test is recognised"
    );
    assert!(
        !passes_eleven_test(b"111222334"),
        "a number that fails it is not"
    );
    assert!(!passes_eleven_test(b"12345"), "a short number is not");
}

#[test]
fn every_patient_identifier_is_in_the_example_arc() {
    for domain in [0, 1, 2, u16::MAX] {
        for number in [0, 1, 42, u16::MAX] {
            let patient = PatientId::new(domain, number);
            let namespace = patient.namespace();
            assert!(
                namespace.starts_with(&format!("{EXAMPLE_ARC}.")),
                "{namespace} is inside the example arc"
            );
            assert!(
                namespace.starts_with("urn:oid:2.999."),
                "{namespace} is under 2.999"
            );
            let value = patient.value();
            assert!(
                value.starts_with("ffd-test-"),
                "{value} carries the synthetic prefix"
            );
            assert!(
                digit_runs(value.as_bytes())
                    .into_iter()
                    .all(|run| !passes_eleven_test(run)),
                "{value} has no run of digits a national scheme validates"
            );
        }
    }
}

#[test]
fn the_ehr_status_names_the_subject_in_the_example_arc() {
    let patient = PatientId::new(1, 1);
    let body = to_canonical_json(&seed::ehr_status(Some(patient)));
    assert!(
        body.contains("\"_type\":\"EHR_STATUS\""),
        "the body is a canonical EHR_STATUS: {body}"
    );
    assert!(
        body.contains("\"_type\":\"PARTY_SELF\""),
        "the subject is PARTY_SELF: {body}"
    );
    assert!(
        body.contains("\"namespace\":\"urn:oid:2.999.1.1\""),
        "the namespace is the arc: {body}"
    );
    assert!(
        body.contains("\"value\":\"ffd-test-0001\""),
        "the value is synthetic: {body}"
    );

    let anonymous = to_canonical_json(&seed::ehr_status(None));
    assert!(
        !anonymous.contains("external_ref"),
        "an anonymous subject carries no reference: {anonymous}"
    );
}

#[test]
fn the_ehr_status_reads_back_through_the_strict_canonical_reader() {
    for subject in [Some(PatientId::new(1, 1)), None] {
        let status = seed::ehr_status(subject);
        let read: EhrStatus = from_canonical_json(&to_canonical_json(&status)).unwrap();
        assert_eq!(read, status, "the EHR_STATUS for {subject:?} round-trips");
    }
}

#[test]
fn the_vendored_demo_data_carries_no_patient_identifier() {
    for composition in DemoComposition::ALL {
        let bytes = std::fs::read(composition.path()).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        // The composer is a PARTY_IDENTIFIED with a fictional name and no
        // identifier list; an identifier would need one of these carriers.
        for carrier in ["external_ref", "DV_IDENTIFIER", "\"identifiers\""] {
            assert!(
                !text.contains(carrier),
                "{} carries no {carrier}",
                composition.path().display()
            );
        }
        assert!(
            text.contains("\"_type\": \"PARTY_SELF\""),
            "{} names only the record's own subject",
            composition.path().display()
        );
    }
    assert!(
        seed::template_path().exists(),
        "the vendored template exists"
    );
}

/// A stub node that accepts every seed step, answering the composition with
/// an `ETag`.
async fn accepting_node() -> Server {
    let server = Server::start().await;
    Mock::given(method("PUT"))
        .and(path(format!("/rest/openehr/v1/ehr/{EHR}")))
        .and(header("content-type", "application/json"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/rest/openehr/v1/definition/template/adl1.4"))
        .and(header("content-type", "application/xml"))
        .and(header("accept", "application/xml"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("/rest/openehr/v1/ehr/{EHR}/composition")))
        .respond_with(ResponseTemplate::new(201).insert_header("ETag", "\"8849182c::node-a::1\""))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_seed_writes_over_its_rest_alone_in_plan_order() {
    let node = accepting_node().await;
    let plan = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR,
            subject: Some(PatientId::new(1, 1)),
        }],
        template: true,
        compositions: vec![CompositionSeed {
            ehr_id: EHR,
            composition: DemoComposition::FirstClinic,
        }],
    };

    let report = seed::seed(&format!("{}/rest/openehr", node.uri()), &plan)
        .await
        .unwrap();
    assert_eq!(report.ehrs, vec![EHR], "the EHR is reported");
    assert_eq!(report.compositions.len(), 1, "the composition is reported");
    assert_eq!(
        report.compositions[0].version_uid.as_deref(),
        Some("8849182c::node-a::1"),
        "the version uid comes from the ETag without its quotes"
    );

    let received = node.received_requests().await.unwrap();
    let steps: Vec<(String, String)> = received
        .iter()
        .map(|request| (request.method.to_string(), request.url.path().to_owned()))
        .collect();
    assert_eq!(
        steps,
        vec![
            ("PUT".to_owned(), format!("/rest/openehr/v1/ehr/{EHR}")),
            (
                "POST".to_owned(),
                "/rest/openehr/v1/definition/template/adl1.4".to_owned()
            ),
            (
                "POST".to_owned(),
                format!("/rest/openehr/v1/ehr/{EHR}/composition")
            ),
        ],
        "the steps are the three ITS-REST calls, in plan order"
    );
    assert_eq!(
        received[0].body,
        to_canonical_json(&seed::ehr_status(Some(PatientId::new(1, 1)))).into_bytes(),
        "the EHR is created with the canonical JSON of its EHR_STATUS"
    );
    assert_eq!(
        received[1].body,
        std::fs::read(seed::template_path()).unwrap(),
        "the template {TEMPLATE_ID} is uploaded byte for byte"
    );
    assert_eq!(
        received[2].body,
        std::fs::read(DemoComposition::FirstClinic.path()).unwrap(),
        "the composition is committed byte for byte"
    );
    for request in &received {
        assert!(
            digit_runs(&request.body)
                .into_iter()
                .chain(digit_runs(request.url.as_str().as_bytes()))
                .all(|run| !passes_eleven_test(run))
                || request.url.path().ends_with("/composition")
                || request.url.path().ends_with("/adl1.4"),
            "the EHR request carries no number a national scheme validates"
        );
    }
}

#[tokio::test]
async fn a_refused_step_is_an_error_naming_the_step_and_the_status() {
    let node = Server::start().await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&node)
        .await;
    let plan = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR,
            subject: None,
        }],
        ..SeedPlan::default()
    };

    let error = seed::seed(&format!("{}/rest/openehr", node.uri()), &plan)
        .await
        .unwrap_err();
    match error {
        SeedError::Refused { step, status, .. } => {
            assert_eq!(
                step,
                format!("PUT /v1/ehr/{EHR}"),
                "the refused step is named"
            );
            assert_eq!(status, StatusCode::CONFLICT, "the node's status is carried");
        }
        other => panic!("a 409 is a refusal, not {other:?}"),
    }
}
