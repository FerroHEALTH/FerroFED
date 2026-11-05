// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The routed requests of a national contact point (ITS-REST 1.1.0 EHR,
//! Definition and Demographic APIs) held to its TRC patient (eHDSI XCA
//! Profile 8.0.1 §4.2.1, §4.2.2): every operation under a path `ehr_id`
//! reaches the TRC patient's own pair and is no data for another patient's,
//! a read by subject is held as a query is, the creation of an EHR and a
//! DEMOGRAPHIC request are `trc-unbound`, and a definition request, which
//! reaches no patient data, needs no TRC patient.

use axum::body::Body;
use http::{Method, Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use openehr_its::rest::generated::ehr::ROUTES;

use super::{
    Bound, OTHER, OTHER_A, RESTRICTED, UNKNOWN, answered, as_treating, assert_no_data,
    assert_refused_as, gateway, outcome, plain, untreated,
};
use crate::auth::{TestResult, bearing, claims, minted};
use crate::facade::{EHR_A, NAMESPACE, PATIENT};
use crate::support::asked;

/// A version of a composition node A controls.
const VERSION: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// Its versioned object.
const VERSIONED: &str = "8849182c-82ad-4088-a07f-48ead4180515";

/// A synthetic `CONTRIBUTION` uid.
const CONTRIBUTION: &str = "9a9a9a9a-9a9a-49a9-89a9-9a9a9a9a9a9a";

/// An `ehr_id` no member holds, for a new EHR.
const NEW_EHR: &str = "3333cccc-3333-4333-8333-333333333333";

/// Every operation of the EHR API under a path `ehr_id` that reads or
/// changes an existing EHR, as `(method, path)` with every path parameter
/// filled and the `ehr_id` left as `{ehr_id}`.
fn under_an_ehr() -> Vec<(Method, String)> {
    ROUTES
        .iter()
        .filter(|(_, path, operation)| {
            path.contains("{ehr_id}") && *operation != "ehr_create_with_id"
        })
        .filter_map(|(method, path, operation)| {
            // NOTE: ITS-REST 1.1.0 composition_update names the versioned object, a UUID.
            let uid = if *operation == "composition_update" {
                VERSIONED
            } else {
                VERSION
            };
            let filled = path
                .replace("{version_uid}", VERSION)
                .replace("{uid_based_id}", uid)
                .replace("{versioned_object_uid}", VERSIONED)
                .replace("{contribution_uid}", CONTRIBUTION)
                .replace("{key}", "category");
            Some((method.parse().ok()?, filled))
        })
        .collect()
}

/// The request `method` sends to `path` under the EHR `ehr_id`, with the
/// headers and the body a write carries.
fn to_ehr(method: &Method, path: &str, ehr_id: &str) -> Result<Request<Body>, http::Error> {
    let uri = format!("/v1{}", path.replace("{ehr_id}", ehr_id));
    let mut request = Request::builder()
        .method(method.clone())
        .uri(uri)
        .header(header::ACCEPT, "application/json");
    if !method.is_safe() {
        request = request
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::IF_MATCH, format!("\"{VERSION}\""));
    }
    let body = if method.is_safe() {
        Body::empty()
    } else if path.ends_with("/contribution") {
        Body::from(CREATING)
    } else {
        Body::from("{}")
    };
    request.body(body)
}

/// A `CONTRIBUTION` creating one version, amending none (ITS-REST 1.1.0
/// `contribution_create`).
const CREATING: &str = r#"{"versions":[{"lifecycle_state":{"_type":"DV_CODED_TEXT","value":"complete","defining_code":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"openehr"},"code_string":"532"}},"commit_audit":{"_type":"UPDATE_AUDIT","change_type":{"_type":"DV_CODED_TEXT","value":"creation","defining_code":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"openehr"},"code_string":"249"}},"committer":{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}},"data":{"_type":"COMPOSITION","name":{"_type":"DV_TEXT","value":"Synthetic note"},"archetype_node_id":"openEHR-EHR-COMPOSITION.report.v1","language":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"ISO_639-1"},"code_string":"en"},"territory":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"ISO_3166-1"},"code_string":"NL"},"category":{"_type":"DV_CODED_TEXT","value":"event","defining_code":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"openehr"},"code_string":"433"}},"composer":{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}}}],"audit":{"_type":"UPDATE_AUDIT","change_type":{"_type":"DV_CODED_TEXT","value":"creation","defining_code":{"_type":"CODE_PHRASE","terminology_id":{"_type":"TERMINOLOGY_ID","value":"openehr"},"code_string":"249"}},"committer":{"_type":"PARTY_IDENTIFIED","name":"Synthetic clinician"}}}"#;

