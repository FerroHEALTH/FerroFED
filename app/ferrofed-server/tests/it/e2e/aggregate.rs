// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An undirected `AVG` over two FerroEHR nodes that hold different numbers of
//! values, behind the `FERROFED_E2E` gate (§11.6.3, N14, N39; CP-10, CP-32,
//! track 5).
//!
//! §11.6.3 lets a gateway decompose `AVG` across nodes only when it "also
//! retrieves the per-node counts". The gateway declares `AVG` decomposable in
//! `OPTIONS {base}/`; this checks that the declaration holds on real nodes:
//! each node is asked its `SUM` and `COUNT`, the answer is the mean weighted
//! by those counts, and the counts are the ones each node returns for the
//! query it was sent. The self-description is read with the conformance
//! run's own check
//! ([`ferrofed_server::conformance::scenarios::track9::options`]).

use std::error::Error;

use ferrofed_server::conformance::scenarios::track9;
use ferrofed_testkit::containers::{self, ProxiedNode};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, CompositionSeed, DemoComposition, EhrSeed, SeedPlan};
use http::StatusCode;
use serde::Deserialize;

use crate::e2e::scenario::{
    Options, clear, exchange, gateway_with, in_process, patient_predicate, post_aql, queries,
};
use crate::e2e::{EHR_A, EHR_B, PATIENT, TestResult, plan};

/// The systolic pressure of the blood pressure observation in the vendored
/// demo compositions.
const SYSTOLIC: &str = "o/data[at0001]/events[at0006]/data[at0003]/items[at0004]/value/magnitude";

/// The façade query: the patient's mean systolic pressure across members.
fn mean_systolic() -> String {
    format!(
        "SELECT AVG({SYSTOLIC}) AS mean FROM EHR e CONTAINS COMPOSITION c \
         CONTAINS OBSERVATION o[openEHR-EHR-OBSERVATION.blood_pressure.v2] WHERE {}",
        patient_predicate()
    )
}

/// A result set whose cells are numbers.
#[derive(Debug, Deserialize)]
struct Numbers {
    rows: Vec<Vec<f64>>,
}

/// Sends the query node `node` received through its proxy straight to the
/// node, and returns the one row it answers.
async fn answered_by(node: &ProxiedNode, sent: &Capture) -> Result<Vec<f64>, Box<dyn Error>> {
    let answer = reqwest::Client::new()
        .post(format!("{}/v1/query/aql", node.node.api_root()))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(sent.body.clone())
        .send()
        .await?
        .error_for_status()?
        .json::<Numbers>()
        .await?;
    let [row] = answer.rows.as_slice() else {
        return Err(format!("a node answers its aggregate in one row: {:?}", answer.rows).into());
    };
    Ok(row.clone())
}

// conformance: CP-10 CP-32 track-5
#[tokio::test]
async fn an_undirected_avg_is_the_mean_weighted_by_the_counts_each_node_returns() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    let two_at_a = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR_A,
            subject: Some(PATIENT),
        }],
        template: true,
        compositions: [
            DemoComposition::FirstHospital,
            DemoComposition::SecondHospital,
        ]
        .into_iter()
        .map(|composition| CompositionSeed {
            ehr_id: EHR_A,
            composition,
        })
        .collect(),
    };
    seed::seed(&nodes.a.api_root(), &two_at_a).await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, DemoComposition::FirstClinic),
    )
    .await?;
    clear(&nodes);
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;

    let root = track9::options(&in_process(&app)).await?;
    assert!(
        root.federation
            .aggregates
            .decomposable
            .iter()
            .any(|function| function == "AVG"),
        "the gateway declares AVG decomposable, so §11.6.3 holds it to the per-node counts"
    );

    let reply = exchange(&app, post_aql(&mean_systolic(), &[])?).await?;
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    crate::facade::schema::validate(&reply.text)?;
    let answer: Numbers = serde_json::from_str(&reply.text)?;
    let [row] = answer.rows.as_slice() else {
        return Err(format!("§11.6.3: one row, never one per node: {}", reply.text).into());
    };
    let [mean] = row.as_slice() else {
        return Err(format!("one column, the client's: {}", reply.text).into());
    };

    let mut parts = Vec::new();
    for node in [&nodes.a, &nodes.b] {
        let sent = queries(node);
        let [capture] = sent.as_slice() else {
            return Err(format!("each node is asked once: {sent:?}").into());
        };
        let text = String::from_utf8_lossy(&capture.body);
        assert!(
            text.contains(&format!("SUM({SYSTOLIC})"))
                && text.contains(&format!("COUNT({SYSTOLIC})")),
            "§11.6.3: AVG is asked as its SUM and its COUNT, the per-node count retrieved: {text}"
        );
        let node_row = answered_by(node, capture).await?;
        let [sum, count] = node_row.as_slice() else {
            return Err(format!("the node answers its SUM and COUNT: {node_row:?}").into());
        };
        parts.push((*sum, *count));
    }
    let [(sum_a, count_a), (sum_b, count_b)] = parts.as_slice() else {
        return Err("two nodes answered".into());
    };
    assert_eq!(
        (2.0, 1.0),
        (*count_a, *count_b),
        "node A holds two systolic values and node B one, so the counts differ"
    );
    let weighted = (sum_a + sum_b) / (count_a + count_b);
    let unweighted = f64::midpoint(sum_a / count_a, sum_b / count_b);
    assert!(
        (weighted - unweighted).abs() > 1.0,
        "the case tells a weighted mean from a mean of the node means"
    );
    assert!(
        (mean - weighted).abs() < 1e-9,
        "§11.6.3: the answer {mean} is the mean weighted by the node counts, {weighted}"
    );
    Ok(())
}
