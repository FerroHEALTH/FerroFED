// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 9 over two FerroEHR nodes with the gateway mounted at a prefix of
//! the deployment's choosing (§16.3 track 9; N28, N29, N31, CP-21, CP-22).
//!
//! The gateway is served on a real socket under `/fed/openehr`, and the
//! client is a plain HTTP client given that base URL and nothing else: no
//! federation header, no `/rest/openehr`. It reads the self-description,
//! queries one EHR in both addressing forms of N29, reads the EHR at the
//! canonical path, and commits a composition there, which the `ehr_id` index
//! routes (§12.5.1 step 3). Those are the conformance run's own checks
//! ([`ferrofed_server::conformance::scenarios::track9`]), driven through its
//! own HTTP client; this suite adds that the nodes are asked at their own
//! base, and the gateway's base reaches neither.

use std::error::Error;
use std::time::Duration;

use ferrofed_server::conformance::client::HttpGateway;
use ferrofed_server::conformance::scenarios::track9;
use ferrofed_testkit::containers::{self, TwoNodes};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, DemoComposition, EhrSeed, SeedPlan};
use secrecy::SecretString;
use tokio::net::TcpListener;

use crate::e2e::scenario::{Validated, fixture};
use crate::e2e::{
    EHR_A, EHR_B, PATIENT, TestResult, composition_carrying, dev_resolver, gateway_mounted, plan,
};

/// The prefix the deployment mounts the gateway at.
const BASE: &str = "/fed/openehr";

/// Seeds node A with [`EHR_A`] and one composition, and node B with
/// [`EHR_B`], and clears both journals.
pub(crate) async fn seeded() -> Result<TwoNodes, Box<dyn Error>> {
    let nodes = containers::two_nodes().await?;
    seed::seed(
        &nodes.a.api_root(),
        &plan(EHR_A, DemoComposition::FirstHospital),
    )
    .await?;
    let only_the_ehr = SeedPlan {
        ehrs: vec![EhrSeed {
            ehr_id: EHR_B,
            subject: Some(PATIENT),
        }],
        template: false,
        compositions: Vec::new(),
    };
    seed::seed(&nodes.b.api_root(), &only_the_ehr).await?;
    nodes.a.proxy.clear_journal();
    nodes.b.proxy.clear_journal();
    Ok(nodes)
}

/// Whether `capture` is the commit of a composition.
fn is_commit(capture: &Capture) -> bool {
    capture.method == "POST" && capture.path.ends_with("/composition")
}

// conformance: CP-21 CP-22 CP-24 track-9
#[tokio::test]
async fn an_unmodified_client_given_only_the_base_url_reads_and_writes_at_a_prefix() -> TestResult {
    if !containers::e2e_enabled() {
        return Ok(());
    }
    let nodes = Box::pin(seeded()).await?;
    let dir = tempfile::tempdir()?;
    let app = gateway_mounted(dir.path(), (&nodes.a, &nodes.b), &dev_resolver(), BASE)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}{BASE}", listener.local_addr()?);
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(ferrofed_server::serve_until(
        listener,
        app,
        Duration::from_secs(1),
        async {
            // NOTE: a dropped sender ends the wait as a sent signal does.
            let _signalled = stopped.await;
        },
    ));
    let client = Validated(HttpGateway::new(
        base.parse()?,
        SecretString::from(crate::support::token()?),
    )?);
    let fixture = fixture(Some(1), Some(0))?;
    let node_a = fixture.member("node-a-pub").ok_or("node A is a member")?;

    track9::plain_client(
        &client,
        node_a,
        (&EHR_A.to_string(), 1),
        &composition_carrying(PATIENT)?,
    )
    .await?;

    let (a, b) = (nodes.a.proxy.journal(), nodes.b.proxy.journal());
    for capture in a.iter().chain(&b) {
        assert!(
            !capture.contains(BASE.as_bytes()),
            "the gateway's base never reaches a node (N28): {} {}",
            capture.method,
            capture.path
        );
        assert!(
            is_commit(capture) || !capture.contains(PATIENT.value().as_bytes()),
            "N33: only the committed body carries the identifier, as modelled data"
        );
    }
    assert_eq!(
        1,
        a.iter().filter(|c| is_commit(c)).count(),
        "one commit at node A"
    );
    assert!(
        b.iter().all(|capture| capture.method == "GET"),
        "node B is only ever probed, never queried or written"
    );

    drop(stop);
    serving.await??;
    Ok(())
}
