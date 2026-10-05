// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The access log under the overload limits: a request one of them refuses
//! reaches no data and writes no record, and an admitted request passes every
//! layer and is recorded naming the caller the gate verified (Annex II 3.2).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::num::NonZeroU32;

use ferrofed_server::config::limits::CallerRateSettings;
use ferrofed_testkit::atna_feed::FeedRepository;
use http::StatusCode;

use super::{LAB_REPORT, TestResult, accesses, composition, gateway_under, node_with_rows};
use crate::facade::{body, patient_query, post, settings_with_room};
use crate::feed_audit::{SETTLE, names_the_default_caller};
use crate::support::{call, error_body};

const UID_A: &str = "0b7e3c2a-5f1d-4a9e-8c6b-2d4f6a8b0c1e::cdr-a.example.org::1";

#[tokio::test]
async fn a_rate_limited_request_writes_no_access_record() -> TestResult {
    let node_a = node_with_rows(&[composition(LAB_REPORT, UID_A)]).await;
    let node_b = node_with_rows(&[]).await;
    let repository = FeedRepository::start().await;
    let dir = tempfile::tempdir()?;
    let mut server = settings_with_room();
    server.overload.caller_rate = Some(CallerRateSettings {
        requests_per_second: NonZeroU32::new(1).ok_or("one is positive")?,
        burst: NonZeroU32::new(1).ok_or("one is positive")?,
    });
    let app = gateway_under(
        dir.path(),
        (&node_a.uri(), &node_b.uri()),
        &repository,
        "",
        &server,
    )?;

    let (status, text) = call(app.clone(), post(body(&patient_query())?)?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "admitted through every layer: {text}"
    );
    let records = accesses(&repository.wait_for(1, SETTLE).await)?;
    let [record] = records.as_slice() else {
        return Err(format!("one record of the admitted access, got {records:?}").into());
    };
    names_the_default_caller(&record.to_string())?;
    let reached = node_a.received_requests().await.unwrap_or_default().len();

    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::TOO_MANY_REQUESTS, status, "{text}");
    assert_eq!("rate-limited", error_body(&text)?.code);
    assert_eq!(
        reached,
        node_a.received_requests().await.unwrap_or_default().len(),
        "a refused request reaches no node"
    );
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2: a record is stored before its answer
    // leaves, so the repository holds every record of the refused request by now.
    let records = accesses(&repository.records())?;
    assert_eq!(1, records.len(), "a refused request writes no record");
    Ok(())
}
