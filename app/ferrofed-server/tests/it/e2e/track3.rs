// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 3, the directed query and the endpoint pin, over two FerroEHR nodes:
//! `FROM ENDPOINT` and `FROM ORGANISATION` select the node set, a named
//! member where the patient does not resolve is reported and not errored,
//! the endpoint header selects what the directive selects, and an undirected
//! query asks only the members the localizer names (§8, §14.1, §16.3 track 3
//! and track 4; N4, N10, N11, N12, N35; CP-5, CP-6, CP-11, CP-28, CP-30,
//! CP-37).
//!
//! The client-visible checks of the directives and the header are the
//! conformance run's own
//! ([`ferrofed_server::conformance::scenarios::track3`]); this suite adds
//! what reached each node, and the localizer it makes fail. Selected
//! ENDPOINT attributes in the rows are scored in `e2e::attributes`.

use ferrofed_server::conformance::client::Federated;
use ferrofed_server::conformance::scenarios::track3;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::proxy::{CapturingProxy, Fault};
use http::StatusCode;
use openehr_federation::options::OptionsRoot;

use crate::e2e::pixm::{DOMAIN_A, nodes_and_pix, pixm_resolver, pixm_resolver_at};
use crate::e2e::scenario::{
    Options, Reply, asked, clear, exchange, fixture, gateway_with, in_process, nobody_asked,
    patient_predicate, post_aql, queries, seed_both,
};
use crate::e2e::{EHR_A, TestResult, assert_no_patient_identifier_on_the_wire};

/// The undirected patient query.
fn undirected() -> String {
    format!(
        "SELECT c/uid/value AS uid FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        patient_predicate()
    )
}

/// Asserts a `200` and returns the answer.
fn answered(reply: &Reply) -> Result<Federated, Box<dyn std::error::Error>> {
    assert_eq!(StatusCode::OK, reply.status, "{}", reply.text);
    reply.federated()
}

/// Asserts that no node request carries the directive or a targeting header.
fn no_targeting_on_the_wire(nodes: &TwoNodes) {
    for node in [&nodes.a, &nodes.b] {
        for absent in [
            "ENDPOINT",
            "ORGANISATION",
            "node-a-pub",
            "node-b-pub",
            "org-b",
        ] {
            assert!(
                !node.proxy.journal_contains(absent.as_bytes()),
                "§8.1: {} received {absent:?}",
                node.node.system_id()
            );
        }
    }
}

// conformance: CP-6 CP-37 track-3
#[tokio::test]
async fn a_directive_selects_the_node_set_and_leaves_the_row_shape_alone() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;
    let (gateway, fixture) = (in_process(&app), fixture(Some(1), Some(1))?);

    track3::directive_endpoint(&gateway, &fixture, "node-a-pub").await?;
    assert!(asked(&nodes.b).is_empty(), "CP-6: node B was not named");
    let sent = queries(&nodes.a);
    assert!(
        sent.iter()
            .all(|capture| String::from_utf8_lossy(&capture.body).contains(&EHR_A.to_string())),
        "the per-node scope stays the resolved ehr_id"
    );
    no_targeting_on_the_wire(&nodes);

    clear(&nodes);
    track3::directive_organisation(&gateway, &fixture, "org-b").await?;
    assert!(asked(&nodes.a).is_empty(), "CP-6: node A was not named");
    no_targeting_on_the_wire(&nodes);
    Ok(())
}

// conformance: CP-6 track-3
#[tokio::test]
async fn a_named_member_where_the_patient_does_not_resolve_is_reported_not_errored() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A)])).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        resolver: pixm_resolver(&pix),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;

    track3::named_unresolved(
        &in_process(&app),
        &fixture(Some(1), None)?,
        "node-a-pub",
        "node-b-pub",
    )
    .await?;
    assert!(asked(&nodes.b).is_empty(), "N8: node B is never asked");
    assert_no_patient_identifier_on_the_wire(&nodes);
    Ok(())
}

// conformance: CP-28 track-3
#[tokio::test]
async fn the_endpoint_header_selects_what_the_directive_selects() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = containers::two_nodes().await?;
    seed_both(&nodes).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_with(dir.path(), &nodes, &Options::default())?;
    let (gateway, fixture) = (in_process(&app), fixture(Some(1), Some(1))?);

    track3::header_selects(&gateway, &fixture, "node-b-pub").await?;
    assert!(asked(&nodes.a).is_empty(), "node A is named by neither");
    no_targeting_on_the_wire(&nodes);

    clear(&nodes);
    track3::conflict_refused(&gateway, &fixture, "node-b-pub", "node-a-pub").await?;
    nobody_asked(&nodes, "CP-28: a refused request asks no node");

    track3::parameter_targets_nothing(&gateway, &fixture, "node-b-pub").await?;
    for node in [&nodes.a, &nodes.b] {
        assert!(
            !node.proxy.journal_contains(b"endpoint="),
            "the parameter reaches no node"
        );
    }
    Ok(())
}

// conformance: CP-5 CP-11 CP-30 track-3 track-4
#[tokio::test]
async fn an_undirected_query_asks_only_the_members_the_localizer_names() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let (nodes, pix) = Box::pin(nodes_and_pix(vec![(DOMAIN_A, EHR_A)])).await?;
    let proxy = CapturingProxy::start(pix.origin()).await?;
    let dir = tempfile::tempdir()?;
    let options = Options {
        resolver: pixm_resolver_at(&format!("{}/fhir/", proxy.origin())),
        node_selection: "localized",
        tables: "[federation.localization]\ntimeout_ms = 5000\n".to_owned(),
        ..Options::default()
    };
    let app = gateway_with(dir.path(), &nodes, &options)?;

    let reply = exchange(&app, post_aql(&undirected(), &[])?).await?;
    let answer = answered(&reply)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-localized")],
        answer.statuses(),
        "CP-5, CP-11: the member the localizer did not name is not-localized"
    );
    assert!(
        answer.meta.federation.complete,
        "CP-30: not-localized members do not clear complete"
    );
    assert_eq!(1, answer.rows.len(), "{}", reply.text);
    assert!(asked(&nodes.b).is_empty(), "CP-5: node B is never asked");

    clear(&nodes);
    for fault in [
        Fault::Status(StatusCode::SERVICE_UNAVAILABLE),
        Fault::Refuse,
    ] {
        proxy.set_fault(fault);
        let reply = exchange(&app, post_aql(&undirected(), &[])?).await?;
        let answer = answered(&reply)?;
        for record in &answer.meta.federation.endpoints {
            assert_eq!(
                "not-localized", record.status,
                "CP-5: a localizer that cannot answer fails closed ({fault:?})"
            );
            assert!(
                record.error.is_some(),
                "CP-5: every member carries the error"
            );
        }
        nobody_asked(&nodes, "CP-5: no dispatch and no silent ask-all");
    }

    let options_request = http::Request::options("/").body(axum::body::Body::empty())?;
    let described = exchange(&app, options_request).await?;
    assert_eq!(StatusCode::OK, described.status);
    crate::facade::schema::validate_options(&described.text)?;
    let root: OptionsRoot = serde_json::from_str(&described.text)?;
    assert_eq!(
        "closed", root.federation.localization.on_failure,
        "CP-5: the gateway behaves as OPTIONS declares"
    );
    Ok(())
}
