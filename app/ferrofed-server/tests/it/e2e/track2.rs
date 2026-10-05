// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 2, `subject` to `ehr_id` resolution, over two FerroEHR nodes with
//! the patient resolved by the harness PIX Manager over ITI-83: each node is
//! asked by its own `ehr_id` and never receives `subject`, judged on the
//! node-side AQL, and both patient carriers return the same rows (§16.3
//! track 2; N3, N5, N7, N33; CP-2, CP-3, CP-4, CP-7, CP-38).
//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track2`]); this suite adds the
//! node-side AQL. The member where the patient does not resolve is never
//! asked (N8, CP-36), in `e2e::pixm`.

use ferrofed_server::conformance::scenarios::track2;
use ferrofed_testkit::containers;

use crate::e2e::pixm::{DOMAIN_A, DOMAIN_B, nodes_and_pix, pixm_resolver};
use crate::e2e::scenario::{Options, fixture, gateway_with, in_process, queries, seed_both};
use crate::e2e::{EHR_A, EHR_B, TestResult, assert_no_patient_identifier_on_the_wire};

// conformance: CP-2 CP-3 CP-4 CP-38 track-2
#[tokio::test]
async fn the_pix_manager_resolves_both_carriers_to_each_nodes_own_ehr_id() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)])).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        resolver: pixm_resolver(&pix),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;

    track2::both_carriers(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    assert!(pix.queries() >= 1, "CP-3: resolved through ITI-83");

    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let sent = queries(node);
        assert_eq!(2, sent.len(), "one node query per carrier");
        for capture in &sent {
            let body = String::from_utf8_lossy(&capture.body);
            assert!(
                body.contains(&own.to_string()) && !body.contains(&other.to_string()),
                "CP-4: keyed on the node's own ehr_id alone: {body}"
            );
            assert!(
                !body.contains("subject"),
                "CP-2: the node AQL carries no subject: {body}"
            );
        }
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}

// conformance: CP-7 track-2
#[tokio::test]
async fn a_selected_subject_column_is_the_input_and_never_asked_of_a_node() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    track2::subject_column(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    for node in [&nodes.a, &nodes.b] {
        for capture in queries(node) {
            let body = String::from_utf8_lossy(&capture.body);
            assert!(
                !body.contains("subject"),
                "CP-7: the subject is never read from a CDR: {body}"
            );
        }
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}
