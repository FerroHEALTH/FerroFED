// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The edge mode (CP-17 inbound half): a proxy authenticates the caller and
//! signs an assertion, which the gateway verifies as it verifies a token and
//! records; a bearer token alone admits no one.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use axum::body::Body;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::auth::{
    AuthMode, AuthSettings, IssuerSettings, KeySource, Verification,
};
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::issuer::{Claims, Issuer};
use http::{HeaderName, Request};

use super::{Gateway, TestResult, assert_admitted, assert_refused, bearing, query};
use crate::support::{self, AUDIENCE, Logs};

/// The header the edge's assertion travels in.
const HEADER: &str = "ferrofed-edge-assertion";

/// The edge's issuer identifier.
const EDGE: &str = "https://edge.example.test";

/// `[auth]` in the edge mode, trusting `edge`.
fn at_the_edge(edge: &Issuer) -> AuthSettings {
    AuthSettings {
        mode: AuthMode::Edge(HeaderName::from_static(HEADER)),
        audience: Some(AUDIENCE.to_owned()),
        issuers: vec![IssuerSettings {
            issuer: EDGE.to_owned(),
            verification: Verification::KeySet(KeySource::Set(edge.jwks())),
            backend_clients: BTreeSet::new(),
            demographic_clients: BTreeSet::new(),
            operator_scope: None,
            patient: None,
            requester: None,
            assurance: None,
            client_tokens_act_for_professional: false,
        }],
        ..AuthSettings::default()
    }
}

/// The query carrying `assertion` in the edge's header.
fn asserted(assertion: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = query()?;
    request.headers_mut().insert(HEADER, assertion.parse()?);
    Ok(request)
}

/// CP-17, inbound half: the edge's assertion admits the caller, and the
/// gateway records the identity the edge asserted.
// conformance: CP-17
#[tokio::test]
async fn an_assertion_of_the_edge_admits_and_is_recorded() -> TestResult {
    let edge = Issuer::new(EDGE)?;
    let gateway = Gateway::with(at_the_edge(&edge)).await?;
    let mut claims = Claims::new(EDGE, AUDIENCE);
    claims.sub = String::from("synthetic-clinician-at-the-edge");
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    assert_admitted(&gateway, asserted(&edge.mint(&claims)?)?).await?;
    drop(guard);
    let text = logs.text();
    assert!(
        text.lines()
            .any(|line| line.contains("edge-identity-asserted")
                && line.contains(EDGE)
                && line.contains("\"subject_ref\":\"")
                && line.contains("\"client_ref\":\"")),
        "the asserted identity is recorded by its references: {text}"
    );
    for value in [claims.sub.as_str(), claims.client_id.as_str()] {
        assert!(!text.contains(value), "the log names no caller: {text}");
    }
    Ok(())
}

/// One caller's edge events carry one reference, and another caller's
/// another, so the security log correlates them without naming either.
#[tokio::test]
async fn the_edge_event_references_are_stable_per_caller() -> TestResult {
    let edge = Issuer::new(EDGE)?;
    let gateway = Gateway::with(at_the_edge(&edge)).await?;
    let mut first = Claims::new(EDGE, AUDIENCE);
    first.sub = String::from("synthetic-clinician-one");
    let mut second = Claims::new(EDGE, AUDIENCE);
    second.sub = String::from("synthetic-clinician-two");
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "info", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    for claims in [&first, &first, &second] {
        assert_admitted(&gateway, asserted(&edge.mint(claims)?)?).await?;
    }
    drop(guard);
    let text = logs.text();
    let references: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("edge-identity-asserted"))
        .filter_map(|line| line.split("\"subject_ref\":\"").nth(1))
        .filter_map(|rest| rest.split('"').next())
        .collect();
    assert_eq!(3, references.len(), "{text}");
    assert_eq!(references[0], references[1], "one caller, one reference");
    assert_ne!(references[0], references[2], "another caller, another");
    assert!(!text.contains("synthetic-clinician"), "{text}");
    Ok(())
}

/// CP-17, inbound half: in the edge mode a bearer token alone is a `401`,
/// even one a trusted issuer signed.
// conformance: CP-17
#[tokio::test]
async fn a_bearer_token_alone_is_401_at_the_edge() -> TestResult {
    let edge = Issuer::new(EDGE)?;
    let gateway = Gateway::with(at_the_edge(&edge)).await?;
    let token = edge.mint(&Claims::new(EDGE, AUDIENCE))?;
    assert_refused(&gateway, bearing(query()?, &token)?, Refusal::Missing).await
}

/// CP-17, inbound half: an assertion another issuer signed is a `401`.
// conformance: CP-17
#[tokio::test]
async fn an_assertion_of_another_issuer_is_401() -> TestResult {
    let edge = Issuer::new(EDGE)?;
    let gateway = Gateway::with(at_the_edge(&edge)).await?;
    let token = support::token()?;
    assert_refused(&gateway, asserted(&token)?, Refusal::Issuer).await
}
