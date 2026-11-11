// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A person's federated query reaches a FerroEHR node with authentication
//! on under the token its authorization server exchanged the caller's token
//! for, and the same query fails at that node under the gateway's own
//! client-credentials token (§13.1, N25; RFC 8693 §2.1).
//!
//! The node is FerroEHR 5.7.0 or later, which binds every person who signs
//! in to a demographic `PERSON` and creates a provisional one at a first
//! request ([`containers::bound::persons`]). It trusts the suite's test
//! issuer as its `persons` issuer and lists the gateway's client among its
//! delegates, so the exchanged token, naming the caller as `sub` and the
//! gateway as its client, is the one credential it admits for the caller.
//! The token endpoint is a `wiremock` stand-in that answers the exchange
//! with a token the test issuer signed.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use axum::Router;
use ferrofed_server::config::Config;
use ferrofed_server::state::AppState;
use ferrofed_testkit::containers::bound::{self, PersonsIssuer};
use ferrofed_testkit::containers::{self, NODE_A_SYSTEM_ID, ProxiedNode};
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::seed::{self, DemoComposition};
use http::{StatusCode, header};

use crate::e2e::scenario::captured_field;
use crate::e2e::{Answer, EHR_A, PATIENT, TestResult, patient_query, plan, query};
use crate::facade::settings_with_room;
use crate::onward_lane::{CLIENT_ID, NODE, OWN_TOKEN, authorization_server, exchanges, lane};
use crate::support::caller::{ISSUER, bearer_as, issuer};
use crate::support::{call, signed};

/// The caller, a professional of the node's organisation.
const CALLER: &str = "synthetic-clinician-cdr";

/// The node's own client the harness seeds the patient's record through.
const SEED_CLIENT: &str = "synthetic-seed-client";

/// The identity provider whose persons the node creates at a first request.
const IDP: &str = "synthetic-idp";

/// A token of the test issuer for the node, naming `subject` through
/// `client`, with the name and identity provider the node's just-in-time
/// creation reads.
fn node_token(
    subject: &str,
    client: &str,
    scope: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut claims = Claims::new(ISSUER, NODE);
    subject.clone_into(&mut claims.sub);
    client.clone_into(&mut claims.client_id);
    claims.scope = Some(scope.to_owned());
    claims.extensions = None;
    claims
        .other
        .insert(String::from("name"), format!("Synthetic {subject}"));
    claims.other.insert(String::from("idp"), IDP.to_owned());
    Ok(issuer().mint(&claims)?)
}

/// The FerroEHR node trusting the test issuer, holding the patient's EHR at
/// [`EHR_A`] with one composition, behind its capturing proxy.
async fn seeded_node() -> Result<ProxiedNode, Box<dyn std::error::Error>> {
    let jwks = issuer().jwks_json()?;
    let node = bound::persons(
        NODE_A_SYSTEM_ID,
        &PersonsIssuer {
            issuer: ISSUER,
            audience: NODE,
            jwks: &jwks,
            first_party_clients: &[SEED_CLIENT],
            delegates: &[CLIENT_ID],
            idp: IDP,
        },
    )
    .await?;
    let seeder = node_token(
        "synthetic-seeder",
        SEED_CLIENT,
        ferrofed_testkit::issuer::EVERY_SCOPE,
    )?;
    let mut headers = http::HeaderMap::new();
    headers.insert(header::AUTHORIZATION, format!("Bearer {seeder}").parse()?);
    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;
    seed::seed_with(
        &client,
        &node.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    Ok(ProxiedNode::new(node).await?)
}

/// The gateway over the one node at `node`, resolving [`PATIENT`] to
/// [`EHR_A`] there, its node credentials `tables`.
fn gateway(
    dir: &Path,
    node: &ProxiedNode,
    tables: &str,
) -> Result<Router, Box<dyn std::error::Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        format!(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"{NODE_A_SYSTEM_ID}\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
            node.api_root()
        ),
    )?;
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {}\n\n[federation]\nper_node_timeout = '20s'\noverall_timeout = '25s'\nnode_selection = \"ask-all\"\nid = \"example-federation\"\nbest_effort = false\n\n[[testkit.crossref]]\nnamespace = \"{}\"\nvalue = \"{}\"\nmember = \"node-a\"\nehr_id = \"{EHR_A}\"\n\n{tables}",
        toml::Value::String(document.display().to_string()),
        PATIENT.namespace(),
        PATIENT.value(),
    );
    let settings = Config::from_sources(Some(&signed(&text)), &BTreeMap::new())?.resolve()?;
    let mut server = settings_with_room();
    server.request_timeout = std::time::Duration::from_secs(30);
    server.auth = crate::support::caller::auth();
    Ok(ferrofed_server::router(
        Arc::new(AppState::build(&settings)?),
        &server,
    ))
}

/// The bearer credentials the node received, in arrival order.
fn bearers(node: &ProxiedNode) -> Vec<String> {
    node.proxy
        .journal()
        .iter()
        .filter_map(|capture| captured_field(capture, header::AUTHORIZATION.as_str()))
        .map(str::to_owned)
        .collect()
}

/// The patient query, sent as [`CALLER`].
fn as_the_caller() -> Result<http::Request<axum::body::Body>, Box<dyn std::error::Error>> {
    let mut request = query(&patient_query())?;
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, bearer_as(CALLER)?.parse()?);
    Ok(request)
}

/// With an onward lane, the caller's token is exchanged for the node's own
/// and the node answers the caller's query; without one, the node refuses
/// the gateway's own token and the query fails, so the caller's exchanged
/// token is what the node admits (§13.1, N25).
// conformance: CP-17
#[tokio::test]
async fn a_persons_query_reaches_the_cdr_under_the_exchanged_token_alone() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let node = seeded_node().await?;
    let exchanged = node_token(CALLER, CLIENT_ID, "user/aql-*.s")?;
    let server = authorization_server(&exchanged, 200).await;
    let tables = lane(&server.uri(), "persons", "");
    let dir = tempfile::tempdir()?;

    let start = tables
        .find("[credentials.\"node-a-pub\".credentials.onward]")
        .ok_or("the onward table")?;
    let oauth_alone = tables.get(..start).ok_or("the oauth table")?;
    let without = gateway(dir.path(), &node, oauth_alone)?;
    let (status, text) = call(without, as_the_caller()?).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "the node refuses the gateway's own token for the caller: {text}"
    );
    assert_eq!(
        vec![format!("Bearer {OWN_TOKEN}")],
        bearers(&node),
        "the node was sent the gateway's own token alone"
    );
    node.proxy.clear_journal();

    let with = gateway(dir.path(), &node, &tables)?;
    let (status, text) = call(with, as_the_caller()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        1,
        answer.rows.len(),
        "the node answers the caller's query with the seeded composition: {text}"
    );
    assert!(answer.meta.federation.complete, "{text}");
    assert_eq!(1, exchanges(&server).await?.len(), "one exchange");
    let sent = bearers(&node);
    assert!(!sent.is_empty(), "the node was asked");
    assert!(
        sent.iter()
            .all(|bearer| *bearer == format!("Bearer {exchanged}")),
        "every request reached the node under the exchanged token: {sent:?}"
    );
    Ok(())
}
