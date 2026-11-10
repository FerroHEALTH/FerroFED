// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An issuer verified by RFC 7662 introspection (CP-17 inbound half): an
//! active answer for this gateway admits, an inactive or foreign one is a
//! `401`, and an endpoint that does not answer is a `503`, never a pass.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::auth::{AuthSettings, IssuerSettings, Verification};
use ferrofed_testkit::issuer::{ACT_REASON, METADATA_PATH};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::unreachable;
use ferrohealth_smart::assurance::AssuranceLevel;
use ferrohealth_smart::config::AuthSection;
use wiremock::matchers::{body_string_contains, header, method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{Gateway, TestResult, assert_admitted, assert_refused, bearing, query};
use crate::support::caller::AUDIENCE;

/// What an answer names the issuer by, replaced with the endpoint's URL as
/// the endpoint is mounted.
const ISSUER: &str = "urn:example:introspected-issuer";

/// An opaque token, which names no issuer.
const OPAQUE: &str = "synthetic-opaque-token-7Hq2";

/// The path the introspection endpoint answers at.
const INTROSPECT: &str = "/introspect";

/// `[auth]` introspecting the `persons` issuer at `endpoint`, whose
/// first-party client is `synthetic-client`, as the client `ferrofed` with
/// the secret `synthetic-secret`.
fn introspecting(endpoint: &str) -> Result<AuthSettings, Box<dyn Error>> {
    let section: AuthSection = toml::from_str(&format!(
        "[[issuer]]\nissuer = \"{endpoint}\"\nkind = \"persons\"\naudience = [\"{AUDIENCE}\"]\nfirst_party_clients = [\"synthetic-client\"]\nmode = \"introspection\"\n\n[issuer.introspection]\nendpoint = \"{endpoint}{INTROSPECT}\"\n\n[issuer.introspection.client]\nclient_id = \"ferrofed\"\nclient_secret = \"synthetic-secret\"\n\n[[attributes]]\nissuer = \"{endpoint}\"\nmapping = \"iua\"\n"
    ))?;
    let settings = section
        .resolve()?
        .introspection
        .into_iter()
        .next()
        .ok_or("the issuer is introspected")?;
    Ok(AuthSettings {
        issuers: vec![IssuerSettings::new(
            endpoint,
            Verification::Introspection(Box::new(settings)),
        )],
        ..AuthSettings::default()
    })
}

/// An RFC 7662 answer: `active`, for `audience`, expiring at `exp`.
fn answer(active: bool, audience: &str, exp: i64) -> String {
    format!(
        r#"{{"active":{active},"iss":"{ISSUER}","sub":"synthetic-caller","client_id":"synthetic-client","aud":["{audience}"],"exp":{exp},"scope":"user/aql-*.s","extensions":{{"ihe_iua":{{"purpose_of_use":[{{"system":"{ACT_REASON}","code":"TREAT"}}]}}}}}}"#
    )
}

/// An issuer whose metadata names it, answering `status` with `body` at its
/// introspection endpoint to the gateway's authenticated call about
/// [`OPAQUE`], the issuer `body` names being this one.
async fn endpoint(status: u16, body: String) -> Server {
    let server = Server::start().await;
    let metadata = serde_json::json!({
        "issuer": server.uri(),
        "token_endpoint": format!("{}/token", server.uri()),
        "introspection_endpoint": format!("{}{INTROSPECT}", server.uri()),
    });
    Mock::given(method("GET"))
        .and(path(METADATA_PATH))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(metadata.to_string(), "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(INTROSPECT))
        .and(header(
            "authorization",
            "Basic ZmVycm9mZWQ6c3ludGhldGljLXNlY3JldA==",
        ))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string_contains(format!("token={OPAQUE}")))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_raw(body.replace(ISSUER, &server.uri()), "application/json"),
        )
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
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Invalid).await
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
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Invalid).await
}

/// CP-17, inbound half: an answer whose `exp` has passed is a `401`.
// conformance: CP-17
#[tokio::test]
async fn an_answer_past_its_expiry_is_401() -> TestResult {
    let expired = jiff::Timestamp::now().as_second() - 3_600;
    let introspection = endpoint(200, answer(true, AUDIENCE, expired)).await;
    let gateway = Gateway::with(introspecting(&introspection.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Invalid).await
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

/// Regulation (EU) 2025/327 Annex II 3.1: the assurance claim the issuer's
/// entry names is read from the introspection answer as from a token, and a
/// level below the least one is refused.
#[tokio::test]
async fn an_answer_is_held_to_the_issuers_least_level() -> TestResult {
    let stating = |acr: &str| {
        answer(true, AUDIENCE, in_an_hour()).replacen(
            r#""active":true,"#,
            &format!(r#""active":true,"acr":"{acr}","#),
            1,
        )
    };
    let requiring = |endpoint: &str| -> Result<AuthSettings, Box<dyn Error>> {
        let mut auth = introspecting(endpoint)?;
        crate::support::caller::assuring(
            &mut auth,
            AssuranceLevel::High,
            &[
                ("urn:example:loa:substantial", AssuranceLevel::Substantial),
                ("urn:example:loa:high", AssuranceLevel::High),
            ],
        );
        Ok(auth)
    };
    let substantial = endpoint(200, stating("urn:example:loa:substantial")).await;
    let gateway = Gateway::with(requiring(&substantial.uri())?).await?;
    assert_refused(&gateway, bearing(query()?, OPAQUE)?, Refusal::Assurance).await?;
    let high = endpoint(200, stating("urn:example:loa:high")).await;
    let gateway = Gateway::with(requiring(&high.uri())?).await?;
    assert_admitted(&gateway, bearing(query()?, OPAQUE)?).await
}
