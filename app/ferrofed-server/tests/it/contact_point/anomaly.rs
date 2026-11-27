// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The anomaly rules of the requests national contact points relay
//! (`NCPeH` Architecture Specifications 10.0.0 §5.1.9): detection "does not
//! block traffic", so a request the gateway cannot count is served,
//! uncounted, and is the security event `anomaly-uncounted`. The count
//! itself runs against PostgreSQL in `e2e::anomaly`.

use std::error::Error;
use std::sync::Arc;

use ferrofed_server::security::Event;
use ferrofed_server::state::AppState;
use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;

use super::emergency::{asked, contact_point_router, emergency_settings, records, relayed_for};
use crate::access::node_with_rows;
use crate::facade::received;

type TestResult = Result<(), Box<dyn Error>>;

#[tokio::test]
async fn a_request_no_store_counts_is_served_uncounted_with_no_warning() -> TestResult {
    let (a, b) = (node_with_rows(&[]).await, node_with_rows(&[]).await);
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let settings = emergency_settings(
        dir.path(),
        (&a.uri(), &b.uri()),
        &repository,
        "\n[anomaly.requests]\nthreshold = 1\nwindow = '1h'\n",
    )?;
    let app = contact_point_router(Arc::new(AppState::build(&settings)?));
    let before = Event::AnomalyUncounted.counted();
    for _ in 0..2 {
        let (status, text) = asked(&app, &relayed_for("Qz7-hp-74", "TREATMENT")).await?;
        assert_eq!(
            StatusCode::OK,
            status,
            "§5.1.9: traffic is not blocked: {text}"
        );
    }
    assert_eq!(2, received(&a).await?.len(), "node A was asked each time");
    assert!(
        Event::AnomalyUncounted.counted() >= before + 2,
        "each uncounted request is a security event"
    );
    let records = records(&repository, 2).await?;
    assert!(
        records.iter().all(|record| !record["outcomeDesc"]
            .as_str()
            .is_some_and(|text| text.contains("EHDSI_WARNING"))),
        "an uncounted request raises no warning: {records:?}"
    );
    Ok(())
}
