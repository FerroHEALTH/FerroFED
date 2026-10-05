// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The federated query over two FerroEHR nodes, the patient resolved through
//! the development cross-reference: one `RESULT_SET`, each node asked by its
//! own `ehr_id`, and both patient carriers answering the same rows (§5.4.3,
//! §7, §9, §11; N7, N16, N33).
//!
//! The client-visible checks are the conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track1`] and
//! [`track2`](ferrofed_server::conformance::scenarios::track2)); this suite
//! adds what only the nodes' capturing proxies show.

use ferrofed_server::conformance::scenarios::{track1, track2};
use ferrofed_testkit::containers::{self, API_PATH};
use ferrofed_testkit::seed::{self, DemoComposition};
use http::StatusCode;

use crate::e2e::scenario::{fixture, in_process};
use crate::e2e::{
    Answer, EHR_A, EHR_B, TestResult, assert_no_patient_identifier_on_the_wire, body_holds,
    gateway, plan, query,
};
use crate::support::call;

// conformance: CP-1 CP-2 CP-4 CP-35 track-1 track-2
#[tokio::test]
async fn one_result_set_over_two_cdr_nodes_and_no_identifier_on_the_wire() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;

    let app = gateway(dir.path(), &nodes.a, &nodes.b)?;
    let answer = track1::single_cdr_shaped(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    assert!(answer.meta.federation.complete, "both nodes answered");
    let reported: Vec<(&str, &str, Option<u64>)> = answer
        .meta
        .federation
        .endpoints
        .iter()
        .map(|e| (e.id.as_str(), e.status.as_str(), e.row_count))
        .collect();
    assert_eq!(
        vec![
            ("node-a-pub", "active", Some(1)),
            ("node-b-pub", "active", Some(1)),
        ],
        reported,
        "one composition from each node (N16)"
    );
    assert_eq!(2, answer.rows.len(), "rows from both nodes: {answer:?}");

    let aql = format!("{API_PATH}/v1/query/aql");
    for (node, own, other) in [(&nodes.a, EHR_A, EHR_B), (&nodes.b, EHR_B, EHR_A)] {
        let journal = node.proxy.journal();
        let steps: Vec<(&str, &str)> = journal
            .iter()
            .map(|capture| (capture.method.as_str(), capture.path.as_str()))
            .collect();
        assert_eq!(
            vec![("POST", aql.as_str()); 2],
            steps,
            "{} received one ITS-REST query per client query, by POST and by GET, and nothing else",
            node.node.system_id()
        );
        for sent in &journal {
            let body = String::from_utf8_lossy(&sent.body);
            assert!(
                body_holds(sent, &own.to_string()),
                "the node query is keyed on the node's own ehr_id (N7): {body}"
            );
            assert!(
                !body_holds(sent, &other.to_string()),
                "a node never learns another node's ehr_id: {body}"
            );
        }
    }
    assert_no_patient_identifier_on_the_wire(&nodes);

    // The README quickstart query: no patient, every member asked (N4).
    let (status, text) = call(
        gateway(dir.path(), &nodes.a, &nodes.b)?,
        query("SELECT e/ehr_id/value FROM EHR e")?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    let mut ehr_ids: Vec<String> = answer.rows.into_iter().flatten().collect();
    ehr_ids.sort();
    let mut expected = vec![EHR_A.to_string(), EHR_B.to_string()];
    expected.sort();
    assert_eq!(
        expected, ehr_ids,
        "the quickstart query returns the EHR of each member"
    );
    Ok(())
}

// conformance: CP-38 track-2
#[tokio::test]
async fn both_patient_carriers_resolve_to_the_same_rows_over_two_cdr_nodes() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    let dir = tempfile::tempdir()?;

    // §5.4.3, CP-38: the same patient query once per carrier, the rows equal
    // and not empty, every holding member active.
    let app = gateway(dir.path(), &nodes.a, &nodes.b)?;
    let answers = track2::both_carriers(&in_process(&app), &fixture(Some(1), Some(1))?).await?;
    assert_eq!(2, answers.len(), "one answer per carrier");
    for answer in &answers {
        assert!(answer.meta.federation.complete, "both nodes answered");
    }

    for node in [&nodes.a, &nodes.b] {
        let journal = node.proxy.journal();
        let bodies: Vec<&[u8]> = journal
            .iter()
            .map(|capture| capture.body.as_slice())
            .collect();
        assert_eq!(
            2,
            bodies.len(),
            "{} received one query per carrier",
            node.node.system_id()
        );
        assert_eq!(
            bodies.first(),
            bodies.get(1),
            "{} received the same node query from both carriers (§7.1)",
            node.node.system_id()
        );
        assert!(
            !node.proxy.journal_contains(b"subject/identifiers"),
            "{} received the ENTRY carrier (N33)",
            node.node.system_id()
        );
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}
