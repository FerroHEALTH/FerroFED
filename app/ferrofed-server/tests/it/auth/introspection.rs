// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An issuer verified by RFC 7662 introspection (CP-17 inbound half): an
//! active answer for this gateway admits, an inactive or foreign one is a
//! `401`, and an endpoint that does not answer is a `503`, never a pass.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_registry::secret::Secret;
use ferrofed_server::auth::Refusal;
use ferrofed_server::config::auth::{AuthSettings, Introspection, IssuerSettings, Verification};
use ferrofed_testkit::issuer::ACT_REASON;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::unreachable;
use url::Url;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{Gateway, TestResult, assert_admitted, assert_refused, bearing, query};
use crate::support::AUDIENCE;

/// The issuer whose tokens are introspected.
const ISSUER: &str = "https://introspected-issuer.example.test";

/// An opaque token, which names no issuer.
const OPAQUE: &str = "synthetic-opaque-token-7Hq2";

/// The path the introspection endpoint answers at.
const INTROSPECT: &str = "/introspect";

/// `[auth]` introspecting at `endpoint` as the client `ferrofed` with the
/// secret `synthetic-secret`.
fn introspecting(endpoint: &str) -> Result<AuthSettings, Box<dyn Error>> {
    Ok(AuthSettings {
        audience: Some(AUDIENCE.to_owned()),
        issuers: vec![IssuerSettings {
            issuer: ISSUER.to_owned(),
            verification: Verification::Introspection(Introspection {
                endpoint: Url::parse(&format!("{endpoint}{INTROSPECT}"))?,
                client_id: String::from("ferrofed"),
                client_secret: Secret::new("synthetic-secret"),
            }),
            backend_clients: BTreeSet::new(),
            demographic_clients: BTreeSet::new(),
            patient: None,
        }],
        ..AuthSettings::default()
    })
}

/// An RFC 7662 answer: `active`, for `audience`, expiring at `exp`.
fn answer(active: bool, audience: &str, exp: i64) -> String {
    format!(
        r#"{{"active":{active},"iss":"{ISSUER}","sub":"synthetic-caller","client_id":"synthetic-client","aud":["{audience}"],"exp":{exp},"scope":"user/aql-*.s","extensions":{{"ihe_iua":{{"purpose_of_use":[{{"system":"{ACT_REASON}","code":"TREAT"}}]}}}}}}"#
    )
}

/// An introspection endpoint answering `status` with `body` to the
/// gateway's authenticated call about [`OPAQUE`].
async fn endpoint(status: u16, body: String) -> Server {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path(INTROSPECT))
        .and(header(
            "authorization",
            "Basic ZmVycm9mZWQ6c3ludGhldGljLXNlY3JldA==",
        ))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains(format!("token={OPAQUE}")))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, "application/json"))
        .mount(&server)
        .await;
    server
}

/// The time an hour from now.
fn in_an_hour() -> i64 {
    jiff::Timestamp::now().as_second() + 3_600
}

/// CP-17, inbound half: an opaque token the endpoint calls active for this
/// gateway is admitted, the gateway authenticating to the endpoint.
// conformance: CP-17
#[tokio::test]
async fn an_active_token_for_this_gateway_is_admitted() -> TestResult {
    let introspection = endpoint(200, answer(true, AUDIENCE, in_an_hour())).await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_admitted(&gateway, bearing(query()?, OPAQUE)?).await
}

/// CP-17, inbound half: a token the endpoint calls inactive is a `401`.
// conformance: CP-17
#[tokio::test]
async fn an_inactive_token_is_401() -> TestResult {
    let introspection = endpoint(200, String::from(r#"{"active":false}"#)).await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Inactive).await
}

/// CP-17, inbound half: an active token for another audience is a `401`.
// conformance: CP-17
#[tokio::test]
async fn an_active_token_for_another_audience_is_401() -> TestResult {
    let introspection = endpoint(
        200,
        answer(true, "urn:example:another-resource-server", in_an_hour()),
    )
    .await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Audience).await
}

/// CP-17, inbound half: an answer whose `exp` has passed is a `401`.
// conformance: CP-17
#[tokio::test]
async fn an_answer_past_its_expiry_is_401() -> TestResult {
    let expired = jiff::Timestamp::now().as_second() - 3_600;
    let introspection = endpoint(200, answer(true, AUDIENCE, expired)).await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Expired).await
}

/// CP-17, inbound half: an endpoint answering with an error is a `503`,
/// never a pass.
// conformance: CP-17
#[tokio::test]
async fn an_endpoint_answering_an_error_is_503() -> TestResult {
    let introspection = endpoint(500, String::from("{}")).await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Unavailable).await
}

/// CP-17, inbound half: an endpoint that cannot be reached is a `503`.
// conformance: CP-17
#[tokio::test]
async fn an_unreachable_endpoint_is_503() -> TestResult {
    let gateway = Gateway::with(introspecting(unreachable::BASE)?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Unavailable).await
}
