// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `[telemetry] trace_sample_ratio`: the share of the gateway's own traces
//! that are exported, decided once at the root, the request span, and
//! followed by every span under it; validated at load to `0.0` to `1.0`,
//! with `1.0` as the default.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;

use ferrofed_server::config::Config;
use ferrofed_server::telemetry::SampleRatio;
use http::StatusCode;

use super::{Exported, TestResult, named};
use crate::facade::{EHR_A, EHR_B, body, dev_gateway, node_answering, patient_query, post};
use crate::support::send;

/// The ratio `value`, which the test knows is in range.
fn ratio(value: f64) -> Result<SampleRatio, String> {
    SampleRatio::new(value).ok_or_else(|| format!("{value} is a ratio"))
}

/// The spans one federated query leaves at `ratio`, and the `traceparent`
/// values the two nodes received.
async fn traced_at(
    ratio: SampleRatio,
) -> Result<(usize, usize, Vec<String>), Box<dyn std::error::Error>> {
    let exported = Exported::sampled(ratio)?;
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let rows = [("node-a", EHR_A), ("node-b", EHR_B)];
    let app = dev_gateway(dir.path(), &a.uri(), &b.uri(), &rows)?;
    let response = send(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "the query is answered");
    let spans = exported.spans()?;
    let mut traceparents = Vec::new();
    for server in [&a, &b] {
        let requests = server.received_requests().await.ok_or("recording is on")?;
        for request in &requests {
            for value in request.headers.get_all("traceparent") {
                traceparents.push(String::from_utf8_lossy(value.as_bytes()).into_owned());
            }
        }
    }
    let roots = named(&spans, "POST /v1/query/aql").len();
    Ok((roots, spans.len(), traceparents))
}

#[tokio::test]
async fn a_ratio_of_one_exports_every_root_and_its_whole_tree() -> TestResult {
    let (roots, spans, traceparents) = traced_at(ratio(1.0)?).await?;
    assert_eq!(1, roots, "the request span is exported");
    assert!(spans > roots, "and every span under it: {spans}");
    assert_eq!(2, traceparents.len(), "each node joins the trace");
    for traceparent in &traceparents {
        assert!(traceparent.ends_with("-01"), "sampled: {traceparent}");
    }
    Ok(())
}

#[tokio::test]
async fn a_ratio_of_zero_exports_no_root_and_no_span_under_it() -> TestResult {
    let (roots, spans, traceparents) = traced_at(ratio(0.0)?).await?;
    assert_eq!((0, 0), (roots, spans), "no span is exported");
    assert_eq!(
        2,
        traceparents.len(),
        "each node still receives the gateway's trace"
    );
    for traceparent in &traceparents {
        assert!(traceparent.ends_with("-00"), "not sampled: {traceparent}");
    }
    Ok(())
}

#[test]
fn every_trace_is_sampled_unless_a_ratio_is_set() -> TestResult {
    let settings = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert_eq!(SampleRatio::ALL, settings.telemetry.trace_sample_ratio);
    let text = "[telemetry]\ntrace_sample_ratio = 0.25\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    assert_eq!(ratio(0.25)?, settings.telemetry.trace_sample_ratio);
    Ok(())
}

#[test]
fn a_ratio_outside_zero_to_one_is_refused_at_load() -> TestResult {
    for refused in ["1.5", "-0.1", "nan", "inf"] {
        let text = format!("[telemetry]\ntrace_sample_ratio = {refused}\n");
        let error = Config::from_sources(Some(&text), &BTreeMap::new())?
            .resolve()
            .err()
            .ok_or_else(|| format!("{refused} is refused"))?;
        assert!(
            error.to_string().contains("telemetry.trace_sample_ratio"),
            "{refused}: {error}"
        );
    }
    Ok(())
}

#[test]
fn the_ratio_is_held_to_zero_to_one_and_finite() {
    for accepted in [0.0, 0.5, 1.0] {
        assert!(SampleRatio::new(accepted).is_some(), "{accepted}");
    }
    for refused in [-0.0001, 1.0001, f64::NAN, f64::INFINITY] {
        assert!(SampleRatio::new(refused).is_none(), "{refused}");
    }
}
