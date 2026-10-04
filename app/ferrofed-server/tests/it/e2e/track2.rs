// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 2, `subject` to `ehr_id` resolution, over two FerroEHR nodes with
//! the patient resolved by the harness PIX Manager over ITI-83: each node is
//! asked by its own `ehr_id` and never receives `subject`, judged on the
//! node-side AQL, and both patient carriers return the same rows (§16.3
//! track 2; N3, N5, N7, N33; CP-2, CP-3, CP-4, CP-7, CP-38).
//!
//! The member where the patient does not resolve is never asked (N8,
//! CP-36), in `e2e::pixm`.

use ferrofed_testkit::containers;
use http::StatusCode;

use crate::e2e::pixm::{DOMAIN_A, DOMAIN_B, nodes_and_pix, pixm_resolver};
use crate::e2e::scenario::{
    Options, exchange, gateway_with, patient_predicate, post_aql, queries, seed_both,
};
use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult, assert_no_patient_identifier_on_the_wire};

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

    let from = "FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
    let (namespace, value) = (PATIENT.namespace(), PATIENT.value());
    let via_external_ref = format!("SELECT c/uid/value {from} WHERE {}", patient_predicate());
    let via_entry = format!(
        "SELECT c/uid/value {from} \
         WHERE o/subject/identifiers/id = '{value}' \
         AND o/subject/identifiers/issuer = '{namespace}'"
    );
    let mut answered = Vec::new();
    for aql in [&via_external_ref, &via_entry] {
        let reply = exchange(&app, post_aql(aql, &[])?).await?;
        assert_eq!(StatusCode::OK, reply.status, "CP-38: {}", reply.text);
        let answer = reply.federated()?;
        assert_eq!(
            vec![("node-a-pub", "active"), ("node-b-pub", "active")],
            answer.statuses(),
            "CP-3: the subject resolved at both members"
        );
        answered.push(answer.sorted_rows());
    }
    assert!(
        answered.first().is_some_and(|rows| !rows.is_empty()),
        "the demo compositions hold observations, so the comparison is not vacuous"
    );
    assert_eq!(
        answered.first(),
        answered.get(1),
        "CP-38: both carriers return the same rows"
    );
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
    let aql = format!(
        "SELECT e/ehr_status/subject/external_ref/id/value AS patient, c/uid/value AS uid \
         FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    );

    let reply = exchange(&app, post_aql(&aql, &[])?).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    let answer = reply.federated()?;
    assert_eq!(vec!["patient", "uid"], answer.names());
    assert_eq!(2, answer.rows.len(), "{}", reply.text);
    let value = PATIENT.value();
    for row in &answer.rows {
        assert_eq!(
            Some(&value),
            row.first(),
            "CP-7: the subject column is the re-injected input"
        );
    }
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
