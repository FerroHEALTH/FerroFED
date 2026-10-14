// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log needs no regional binding: a production gateway with a
//! registry starts in every build, with the IHE binding, the Dutch binding,
//! both or neither, and stores the record of each access in its `[audit]`
//! spool before the answer leaves (Regulation (EU) 2025/327 Annex II 3.2).
//! The suite runs this file in each build `cargo hack --each-feature` makes.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use http::StatusCode;

use super::TestResult;
use crate::facade::{body, gateway, node_answering, post, registry};
use crate::feed_audit::{await_feed_state, spool_key, spooled};
use crate::support::call;

/// A query that names no patient, which every member is asked.
const POPULATION: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c";

/// The `[audit]` tables of a production gateway: a repository over `https`
/// no connection reaches, and the spool directory `spool`.
fn audit_tables(spool: &str) -> String {
    format!(
        "\n[audit]\ndestination = \"repository\"\n\n[audit.repository]\n\
         url = \"https://{}/arr/\"\nhostname = \"gateway.example.org\"\n{spool}\n",
        ferrofed_testkit::unreachable::ADDRESS
    )
}

/// Annex II 3.2: whatever bindings the build compiles, a production
/// gateway with a registry answers a federated query only once its access
/// record is stored in the spool, which the health report then shows waiting
/// for the repository.
#[tokio::test]
async fn a_production_gateway_of_any_build_spools_the_access_record() -> TestResult {
    let a = node_answering("8a1f0c2e-1d3b-4c5a-9e6f-7a8b9c0d1e2f::cdr-a.example.org::1").await;
    let b = node_answering("3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f::cdr-b.example.org::1").await;
    let dir = tempfile::tempdir()?;
    let (key, spool) = spool_key(dir.path());
    let app = gateway(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"production\"",
        &audit_tables(&key),
    )?;
    let (status, text) = call(app.clone(), post(body(POPULATION)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, spooled(&spool)?, "the access record is in the spool");
    assert_eq!(
        Some("degraded"),
        await_feed_state(&app, "degraded").await?.as_deref(),
        "the record waits for the repository"
    );
    Ok(())
}
