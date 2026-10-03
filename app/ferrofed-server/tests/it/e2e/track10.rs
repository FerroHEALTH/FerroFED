// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 10 against two FerroEHR nodes, behind the `FERROFED_E2E` gate: the
//! four positions on the fan-out, on a directed query and on the single-node
//! route, and the converse write, judged on the journal of the capturing
//! proxy in front of each node (§16.3 track 10; N33, N34; CP-26).
//!
//! Both nodes hold the patient's identifier on `EHR_STATUS.subject`, so a
//! subject predicate the gateway leaked would match there; a node that
//! answers rows to a query keyed on its `ehr_id` alone is invocable on it
//! (N34).

use ferrofed_testkit::containers::{self, ProxiedNode, TwoNodes};
use ferrofed_testkit::seed::{self, CompositionSeed, DemoComposition, EhrSeed, SeedPlan};
use uuid::Uuid;

use crate::track10::cases;
use crate::track10::{
    EHR_A, EHR_B, ENDPOINT_A, ENDPOINT_B, Member, PATIENT, TestResult, Topology,
    commit_lands_byte_identical, run_all,
};

/// A seed of [`PATIENT`]'s EHR at `ehr_id`, the template, and `composition`
/// in it when given.
fn plan(ehr_id: Uuid, composition: Option<DemoComposition>) -> SeedPlan {
    SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id,
            subject: Some(PATIENT),
        }],
        template: true,
        compositions: composition
            .into_iter()
            .map(|composition| CompositionSeed {
                ehr_id,
                composition,
            })
            .collect(),
    }
}

/// The two FerroEHR nodes as the suite addresses them.
fn topology(nodes: &TwoNodes) -> Topology<'_> {
    Topology {
        a: member(ENDPOINT_A, &nodes.a, EHR_A),
        b: member(ENDPOINT_B, &nodes.b, EHR_B),
        real: true,
    }
}

/// One FerroEHR node as the suite addresses it, through its proxy.
fn member<'a>(endpoint: &'static str, node: &'a ProxiedNode, ehr_id: Uuid) -> Member<'a> {
    Member {
        endpoint,
        proxy: &node.proxy,
        api_root: node.api_root(),
        ehr_id,
    }
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_four_positions_reach_neither_ferroehr_node_on_any_path() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, Some(DemoComposition::FirstHospital)),
    )
    .await?;
    seed::seed(
        &nodes.b.api_root(),
        &plan(EHR_B, Some(DemoComposition::FirstClinic)),
    )
    .await?;
    let topology = topology(&nodes);
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;

    let mut all = Vec::new();
    for position in [
        cases::in_external_ref(),
        cases::in_party_identified(),
        cases::in_projection(),
        cases::in_query_string_or_header(),
    ] {
        all.extend(position.clone());
        all.extend(cases::directed(position));
    }
    all.extend(cases::on_the_route(EHR_A));
    all.extend(cases::in_the_ehr_id_slot());
    run_all(&app, &topology, &all).await
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_committed_dv_identifier_arrives_at_a_ferroehr_node_byte_identical() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed::seed(&nodes.a.api_root(), &plan(EHR_A, None)).await?;
    let topology = topology(&nodes);
    let dir = tempfile::tempdir()?;
    let app = topology.gateway(dir.path())?;
    let composition = crate::e2e::composition_carrying(PATIENT)?;
    commit_lands_byte_identical(&app, &topology, &composition).await
}
