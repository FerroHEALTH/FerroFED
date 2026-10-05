// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an operation requires of its caller (CP-17, inbound half; ITS-REST
//! SMART on openEHR, master07 §Context Selection, master08 §Resource Scopes):
//! a resource scope read with `openehr-sdt`, never a `patient/` grant the
//! gateway cannot bind, and no caller at all for an admin operation.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use axum::body::Body;
use ferrofed_server::auth::refusal::Refusal;
use http::{Method, Request, StatusCode};

use super::{
    Gateway, TestResult, assert_admitted, assert_refused, bearing, claims, minted, query, sent,
};
use crate::facade::EHR_A;
use crate::support::{self, error_body};

/// A token granting exactly `scope`.
fn granting(scope: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut granted = claims();
    granted.scope = Some(scope.to_owned());
    minted(&granted)
}

/// CP-17, inbound half: a query needs an `aql-` search scope; a composition
/// grant does not cover it.
// conformance: CP-17
#[tokio::test]
async fn a_query_without_an_aql_search_scope_is_403() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("user/composition-*.crud user/aql-*.r")?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: an ad hoc query is covered only by a pattern over
/// every query, never by a named one.
// conformance: CP-17
#[tokio::test]
async fn a_named_aql_pattern_does_not_cover_an_ad_hoc_query() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("user/aql-org.example::*.s")?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: a scope the grammar does not read as a resource
/// scope, and a token with no `scope` claim, grant nothing.
// conformance: CP-17
#[tokio::test]
async fn a_scope_outside_the_grammar_and_no_scope_grant_nothing() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("aql-*.s openid launch/patient system/Observation.read")?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Scope).await?;
    let mut none = claims();
    none.scope = None;
    assert_refused(
        &gateway,
        bearing(query()?, &minted(&none)?)?,
        Refusal::Scope,
    )
    .await
}

/// CP-17, inbound half: `system/aql-*` counts only for a backend client the
/// issuer's entry lists (master08 §Resource Scopes).
// conformance: CP-17
#[tokio::test]
async fn a_system_wide_aql_grant_counts_only_for_a_listed_backend_client() -> TestResult {
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer
            .backend_clients
            .insert(String::from("synthetic-backend"));
    }
    let gateway = Gateway::with(auth).await?;
    let token = granting("system/aql-*.s")?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Scope).await?;
    let mut backend = claims();
    backend.scope = Some(String::from("system/aql-*.s"));
    backend.client_id = String::from("synthetic-backend");
    assert_admitted(&gateway, bearing(query()?, &minted(&backend)?)?).await
}

/// CP-17, inbound half: a `patient/` grant is confined to its launch
/// context, which names no subject the gateway resolves, so it admits no
/// query, whichever patient the query names, its own or another.
// conformance: CP-17
#[tokio::test]
async fn a_patient_grant_admits_no_query() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let now = jiff::Timestamp::now().as_second();
    let payload = format!(
        r#"{{"iss":"{}","sub":"synthetic-patient-user","aud":"{}","exp":{},"iat":{now},"jti":"synthetic-jti","client_id":"synthetic-client","scope":"patient/aql-*.s launch/patient","ehrId":"{EHR_A}","extensions":{{"ihe_iua":{{"purpose_of_use":[{{"system":"{}","code":"TREAT"}}]}}}}}}"#,
        support::ISSUER,
        support::AUDIENCE,
        now + 600,
        ferrofed_testkit::issuer::ACT_REASON,
    );
    let token = support::issuer().sign(&support::issuer().header(), &payload)?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: a `patient/` grant admits no request addressed by
