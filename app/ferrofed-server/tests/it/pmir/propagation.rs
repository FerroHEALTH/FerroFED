// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 8 of §16.3 end to end: a merge the identity source publishes over
//! ITI-93 drops the resolution bindings of the retired `ehr_id`, and the next
//! query of the patient resolves to the surviving one; a split, which ITI-93
//! carries only as an update (PMIR §2:3.93.4.1.3.1), drops every binding, so
//! nothing routes by an identity it may have split.
//!
//! In process: node A, a PIXm Manager stub whose cross-reference changes as
//! the merge does, and the gateway's ITI-93 route.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::path::Path;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::pixm_stub;
use ferrofed_testkit::pmir::{merge_message, update_message};
use ferrofed_testkit::seed::{CrossReferenceSeed, EhrDomain, PatientId};
use http::{Request, StatusCode, header};
use uuid::Uuid;

use super::{FEED_CLIENT, Gateway, PATH, feed_credentials, gateway};
use crate::facade::{body, node_answering, post, received, registry};
use crate::support::{call, send_as_is};

type TestResult = Result<(), Box<dyn Error>>;

/// Node A's `ehr_id` domain at the PIX Manager; node B's holds nothing here.
const DOMAIN_A: EhrDomain = EhrDomain::new(1);
const DOMAIN_B: EhrDomain = EhrDomain::new(2);

/// The `ehr_id` the merge retires.
const RETIRED: Uuid = Uuid::from_u128(0x8a8a_8a8a_8a8a_4a8a_8a8a_8a8a_8a8a_8a8a);

/// The `ehr_id` of the surviving identity.
const SURVIVING: Uuid = Uuid::from_u128(0x8b8b_8b8b_8b8b_4b8b_8b8b_8b8b_8b8b_8b8b);

/// The patient the queries name, in the example arc.
fn patient() -> PatientId {
    PatientId::new(1, 48)
}

/// The cross-reference of the patient at node A with `ehr_id`.
fn known(ehr_id: Uuid) -> [CrossReferenceSeed; 1] {
    [CrossReferenceSeed {
        patient: patient(),
        ehrs: vec![(DOMAIN_A, ehr_id)],
    }]
}

/// A development gateway over node A at `a` and node B at `b`, resolving
/// through the PIX Manager at `pix`, with the ITI-93 route of `[pmir]`.
fn propagating(dir: &Path, a: &str, b: &str, pix: &str) -> Result<Gateway, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry(a, b, ""))?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout = '2s'\noverall_timeout = '3s'\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[[pixm.manager]]\nendpoint.url = \"{pix}\"\n\n[pixm.manager.members]\n\"node-a\" = \"{}\"\n\"node-b\" = \"{}\"\n{}\n[pmir]\nendpoint.url = \"http://127.0.0.1:9/fhir/\"\ncallback_url = \"http://127.0.0.1:9/pmir/feed\"\npath = \"{PATH}\"\nfeed_clients = [\"{FEED_CLIENT}\"]\n",
        DOMAIN_A.system(),
        DOMAIN_B.system(),
        crate::support::caller::auth_toml()
    );
    gateway(&text)
}

/// The patient query, through `external_ref`.
fn query() -> Result<Request<Body>, Box<dyn Error>> {
    let patient = patient();
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{}' \
         AND e/ehr_status/subject/external_ref/namespace = '{}'",
        patient.value(),
        patient.namespace()
    );
    Ok(post(body(&aql)?)?)
}

/// Delivers the ITI-93 message `body` with the feed client's credential and
/// returns its status.
async fn deliver(gateway: &Gateway, body: String) -> Result<StatusCode, Box<dyn Error>> {
    let request = Request::post(PATH)
        .header(header::CONTENT_TYPE, "application/fhir+json")
        .header(header::AUTHORIZATION, feed_credentials()?)
        .body(Body::from(body))?;
    Ok(send_as_is(gateway.app.clone(), request).await?.status())
}

/// The last query node A received.
async fn last_query(node: &Server) -> Result<String, Box<dyn Error>> {
    received(node)
        .await?
        .pop()
        .ok_or_else(|| "node A received no query".into())
}

// conformance: track-8
#[tokio::test]
async fn after_a_merge_the_next_query_resolves_to_the_surviving_ehr_id() -> TestResult {
    let pix = pixm_stub::manager(&known(RETIRED)).await;
    let a = node_answering("a-uid::cdr-a.example.org::1").await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let gateway = propagating(dir.path(), &a.uri(), &b.uri(), &pixm_stub::base_url(&pix))?;

    let (status, text) = call(gateway.app.clone(), query()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(last_query(&a).await?.contains(&RETIRED.to_string()));
    assert_eq!(1, gateway.bound()?, "the query bound the patient's ehr_id");

    let retired = RETIRED.to_string();
    let system = DOMAIN_A.system();
    let merge = merge_message("patient-old", &[(&system, &retired)], "patient-new")?;
    assert_eq!(StatusCode::OK, deliver(&gateway, merge).await?);
    assert_eq!(0, gateway.bound()?, "the merge dropped the retired binding");

    pix.reset().await;
    pixm_stub::mount(&pix, &known(SURVIVING)).await;
    let (status, text) = call(gateway.app.clone(), query()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let sent = last_query(&a).await?;
    assert!(
        sent.contains(&SURVIVING.to_string()) && !sent.contains(&retired),
        "track 8: the next query is scoped to the surviving ehr_id alone: {sent}"
    );
    assert_eq!(1, gateway.bound()?, "only the surviving ehr_id is bound");
    assert!(b.received_requests().await.ok_or("recording")?.is_empty());
    Ok(())
}

// conformance: track-8
#[tokio::test]
async fn a_split_sent_as_an_update_drops_every_binding() -> TestResult {
    let pix = pixm_stub::manager(&known(RETIRED)).await;
    let a = node_answering("a-uid::cdr-a.example.org::1").await;
    let b = Server::start().await;
    let dir = tempfile::tempdir()?;
    let gateway = propagating(dir.path(), &a.uri(), &b.uri(), &pixm_stub::base_url(&pix))?;
    let (status, text) = call(gateway.app.clone(), query()?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    gateway.bind("another-caller", &[&SURVIVING.to_string()])?;
    assert_eq!(2, gateway.bound()?);

    let retired = RETIRED.to_string();
    let system = DOMAIN_A.system();
    let unmerged = update_message("patient-old", &[(&system, &retired)])?;
    assert_eq!(StatusCode::OK, deliver(&gateway, unmerged).await?);
    assert_eq!(
        0,
        gateway.bound()?,
        "PMIR §2:3.93.4.1.2.3: an update cannot say what it split off, so every binding goes"
    );
    Ok(())
}
