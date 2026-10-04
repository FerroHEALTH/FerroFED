// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The §9.4 directed query over two FerroEHR nodes: the ENDPOINT attributes
//! in every row, and no node request carrying the directive, a path through
//! its variable or the patient identifier (§8.1, §9.3, §9.4; N12, N17, N33;
//! CP-35, CP-37).

use ferrofed_testkit::containers;
use ferrofed_testkit::seed::{self, DemoComposition};
use http::StatusCode;
use serde::Deserialize;

use crate::e2e::{
    EHR_A, EHR_B, PATIENT, TestResult, assert_no_patient_identifier_on_the_wire, gateway, plan,
    query,
};
use crate::support::call;

/// The members of the answer the test reads.
#[derive(Debug, Deserialize)]
struct Answer {
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct Column {
    name: String,
}

// conformance: CP-35 CP-37 track-3
#[tokio::test]
async fn the_example_query_adds_each_nodes_attributes_over_two_cdr_nodes() -> TestResult {
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
    let aql = format!(
        r#"SELECT p/id AS endpoint_id, p/system_id AS system_id, c/uid/value AS composition_id FROM ENDPOINT p ["node-a-pub","node-b-pub"] CONTAINS EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '{}' AND e/ehr_status/subject/external_ref/namespace = '{}'"#,
        PATIENT.value(),
        PATIENT.namespace()
    );

    let (status, text) = call(gateway(dir.path(), &nodes.a, &nodes.b)?, query(&aql)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    let names: Vec<&str> = answer
        .columns
        .iter()
        .map(|column| column.name.as_str())
        .collect();
    assert_eq!(vec!["endpoint_id", "system_id", "composition_id"], names);
    let provenance: Vec<(&str, &str)> = answer
        .rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [endpoint, system, _uid] => Some((endpoint.as_str(), system.as_str())),
            _ => None,
        })
        .collect();
    let (a, b) = (
        nodes.a.node.system_id().to_string(),
        nodes.b.node.system_id().to_string(),
    );
    assert_eq!(
        vec![("node-a-pub", a.as_str()), ("node-b-pub", b.as_str())],
        provenance,
        "§9.4, N12: one row per node, each with its own endpoint id and system id: {text}"
    );
    for node in [&nodes.a, &nodes.b] {
        for absent in [
            "ENDPOINT",
            "p/id",
            "p/system_id",
            "node-a-pub",
            "node-b-pub",
        ] {
            assert!(
                !node.proxy.journal_contains(absent.as_bytes()),
                "{} received {absent:?} (§8.1)",
                node.node.system_id()
            );
        }
    }
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}
