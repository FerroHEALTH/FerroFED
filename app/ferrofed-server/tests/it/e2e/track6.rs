// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 6, follow-up read and write routing, over two FerroEHR nodes: a
//! follow-up read of a row reaches the CDR that holds it with the uid
//! unchanged, an `ehr_id` the gateway has not seen is found by a read-only
//! probe while a write to it is refused and never probed, and a new EHR is
//! created at the one node the client names (§12, §12a.1, §16.3 track 6;
//! N21, N22, N23, N41; CP-13, CP-15, CP-33).
//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track6`]); this suite adds
//! what each node was asked. A versioned write reaching only the CDR that
//! controls its version is scored in `e2e::commit`.

use ferrofed_server::conformance::scenarios::track6;
use ferrofed_testkit::containers::{self, NODE_B_SYSTEM_ID};
use uuid::Uuid;

use crate::e2e::scenario::{
    Options, asked, clear, fixture, gateway_with, in_process, nobody_asked, seed_both,
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
    let (gateway, fixture) = (in_process(&app), fixture(Some(1), Some(1))?);

    let uids = track6::row_uids(&gateway, &fixture).await?;
    assert_eq!(2, uids.len(), "one composition per node: {uids:?}");
    for uid in uids {
        clear(&nodes);
        let member = track6::follow_up_read(&gateway, &fixture, &uid).await?;
        let (owner, other, ehr_id) = if member.endpoint == "node-a-pub" {
            (&nodes.a, &nodes.b, EHR_A)
        } else {
            (&nodes.b, &nodes.a, EHR_B)
        };
        assert_eq!(
            vec![(
                "GET".to_owned(),
                format!("{}/v1/ehr/{ehr_id}/composition/{uid}", containers::API_PATH)
            )],
            asked(owner),
            "the CDR that holds the row is asked once"
        );
        assert!(asked(other).is_empty(), "the other node is never asked");
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
    track6::probe_read(
        &in_process(&app),
        "node-b-pub",
        NODE_B_SYSTEM_ID,
        &EHR_B.to_string(),
    )
    .await?;
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
        track6::unrouted_write(&in_process(&fresh), &ehr_id.to_string()).await?;
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
    let (gateway, fixture) = (in_process(&app), fixture(Some(1), Some(1))?);

    track6::new_ehr_untargeted(&gateway).await?;
    nobody_asked(&nodes, "CP-15: a new EHR is never probed for");

    let node_b = fixture.member("node-b-pub").ok_or("node B is a member")?;
    track6::new_ehr_at(&gateway, node_b).await?;
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
