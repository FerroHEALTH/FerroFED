// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The two-node harness against the real CDR products, behind the
//! `FERROFED_E2E` gate: both nodes start, take a seed over ITS-REST alone,
//! answer through their proxies, journal what reached them, and misbehave on
//! demand one node at a time.

use ferrofed_testkit::containers::{self, Product, ProxiedNode};
use ferrofed_testkit::proxy::Fault;
use ferrofed_testkit::seed::{
    self, CompositionSeed, DemoComposition, EhrSeed, PatientId, SeedPlan,
};
use http::StatusCode;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// The fixed `ehr_id` the first patient has on node A.
const FIRST_ON_A: Uuid = Uuid::from_u128(0x2222_2222_2222_4222_8222_2222_2222_2222);
/// The fixed `ehr_id` the first patient has on node B.
const FIRST_ON_B: Uuid = Uuid::from_u128(0x1111_1111_1111_4111_8111_1111_1111_1111);

/// The first synthetic patient, in a different assigning domain on each node.
const FIRST_A: PatientId = PatientId::new(1, 1);
const FIRST_B: PatientId = PatientId::new(2, 1);

fn plan(ehr_id: Uuid, patient: PatientId, composition: DemoComposition) -> SeedPlan {
    SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id,
            subject: Some(patient),
        }],
        template: true,
        compositions: vec![CompositionSeed {
            ehr_id,
            composition,
        }],
    }
}

/// Reads `GET {api}/v1/ehr/{ehr_id}` through the node's proxy.
async fn read_ehr(node: &ProxiedNode, ehr_id: Uuid) -> reqwest::Result<reqwest::Response> {
    reqwest::Client::new()
        .get(format!("{}/v1/ehr/{ehr_id}", node.api_root()))
        .header("Accept", "application/json")
        .send()
        .await
}

#[tokio::test]
async fn both_nodes_start_take_a_seed_and_journal_it() {
    if !containers::e2e_enabled() {
        return;
    }
    let nodes = containers::two_nodes().await.unwrap();
    assert_eq!(
        nodes.a.node.product(),
        Product::FerroEhr,
        "node A runs FerroEHR"
    );
    assert_eq!(
        nodes.b.node.product(),
        Product::Ehrbase,
        "node B runs EHRbase"
    );

    let report_a = seed::seed(
        &nodes.a.api_root(),
        &plan(FIRST_ON_A, FIRST_A, DemoComposition::FirstHospital),
    )
    .await
    .unwrap();
    // NOTE: BASE object_ref.adoc §Attributes allows `.` in a namespace;
    // EHRbase 2.36.0 refuses it, so node B's EHR is seeded with no subject.
    let mut plan_b = plan(FIRST_ON_B, FIRST_B, DemoComposition::FirstClinic);
    if let Some(ehr) = plan_b.ehrs.first_mut() {
        ehr.subject = None;
    }
    let report_b = seed::seed(&nodes.b.api_root(), &plan_b).await.unwrap();
    assert_eq!(
        report_a.compositions.len(),
        1,
        "node A committed the composition"
    );
    assert_eq!(
        report_b.compositions.len(),
        1,
        "node B committed the composition"
    );

    for (node, ehr_id) in [(&nodes.a, FIRST_ON_A), (&nodes.b, FIRST_ON_B)] {
        let journal = node.proxy.journal();
        let steps: Vec<(&str, &str)> = journal
            .iter()
            .map(|capture| (capture.method.as_str(), capture.path.as_str()))
            .collect();
        let api = node.node.product().api_path();
        assert_eq!(
            steps,
            vec![
                ("PUT", format!("{api}/v1/ehr/{ehr_id}").as_str()),
                (
                    "POST",
                    format!("{api}/v1/definition/template/adl1.4").as_str()
                ),
                (
                    "POST",
                    format!("{api}/v1/ehr/{ehr_id}/composition").as_str()
                ),
            ],
            "the {:?} journal holds the three ITS-REST seed calls and nothing else",
            node.node.product()
        );

        let answer = read_ehr(node, ehr_id).await.unwrap();
        assert_eq!(
            answer.status(),
            StatusCode::OK,
            "the seeded EHR reads back on {:?}",
            node.node.product()
        );
    }

    assert!(
        nodes.a.proxy.journal_contains(FIRST_A.value().as_bytes()),
        "node A's journal shows the subject the seed wrote, so a leak check can see one"
    );
    assert!(
        !nodes
            .a
            .proxy
            .journal_contains(FIRST_B.namespace().as_bytes()),
        "node A never saw node B's assigning domain"
    );
}

#[tokio::test]
async fn every_fault_is_injectable_per_node() {
    if !containers::e2e_enabled() {
        return;
    }
    let nodes = containers::two_nodes().await.unwrap();
    seed::seed(
        &nodes.a.api_root(),
        &SeedPlan {
            ehrs: vec![EhrSeed {
                ehr_id: FIRST_ON_A,
                subject: None,
            }],
            ..SeedPlan::default()
        },
    )
    .await
    .unwrap();
    seed::seed(
        &nodes.b.api_root(),
        &SeedPlan {
            ehrs: vec![EhrSeed {
                ehr_id: FIRST_ON_B,
                subject: None,
            }],
            ..SeedPlan::default()
        },
    )
    .await
    .unwrap();

    nodes
        .b
        .proxy
        .set_fault(Fault::Status(StatusCode::INTERNAL_SERVER_ERROR));
    assert_eq!(
        read_ehr(&nodes.b, FIRST_ON_B).await.unwrap().status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "node B answers the injected 500"
    );
    assert_eq!(
        read_ehr(&nodes.a, FIRST_ON_A).await.unwrap().status(),
        StatusCode::OK,
        "node A is untouched by node B's fault"
    );

    let delay = Duration::from_millis(1500);
    nodes.b.proxy.set_fault(Fault::Delay(delay));
    let started = Instant::now();
    let delayed = read_ehr(&nodes.b, FIRST_ON_B).await.unwrap();
    assert!(
        started.elapsed() >= delay,
        "node B's answer is held for the delay"
    );
    assert_eq!(
        delayed.status(),
        StatusCode::OK,
        "the delayed read is still answered"
    );

    nodes.b.proxy.set_fault(Fault::Refuse);
    assert!(
        read_ehr(&nodes.b, FIRST_ON_B).await.is_err(),
        "a refusing proxy answers nothing"
    );

    nodes.b.proxy.clear_fault();
    assert_eq!(
        read_ehr(&nodes.b, FIRST_ON_B).await.unwrap().status(),
        StatusCode::OK,
        "node B serves again once its fault is cleared"
    );

    nodes.b.node.stop().await.unwrap();
    assert_eq!(
        read_ehr(&nodes.b, FIRST_ON_B).await.unwrap().status(),
        StatusCode::BAD_GATEWAY,
        "a stopped node B is a 502 at its proxy"
    );
    assert_eq!(
        read_ehr(&nodes.a, FIRST_ON_A).await.unwrap().status(),
        StatusCode::OK,
        "node A still serves while node B is down"
    );
}
