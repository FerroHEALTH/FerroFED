// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The caller's verified access token is kept only where a node's grant
//! exchanges it (RFC 8693 §2.1), only for a caller the gateway verified by
//! signature or introspection, and never in the caller's `Debug`.

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_server::auth::Gate;
use ferrofed_server::auth::permission::Requirement;
use ferrofed_server::config::auth::{
    AuthMode, AuthSettings, IssuerSettings, KeySource, Verification,
};
use ferrofed_testkit::issuer::{Claims, Issuer};
use http::{HeaderMap, HeaderName, header};
use secrecy::ExposeSecret;

use super::TestResult;
use crate::support::{self, AUDIENCE};

/// The headers of a request bearing `token`.
fn bearing(token: &str) -> Result<HeaderMap, Box<dyn Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(header::AUTHORIZATION, format!("Bearer {token}").parse()?);
    Ok(headers)
}

/// A gateway with no node that exchanges keeps no token of the caller.
// conformance: CP-17
#[tokio::test]
async fn no_token_is_kept_where_no_node_exchanges_it() -> TestResult {
    let gate = Gate::new(&support::auth());
    let token = support::token()?;
    let caller = gate
        .admit(&bearing(&token)?, (Requirement::Caller, None), false)
        .await
        .map_err(|refusal| format!("{refusal:?}"))?;
    assert!(caller.token().is_none(), "the token is dropped");
    Ok(())
}

/// Where a node exchanges it, the token the gateway verified is kept, and
/// the caller's `Debug` shows nothing of it.
// conformance: CP-17
#[tokio::test]
async fn the_verified_token_is_kept_for_an_exchanging_node_and_never_shown() -> TestResult {
    let gate = Gate::new(&support::auth());
    let token = support::token()?;
    let caller = gate
        .admit(&bearing(&token)?, (Requirement::Caller, None), true)
        .await
        .map_err(|refusal| format!("{refusal:?}"))?;
    let kept = caller.token().ok_or("the token is kept")?;
    assert_eq!(token, kept.secret().expose_secret());
    let shown = format!("{caller:?}");
    assert!(!shown.contains(&token), "Debug shows the token: {shown}");
    for segment in token.split('.') {
        assert!(!shown.contains(segment), "Debug shows part of the token");
    }
    Ok(())
}

/// The edge's signed assertion is never kept as a caller's token, even
/// where a node exchanges one.
// conformance: CP-17
#[tokio::test]
async fn an_edge_assertion_is_never_kept_as_a_token() -> TestResult {
    let edge = Issuer::new("https://edge.example.test")?;
    let gate = Gate::new(&AuthSettings {
        mode: AuthMode::Edge(HeaderName::from_static("ferrofed-edge-assertion")),
        audience: Some(AUDIENCE.to_owned()),
        issuers: vec![IssuerSettings {
            issuer: "https://edge.example.test".to_owned(),
            verification: Verification::KeySet(KeySource::Set(edge.jwks())),
            backend_clients: BTreeSet::new(),
            demographic_clients: BTreeSet::new(),
        }],
        ..AuthSettings::default()
    });
    let assertion = edge.mint(&Claims::new("https://edge.example.test", AUDIENCE))?;
    let mut headers = HeaderMap::new();
    headers.insert("ferrofed-edge-assertion", assertion.parse()?);
    let caller = gate
        .admit(&headers, (Requirement::Caller, None), true)
        .await
        .map_err(|refusal| format!("{refusal:?}"))?;
    assert!(caller.token().is_none(), "an edge assertion is no token");
    Ok(())
}
