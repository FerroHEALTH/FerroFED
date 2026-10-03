// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Track 9 over two FerroEHR nodes with the gateway mounted at a prefix of
//! the deployment's choosing (§16.3 track 9; N28, N29, N31, CP-21, CP-22).
//!
//! The gateway is served on a real socket under `/fed/openehr`, and the
//! client is a plain HTTP client given that base URL and nothing else: no
//! federation header, no `/rest/openehr`. It reads the self-description,
//! queries one EHR in both addressing forms of N29, reads the EHR at the
//! canonical path, and commits a composition there, which the `ehr_id` index
//! routes (§12.5.1 step 3). The nodes are asked at their own base, and the
//! gateway's base reaches neither.

use std::error::Error;
use std::time::Duration;

use ferrofed_testkit::containers::{self, API_PATH, TwoNodes};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::seed::{self, DemoComposition, EhrSeed, SeedPlan};
use http::StatusCode;
use tokio::net::TcpListener;

use crate::e2e::{
    Answer, EHR_A, EHR_B, PATIENT, TestResult, composition_carrying, dev_resolver, gateway_mounted,
    plan,
};

/// The prefix the deployment mounts the gateway at.
const BASE: &str = "/fed/openehr";

/// The query of node A's one EHR in the `WHERE` form of N29.
fn where_form() -> String {
    format!("SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_A}'")
}

/// The same query in the `FROM EHR` form of N29.
fn from_form() -> String {
    format!("SELECT c/uid/value FROM EHR e[ehr_id/value='{EHR_A}'] CONTAINS COMPOSITION c")
}

/// Seeds node A with [`EHR_A`] and one composition, and node B with
/// [`EHR_B`], and clears both journals.
async fn seeded() -> Result<TwoNodes, Box<dyn Error>> {
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

/// The rows of the answer to `aql`, posted under `base` as an ITS-REST
/// `AdhocQueryExecute`.
async fn rows(
    client: &reqwest::Client,
    base: &str,
    aql: &str,
) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    #[derive(serde::Serialize)]
    struct Adhoc<'a> {
        q: &'a str,
    }
    let response = client
        .post(format!("{base}/v1/query/aql"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_string(&Adhoc { q: aql })?)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    crate::facade::schema::validate(&text)?;
    Ok(serde_json::from_str::<Answer>(&text)?.rows)
}

/// Whether `capture` is the commit of a composition.
fn is_commit(capture: &Capture) -> bool {
    capture.method == "POST" && capture.path.ends_with("/composition")
}

// conformance: CP-21 CP-22
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
    let client = reqwest::Client::new();

    let options = client
        .request(reqwest::Method::OPTIONS, format!("{base}/"))
        .send()
        .await?;
    assert_eq!(StatusCode::OK, options.status(), "OPTIONS {{base}}/");
    crate::facade::schema::validate_options(&options.text().await?)?;

    let where_rows = rows(&client, &base, &where_form()).await?;
    let from_rows = rows(&client, &base, &from_form()).await?;
    assert_eq!(1, where_rows.len(), "node A's one composition");
    assert_eq!(
        where_rows, from_rows,
        "N29: both forms answer the same rows"
    );

    let read = client.get(format!("{base}/v1/ehr/{EHR_A}")).send().await?;
    assert_eq!(
        StatusCode::OK,
        read.status(),
        "the read at the canonical path"
    );
    assert_eq!(
        Some("node-a-pub"),
        read.headers()
            .get("openEHR-federation-endpoint")
            .and_then(|value| value.to_str().ok()),
        "N31"
    );

    let write = client
        .post(format!("{base}/v1/ehr/{EHR_A}/composition"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(composition_carrying(PATIENT)?)
        .send()
        .await?;
    let status = write.status();
    let location = write
        .headers()
        .get(http::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let text = write.text().await?;
    assert_eq!(StatusCode::CREATED, status, "the write: {text}");
    assert!(
        location.is_some_and(|location| location.contains(API_PATH)),
        "the node's own Location, unmodified (N31)"
    );

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
