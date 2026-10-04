// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 6, follow-up read and write routing, over two FerroEHR nodes: a
//! follow-up read of a row reaches the CDR that holds it with the uid
//! unchanged, an `ehr_id` the gateway has not seen is found by a read-only
//! probe while a write to it is refused and never probed, and a new EHR is
//! created at the one node the client names (§12, §12a.1, §16.3 track 6;
//! N21, N22, N23, N41; CP-13, CP-15, CP-33).
//!
//! A versioned write reaching only the CDR that controls its version is
//! scored in `e2e::commit`.

use axum::body::Body;
use ferrofed_testkit::containers::{self, NODE_A_SYSTEM_ID, NODE_B_SYSTEM_ID};
use http::{Request, StatusCode};
use openehr_federation::headers::{ENDPOINT, SYSTEM_ID};
use uuid::Uuid;

use crate::e2e::scenario::{
    Options, asked, clear, exchange, gateway_with, nobody_asked, patient_compositions, post_aql,
    seed_both,
};
use crate::e2e::{EHR_A, EHR_B, TestResult};

/// An `ehr_id` no member holds.
const NOWHERE: Uuid = Uuid::from_u128(0x9999_9999_9999_4999_8999_9999_9999_9999);

// conformance: CP-13 CP-33 track-6
#[tokio::test]
async fn a_follow_up_read_of_a_row_reaches_the_cdr_that_holds_it_with_its_uid_unchanged()
-> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    let reply = exchange(&app, post_aql(&patient_compositions(), &[])?).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    let uids: Vec<String> = reply.federated()?.rows.into_iter().flatten().collect();
    assert_eq!(2, uids.len(), "one composition per node: {uids:?}");

    for uid in uids {
        let (owner, node, ehr_id, endpoint) = if uid.contains(&format!("::{NODE_A_SYSTEM_ID}::")) {
            (&nodes.a, &nodes.b, EHR_A, "node-a-pub")
        } else {
            (&nodes.b, &nodes.a, EHR_B, "node-b-pub")
        };
        clear(&nodes);
        let read =
            Request::get(format!("/v1/ehr/{ehr_id}/composition/{uid}")).body(Body::empty())?;
        let followed = exchange(&app, read).await?;
        assert_eq!(StatusCode::OK, followed.status, "CP-33: {}", followed.text);
        assert_eq!(
            Some(endpoint),
            followed.field(ENDPOINT),
            "CP-13, CP-33: routed by the ehr_id the resolution bound to the node"
        );
        let etag = followed.field("etag").ok_or("the node's ETag")?;
        assert!(
            etag.contains(&uid),
            "the uid comes back as the node wrote it: {etag}"
        );
        assert_eq!(
            vec![(
                "GET".to_owned(),
                format!("{}/v1/ehr/{ehr_id}/composition/{uid}", containers::API_PATH)
            )],
            asked(owner),
            "the CDR that holds the row is asked once"
        );
        assert!(asked(node).is_empty(), "the other node is never asked");
    }
    Ok(())
}

// conformance: CP-33 track-6
#[tokio::test]
async fn an_unseen_ehr_id_is_probed_read_only_and_a_write_to_it_is_never_probed() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;

    let probing = tempfile::tempdir()?;
    let app = gateway_with(probing.path(), &nodes, &Options::default())?;
    let read = Request::get(format!("/v1/ehr/{EHR_B}")).body(Body::empty())?;
    let found = exchange(&app, read).await?;
    assert_eq!(StatusCode::OK, found.status, "CP-33: {}", found.text);
    assert_eq!(Some("node-b-pub"), found.field(ENDPOINT));
    assert_eq!(Some(NODE_B_SYSTEM_ID), found.field(SYSTEM_ID));
    for node in [&nodes.a, &nodes.b] {
        assert!(
            asked(node).iter().all(|(verb, _)| verb == "GET"),
            "CP-33: the probe is read-only"
        );
    }

    let writing = tempfile::tempdir()?;
    let fresh = gateway_with(writing.path(), &nodes, &Options::default())?;
    for ehr_id in [EHR_A, NOWHERE] {
        clear(&nodes);
        let write = Request::post(format!("/v1/ehr/{ehr_id}/composition"))
            .header(http::header::CONTENT_TYPE, "application/json")
            .body(Body::from("{}"))?;
        let refused = exchange(&fresh, write).await?;
        assert_eq!(
            StatusCode::BAD_REQUEST,
            refused.status,
            "CP-33: a write no target, binding or index routes: {}",
            refused.text
        );
        nobody_asked(&nodes, "CP-33: a write is never ask-all-probed");
    }
    Ok(())
}

// conformance: CP-15 track-6
#[tokio::test]
async fn a_new_ehr_is_created_at_the_one_node_the_client_names() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    let untargeted = exchange(&app, Request::post("/v1/ehr").body(Body::empty())?).await?;
    assert_eq!(
        StatusCode::BAD_REQUEST,
        untargeted.status,
        "CP-15: a new object names no node: {}",
        untargeted.text
    );
    assert_eq!("target-required", untargeted.code()?);
    nobody_asked(&nodes, "CP-15: a new EHR is never probed for");

    let named = Request::post("/v1/ehr")
        .header(ENDPOINT, "node-b-pub")
        .body(Body::empty())?;
    let created = exchange(&app, named).await?;
    assert_eq!(
        StatusCode::CREATED,
        created.status,
        "CP-15: {}",
        created.text
    );
    assert_eq!(Some("node-b-pub"), created.field(ENDPOINT), "N31");
    assert!(
        created
            .field("location")
            .is_some_and(|location| location.contains(containers::API_PATH)),
        "N31: node B's own Location"
    );
    assert!(asked(&nodes.a).is_empty(), "CP-15: node A is never asked");
    assert_eq!(
        vec![(
            "POST".to_owned(),
            format!("{}/v1/ehr", containers::API_PATH)
        )],
        asked(&nodes.b),
        "CP-15: the one chosen node creates it"
    );
    Ok(())
}