/// `ehr_id` either.
// conformance: CP-17
#[tokio::test]
async fn a_patient_grant_admits_no_ehr_id_route() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("patient/composition-*.cruds patient/aql-*.cruds")?;
    let request = Request::get(format!("/v1/ehr/{EHR_A}/ehr_status")).body(Body::empty())?;
    assert_refused(&gateway, bearing(request, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: a routed composition read needs a composition read
/// grant over every template, because only the node knows the template.
// conformance: CP-17
#[tokio::test]
async fn a_composition_read_needs_a_composition_read_over_every_template() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let read = || {
        Request::get(format!(
            "/v1/ehr/{EHR_A}/composition/8849182c-82ad-4088-a07f-48ead4180515::node-a::1"
        ))
        .body(Body::empty())
    };
    for scope in [
        "user/aql-*.s",
        "user/composition-*.c",
        "user/composition-org.example::*.r",
    ] {
        assert_refused(
            &gateway,
            bearing(read()?, &granting(scope)?)?,
            Refusal::Scope,
        )
        .await?;
    }
    Ok(())
}

/// CP-17, inbound half: an EHR resource that is no COMPOSITION is held to
/// the composition family with the operation's permission.
// conformance: CP-17
#[tokio::test]
async fn an_ehr_status_update_needs_a_composition_update() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("user/composition-*.r")?;
    let request = Request::put(format!("/v1/ehr/{EHR_A}/ehr_status")).body(Body::empty())?;
    assert_refused(&gateway, bearing(request, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: a template named in the path is covered by a
/// pattern that matches it, and by no other.
// conformance: CP-17
#[tokio::test]
async fn a_named_template_is_covered_by_its_own_pattern() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("user/template-org.example::*.r")?;
    let read =
        |id: &str| Request::get(format!("/v1/definition/template/adl1.4/{id}")).body(Body::empty());
    let (status, _, text) =
        sent(&gateway.app, bearing(read("org.example::vitals")?, &token)?).await?;
    assert_eq!(StatusCode::BAD_REQUEST, status, "past the gate: {text}");
    assert_eq!("target-required", error_body(&text)?.code, "{text}");
    assert_refused(
        &gateway,
        bearing(read("org.other::vitals")?, &token)?,
        Refusal::Scope,
    )
    .await
}

/// CP-17, inbound half: a stored query needs a scope whose pattern covers
/// its name.
// conformance: CP-17
#[tokio::test]
async fn a_stored_query_needs_a_pattern_over_its_name() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let token = granting("user/aql-org.other::*.s")?;
    let request = Request::get("/v1/query/org.example::vitals").body(Body::empty())?;
    assert_refused(&gateway, bearing(request, &token)?, Refusal::Scope).await
}

/// CP-17, inbound half: an admin operation is refused to every caller,
/// before any credential is read.
// conformance: CP-17
#[tokio::test]
async fn an_admin_operation_is_refused_to_every_caller() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let delete = || Request::delete(format!("/v1/admin/ehr/{EHR_A}")).body(Body::empty());
    assert_refused(&gateway, delete()?, Refusal::Operation).await?;
    let token = minted(&claims())?;
    assert_refused(&gateway, bearing(delete()?, &token)?, Refusal::Operation).await
}

/// CP-17, inbound half: the DEMOGRAPHIC API, which no SMART on openEHR
/// family covers, admits only a client the issuer's entry lists.
// conformance: CP-17
#[tokio::test]
async fn the_demographic_api_admits_only_a_listed_client() -> TestResult {
    let gateway = Gateway::trusting_the_test_issuer().await?;
    let read = || {
        Request::builder()
            .method(Method::GET)
            .uri("/v1/demographic/person/8849182c-82ad-4088-a07f-48ead4180515::node-a::1")
            .body(Body::empty())
    };
    let mut stranger = claims();
    stranger.client_id = String::from("synthetic-unlisted-client");
    assert_refused(
        &gateway,
        bearing(read()?, &minted(&stranger)?)?,
        Refusal::Demographic,
    )
    .await?;
    let (status, _, text) = sent(&gateway.app, bearing(read()?, &minted(&claims())?)?).await?;
    assert_eq!(
        StatusCode::NOT_IMPLEMENTED,
        status,
        "a listed client passes the gate to an undeclared area: {text}"
    );
    Ok(())
}
