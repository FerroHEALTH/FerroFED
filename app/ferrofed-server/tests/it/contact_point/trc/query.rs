// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query of a national contact point, in each form the
//! ITS-REST Query API takes it (ITS-REST 1.1.0, `query_execute_adhoc_query`,
//! `query_execute_adhoc_query_body`, `query_execute_stored_query`,
//! `query_execute_stored_query_body`), held to its TRC patient (eHDSI XCA
//! Profile 8.0.1 §4.2.1): the TRC patient's own query reaches that
//! patient's pairs, and another patient's is no data with nothing asked.

use axum::body::Body;
use http::{Request, StatusCode};
use openehr_federation::headers::ENDPOINT;

use super::{
    Bound, NAME, OTHER, OTHER_A, RESTRICTED, UNKNOWN, answered, as_treating, assert_no_data,
    assert_refused_as, gateway, outcome, plain, untreated,
};
use crate::auth::{TestResult, bearing, claims, minted};
use crate::facade::{Answer, EHR_A, NAMESPACE, PATIENT, PATIENT_TAIL, body, post, statuses, wire};
use crate::query_get::encoded;

/// The federated query for `patient` in the suite's namespace.
fn aql_for(patient: &str) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{patient}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    )
}

/// `POST {base}/v1/query/aql` for `patient`.
fn posted(patient: &str) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    Ok(post(body(&aql_for(patient))?)?)
}

/// `GET {base}/v1/query/aql` for `patient`.
fn got(patient: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/query/aql?q={}", encoded(&aql_for(patient)))).body(Body::empty())
}

/// `POST {base}/v1/query/{name}` binding `$patient` to `patient`.
fn stored_posted(patient: &str) -> Result<Request<Body>, http::Error> {
    Request::post(format!("/v1/query/{NAME}"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(format!(
            r#"{{"query_parameters":{{"patient":"{patient}"}}}}"#
        )))
}

/// `GET {base}/v1/query/{name}` binding `$patient` to `patient`.
fn stored_got(patient: &str) -> Result<Request<Body>, http::Error> {
    Request::get(format!("/v1/query/{NAME}?patient={patient}")).body(Body::empty())
}

/// The query scoped to `ehr_id` (N29).
fn scoped_to(ehr_id: &str) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{ehr_id}'"
    );
    Ok(post(body(&aql)?)?)
}

/// Sends the TRC patient's query `request` to `bound` and asserts it is
/// answered from both members, with no identifier on the wire (N33).
async fn assert_served_at_both(bound: &Bound, request: Request<Body>) -> TestResult {
    let (status, text) = answered(bound, as_treating(request, PATIENT)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "{text}"
    );
    for node in [&bound.a, &bound.b] {
        let captured = wire(node).await?;
        assert!(!captured.is_empty(), "each member was asked");
        assert!(!captured.contains(PATIENT_TAIL), "N33: {captured}");
    }
    Ok(())
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: the patient of the request must equal the TRC's.
#[tokio::test]
async fn another_patients_query_by_post_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(posted(OTHER)?, PATIENT)?).await
}

#[tokio::test]
async fn the_trc_patients_query_by_post_reaches_its_own_pairs() -> TestResult {
    let bound = gateway().await?;
    assert_served_at_both(&bound, posted(PATIENT)?).await
}

#[tokio::test]
async fn another_patients_query_by_get_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(got(OTHER)?, PATIENT)?).await
}

#[tokio::test]
async fn the_trc_patients_query_by_get_reaches_its_own_pairs() -> TestResult {
    let bound = gateway().await?;
    assert_served_at_both(&bound, got(PATIENT)?).await
}

#[tokio::test]
async fn another_patients_stored_query_by_post_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(stored_posted(OTHER)?, PATIENT)?).await
}

#[tokio::test]
async fn the_trc_patients_stored_query_by_post_reaches_its_own_pairs() -> TestResult {
    let bound = gateway().await?;
    assert_served_at_both(&bound, stored_posted(PATIENT)?).await
}

#[tokio::test]
async fn another_patients_stored_query_by_get_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(stored_got(OTHER)?, PATIENT)?).await
}

#[tokio::test]
async fn the_trc_patients_stored_query_by_get_reaches_its_own_pairs() -> TestResult {
    let bound = gateway().await?;
    assert_served_at_both(&bound, stored_got(PATIENT)?).await
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.2: an EHR of another patient is no data, never probed for.
#[tokio::test]
async fn a_query_scoped_to_another_patients_ehr_id_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(scoped_to(OTHER_A)?, PATIENT)?).await?;
    let mut targeted = scoped_to(OTHER_A)?;
    targeted
        .headers_mut()
        .insert(ENDPOINT, "node-a-pub".parse()?);
    assert_no_data(&bound, as_treating(targeted, PATIENT)?).await
}

#[tokio::test]
async fn a_query_scoped_to_the_trc_patients_ehr_id_goes_to_its_member_alone() -> TestResult {
    let bound = gateway().await?;
    let (status, text) = answered(&bound, as_treating(scoped_to(EHR_A)?, PATIENT)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "excluded")],
        statuses(&answer),
        "N29: {text}"
    );
    assert!(wire(&bound.b).await?.is_empty(), "node B is not asked");
    Ok(())
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: a query that names no patient has none to compare.
#[tokio::test]
async fn a_population_query_is_trc_unbound_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    let population = post(body(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    )?)?;
    assert_refused_as(&bound, as_treating(population, PATIENT)?, "trc-unbound").await
}

#[tokio::test]
async fn a_population_query_of_a_caller_that_is_no_contact_point_is_served() -> TestResult {
    let bound = plain().await?;
    let population = post(body(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    )?)?;
    let (status, text) = answered(&bound, bearing(population, &minted(&claims())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(!wire(&bound.a).await?.is_empty(), "node A is asked");
    Ok(())
}

// NOTE: eHDSI SAML Profile 10.0.0 §4.3 makes the TRC's subject-id mandatory.
#[tokio::test]
async fn a_query_whose_token_names_no_trc_patient_is_trc_missing_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    for request in [posted(PATIENT)?, got(PATIENT)?, stored_posted(PATIENT)?] {
        let request = bearing(request, &minted(&untreated())?)?;
        assert_refused_as(&bound, request, "trc-missing").await?;
    }
    Ok(())
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.2 answers "No Data" so nothing tells the cases apart.
#[tokio::test]
async fn another_an_unknown_and_a_restricted_patient_answer_alike() -> TestResult {
    let bound = gateway().await?;
    let other = outcome(&bound, as_treating(posted(OTHER)?, PATIENT)?).await?;
    let unknown = outcome(&bound, as_treating(posted(UNKNOWN)?, UNKNOWN)?).await?;
    let restricted = outcome(&bound, as_treating(posted(RESTRICTED)?, RESTRICTED)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, other.0, "{}", other.1);
    assert_eq!(other, unknown, "a TRC patient no member holds");
    assert_eq!(other, restricted, "a TRC patient every holder restricts");
    bound.nobody_asked().await
}