/// `GET {base}/v1/ehr` for `value`, naming node A in the targeting header.
fn by_subject(value: &str) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    let namespace = NAMESPACE.replace(':', "%3A");
    let mut request = Request::get(format!(
        "/v1/ehr?subject_id={value}&subject_namespace={namespace}"
    ))
    .header(header::ACCEPT, "application/json")
    .body(Body::empty())?;
    request
        .headers_mut()
        .insert(ENDPOINT, "node-a-pub".parse()?);
    Ok(request)
}

/// `request` naming node A in the targeting header.
fn at_a(mut request: Request<Body>) -> Result<Request<Body>, Box<dyn std::error::Error>> {
    request
        .headers_mut()
        .insert(ENDPOINT, "node-a-pub".parse()?);
    Ok(request)
}

/// Asserts that node A received `method` on `path` and node B nothing.
async fn assert_reached_a(bound: &Bound, method: &Method, path: &str) -> TestResult {
    let at_a = asked(&bound.a).await?;
    assert!(
        at_a.contains(&(method.to_string(), path.to_owned())),
        "{method} {path} reaches node A: {at_a:?}"
    );
    assert!(asked(&bound.b).await?.is_empty(), "node B received nothing");
    Ok(())
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.2: a resource of another patient is no data, never probed.
#[tokio::test]
async fn every_operation_on_another_patients_ehr_is_no_data_and_asks_nobody() -> TestResult {
    let operations = under_an_ehr();
    assert!(operations.len() >= 30, "the EHR API: {operations:?}");
    for (method, path) in operations {
        let bound = gateway().await?;
        let request = as_treating(to_ehr(&method, &path, OTHER_A)?, PATIENT)?;
        assert_no_data(&bound, request).await?;
    }
    Ok(())
}

#[tokio::test]
async fn every_operation_on_the_trc_patients_ehr_reaches_its_member() -> TestResult {
    for (method, path) in under_an_ehr() {
        let bound = gateway().await?;
        let request = as_treating(to_ehr(&method, &path, EHR_A)?, PATIENT)?;
        let (status, text) = answered(&bound, request).await?;
        assert_ne!(StatusCode::FORBIDDEN, status, "{method} {path}: {text}");
        let sent = format!("/v1{}", path.replace("{ehr_id}", EHR_A));
        let at_a = asked(&bound.a).await?;
        assert!(
            at_a.contains(&(method.to_string(), sent.clone())),
            "{method} {sent} reaches node A: {status} {text}"
        );
        assert!(asked(&bound.b).await?.is_empty(), "node B received nothing");
    }
    Ok(())
}

#[tokio::test]
async fn an_ehr_id_no_member_holds_is_no_data_and_is_never_probed() -> TestResult {
    let bound = gateway().await?;
    let request = to_ehr(&Method::GET, "/ehr/{ehr_id}", NEW_EHR)?;
    assert_no_data(&bound, as_treating(request, PATIENT)?).await
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: the subject named must equal the TRC's patient.
#[tokio::test]
async fn another_patients_read_by_subject_is_no_data_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_no_data(&bound, as_treating(by_subject(OTHER)?, PATIENT)?).await
}

#[tokio::test]
async fn the_trc_patients_read_by_subject_reaches_its_own_ehr() -> TestResult {
    let bound = gateway().await?;
    let (status, text) = answered(&bound, as_treating(by_subject(PATIENT)?, PATIENT)?).await?;
    assert_ne!(StatusCode::FORBIDDEN, status, "{text}");
    assert_reached_a(&bound, &Method::GET, &format!("/v1/ehr/{EHR_A}")).await
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.2 answers "No Data" so nothing tells the cases apart.
#[tokio::test]
async fn reads_by_subject_of_another_an_unknown_and_a_restricted_patient_answer_alike() -> TestResult
{
    let bound = gateway().await?;
    let other = outcome(&bound, as_treating(by_subject(OTHER)?, PATIENT)?).await?;
    let unknown = outcome(&bound, as_treating(by_subject(UNKNOWN)?, UNKNOWN)?).await?;
    let restricted = outcome(&bound, as_treating(by_subject(RESTRICTED)?, RESTRICTED)?).await?;
    assert_eq!(StatusCode::NOT_FOUND, other.0, "{}", other.1);
    assert_eq!(other, unknown, "a TRC patient no member holds");
    assert_eq!(other, restricted, "a TRC patient every holder restricts");
    bound.nobody_asked().await
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: a new EHR names no patient of the TRC to compare.
#[tokio::test]
async fn the_creation_of_an_ehr_is_trc_unbound_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    let posted = at_a(Request::post("/v1/ehr").body(Body::empty())?)?;
    assert_refused_as(&bound, as_treating(posted, PATIENT)?, "trc-unbound").await?;
    let put = at_a(Request::put(format!("/v1/ehr/{NEW_EHR}")).body(Body::empty())?)?;
    assert_refused_as(&bound, as_treating(put, PATIENT)?, "trc-unbound").await
}

#[tokio::test]
async fn the_creation_of_an_ehr_by_a_caller_that_is_no_contact_point_is_routed() -> TestResult {
    let bound = plain().await?;
    let posted = at_a(Request::post("/v1/ehr").body(Body::empty())?)?;
    let (status, text) = answered(&bound, bearing(posted, &minted(&claims())?)?).await?;
    assert_ne!(StatusCode::FORBIDDEN, status, "{text}");
    assert_reached_a(&bound, &Method::POST, "/v1/ehr").await
}

/// `GET {base}/v1/demographic/person/{uid}` naming node A.
fn person() -> Result<Request<Body>, Box<dyn std::error::Error>> {
    at_a(Request::get(format!("/v1/demographic/person/{VERSION}")).body(Body::empty())?)
}

// NOTE: eHDSI XCA Profile 8.0.1 §4.2.1: a party names no patient of the TRC to compare.
#[tokio::test]
async fn a_demographic_request_is_trc_unbound_and_asks_nobody() -> TestResult {
    let bound = gateway().await?;
    assert_refused_as(&bound, as_treating(person()?, PATIENT)?, "trc-unbound").await
}

#[tokio::test]
async fn a_demographic_request_of_a_caller_that_is_no_contact_point_is_routed() -> TestResult {
    let bound = plain().await?;
    let (status, text) = answered(&bound, bearing(person()?, &minted(&claims())?)?).await?;
    assert_ne!(StatusCode::FORBIDDEN, status, "{text}");
    let path = format!("/v1/demographic/person/{VERSION}");
    assert_reached_a(&bound, &Method::GET, &path).await
}

#[tokio::test]
async fn a_definition_request_needs_no_trc_patient() -> TestResult {
    let bound = gateway().await?;
    let listed = at_a(Request::get("/v1/definition/template/adl1.4").body(Body::empty())?)?;
    let (status, text) = answered(&bound, bearing(listed, &minted(&untreated())?)?).await?;
    assert_ne!(StatusCode::FORBIDDEN, status, "{text}");
    assert_reached_a(&bound, &Method::GET, "/v1/definition/template/adl1.4").await
}

// NOTE: eHDSI SAML Profile 10.0.0 §4.3 makes the TRC's subject-id mandatory.
#[tokio::test]
async fn a_routed_request_whose_token_names_no_trc_patient_is_trc_missing() -> TestResult {
    let bound = gateway().await?;
    let token = minted(&untreated())?;
    let read = to_ehr(&Method::GET, "/ehr/{ehr_id}", EHR_A)?;
    assert_refused_as(&bound, bearing(read, &token)?, "trc-missing").await?;
    assert_refused_as(
        &bound,
        bearing(by_subject(PATIENT)?, &token)?,
        "trc-missing",
    )
    .await
}
