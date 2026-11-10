// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log under concurrent load (#1812): a production gateway whose
//! records go to a disk spool answers every one of many concurrent queries,
//! each once its record is stored, though the spool writes one record at a
//! time and syncs each to the disk (Regulation (EU) 2025/327 Annex II 3.2).
//! No specification governs the load: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use http::StatusCode;

use super::TestResult;
use crate::facade::{body, gateway, node_answering, post, registry};
use crate::feed_audit::{spool_key, spooled};
use crate::support::call;

/// A query that names no patient, which every member is asked.
const POPULATION: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

/// How many queries run at once: each record waits for the durable writes
/// queued before it, so the last waits for all of them.
const QUERIES: usize = 64;

/// Many concurrent queries each wait their turn at the spool, and every one
/// is answered with its record stored: none is refused `access-unrecorded`
/// while the request's own time is left.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_queries_are_each_answered_once_their_records_are_stored() -> TestResult {
    let a = node_answering("8a1f0c2e-1d3b-4c5a-9e6f-7a8b9c0d1e2f::cdr-a.example.org::1").await;
    let b = node_answering("3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let (key, spool) = spool_key(dir.path());
    let tables = format!(
        "\n[audit]\ndestination = \"repository\"\n\n[audit.repository]\n\
         endpoint.url = \"https://{}/arr/\"\nhostname = \"gateway.example.org\"\n{key}\n",
        ferrofed_testkit::unreachable::ADDRESS
    );
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"production\"",
        &tables,
    )?;
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..QUERIES {
        let app = app.clone();
        tasks.spawn(async move {
            let request = post(body(POPULATION).map_err(|failed| failed.to_string())?)
                .map_err(|failed| failed.to_string())?;
            call(app, request)
                .await
                .map_err(|failed| failed.to_string())
        });
    }
    while let Some(joined) = tasks.join_next().await {
        let (status, text) = joined??;
        assert_eq!(StatusCode::OK, status, "{text}");
    }
    assert_eq!(
        QUERIES,
        spooled(&spool)?,
        "every access record is in the spool"
    );
    Ok(())
}
