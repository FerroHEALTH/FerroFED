// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The stub PIX Manager answers ITI-83 with the identifiers of the domains a
//! request's `targetSystem` names alone, or of every domain when it names
//! none (ITI TF-2 §3.83.4.1.2.2), by `GET` and by `POST`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use ferrofed_testkit::pixm_stub;
use ferrofed_testkit::seed::{CrossReferenceSeed, EhrDomain, PatientId};

type TestResult = Result<(), Box<dyn Error>>;

const PATIENT: PatientId = PatientId::new(1, 83);
const DOMAINS: [EhrDomain; 2] = [EhrDomain::new(1), EhrDomain::new(2)];
const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const EHR_B: &str = "1111bbbb-1111-4111-8111-111111111111";

/// A stub that knows [`PATIENT`] in both domains.
async fn manager() -> Result<ferrofed_testkit::mock::Server, Box<dyn Error>> {
    Ok(pixm_stub::manager(&[CrossReferenceSeed {
        patient: PATIENT,
        ehrs: vec![(DOMAINS[0], EHR_A.parse()?), (DOMAINS[1], EHR_B.parse()?)],
    }])
    .await)
}

/// The answer to a `GET` naming `targets`.
async fn get(targets: &[EhrDomain]) -> Result<String, Box<dyn Error>> {
    let pix = manager().await?;
    let mut query = vec![(
        "sourceIdentifier",
        format!("{}|{}", PATIENT.namespace(), PATIENT.value()),
    )];
    query.extend(
        targets
            .iter()
            .map(|domain| ("targetSystem", domain.system())),
    );
    let url = reqwest::Url::parse_with_params(
        &format!("{}Patient/$ihe-pix", pixm_stub::base_url(&pix)),
        &query,
    )?;
    Ok(reqwest::get(url).await?.text().await?)
}

#[tokio::test]
async fn a_get_naming_one_domain_is_answered_with_that_domain_alone() -> TestResult {
    let answer = get(&[DOMAINS[0]]).await?;
    assert!(answer.contains(EHR_A), "{answer}");
    assert!(!answer.contains(EHR_B), "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_get_naming_no_domain_is_answered_with_every_domain() -> TestResult {
    let answer = get(&[]).await?;
    assert!(answer.contains(EHR_A) && answer.contains(EHR_B), "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_post_naming_one_domain_is_answered_with_that_domain_alone() -> TestResult {
    let pix = manager().await?;
    let body = format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"sourceIdentifier","valueIdentifier":{{"system":"{}","value":"{}"}}}},{{"name":"targetSystem","valueUri":"{}"}}]}}"#,
        PATIENT.namespace(),
        PATIENT.value(),
        DOMAINS[1].system()
    );
    let answer = reqwest::Client::new()
        .post(format!("{}Patient/$ihe-pix", pixm_stub::base_url(&pix)))
        .header("content-type", "application/fhir+json")
        .body(body)
        .send()
        .await?
        .text()
        .await?;
    assert!(answer.contains(EHR_B), "{answer}");
    assert!(!answer.contains(EHR_A), "{answer}");
    Ok(())
}
