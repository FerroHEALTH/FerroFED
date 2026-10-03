// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node request counter and duration histogram: one count per request
//! sent to a member, by registry endpoint and the §11.1 outcome the
//! per-endpoint report gives it, and one duration per measured request
//! (§9.5, §11.1).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::time::Duration;

use axum::body::Body;
use ferrofed_testkit::unreachable;
use http::{Request, StatusCode, header};
use openehr_federation::headers::COMPLETENESS;
use wiremock::MockServer;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{
    EHR_A, EHR_B, body, crossref, node_answering, node_failing, patient_query, registry,
};
use crate::metrics::{Metered, count, value};
use crate::path_ehr_id::{holder, stranger};
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The counter's and the histogram's Prometheus names.
const REQUESTS: &str = "ferrofed_node_requests_total";
const DURATION_COUNT: &str = "ferrofed_node_request_duration_seconds_count";
const DURATION_SUM: &str = "ferrofed_node_request_duration_seconds_sum";
const DURATION_BUCKET: &str = "ferrofed_node_request_duration_seconds_bucket";

/// The patient's `ehr_id` at node C and at node D.
const EHR_C: &str = "3333cccc-3333-4333-8333-333333333333";
const EHR_D: &str = "4444dddd-4444-4444-8444-444444444444";

/// The per-node timeout of the fan-out, and how long node C keeps quiet.
const PER_NODE_MS: u64 = 600;
const SILENCE: Duration = Duration::from_millis(2_500);

/// Node C and node D, appended to the registry of node A and node B.
fn two_more(c: &str, d: &str) -> String {
    format!(
        r#"
[[node]]
id = "node-c"
organisation = "org-a"
system_id = "cdr-c.example.org"

[[node]]
id = "node-d"
organisation = "org-b"
system_id = "cdr-d.example.org"

[[endpoint]]
id = "node-c-pub"
node = "node-c"
url = "{c}"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-d-pub"
node = "node-d"
url = "{d}"
connection_type = "openehr-rest-query"
managing_organisation = "org-b"
"#
    )
}

/// A node answering the query only after [`SILENCE`].
async fn node_silent() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(200).set_delay(SILENCE))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn a_fan_out_counts_and_times_each_member_by_its_outcome() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_failing(500).await;
    let c = node_silent().await;
    let dir = tempfile::tempdir()?;
    let rows = crossref(&[
        ("node-a", EHR_A),
        ("node-b", EHR_B),
        ("node-c", EHR_C),
        ("node-d", EHR_D),
    ]);
    let gateway = Metered::start(
        dir.path(),
        &registry(&a.uri(), &b.uri(), &two_more(&c.uri(), unreachable::BASE)),
        ("profile = \"development\"", &rows),
        (PER_NODE_MS, 3_000),
    )?;
    let request = Request::post("/v1/query/aql")
        .header(header::CONTENT_TYPE, "application/json")
        .header(COMPLETENESS, "partial")
        .body(Body::from(body(&patient_query())?))?;
    let (status, text) = call(gateway.app.clone(), request).await?;
    assert_eq!(StatusCode::OK, status, "a partial answer: {text}");

    let samples = gateway.scraped()?;
    for (endpoint, outcome) in [
        ("node-a-pub", "active"),
        ("node-b-pub", "node-error"),
        ("node-c-pub", "time-out"),
        ("node-d-pub", "offline"),
    ] {
        assert_eq!(
            Some("1".to_owned()),
            count(
                &samples,
                REQUESTS,
                &[("endpoint", endpoint), ("outcome", outcome)]
            ),
            "{endpoint} {outcome}: {samples:?}"
        );
        assert_eq!(
            Some("1".to_owned()),
            count(&samples, DURATION_COUNT, &[("endpoint", endpoint)]),
            "{endpoint} is timed once"
        );
        assert_eq!(
            Some("1".to_owned()),
            count(
                &samples,
                DURATION_BUCKET,
                &[("endpoint", endpoint), ("le", "+Inf")]
            ),
        );
    }
    let waited =
        value(&samples, DURATION_SUM, &[("endpoint", "node-c-pub")]).ok_or("node C is timed")?;
    assert!(
        waited >= Duration::from_millis(PER_NODE_MS).as_secs_f64() * 0.5,
        "the silent node is timed to its deadline: {waited}"
    );
    let counted = samples
        .iter()
        .filter(|sample| sample.name == REQUESTS)
        .count();
    assert_eq!(4, counted, "one series per member asked: {samples:?}");
    Ok(())
}

#[tokio::test]
async fn a_routed_read_counts_the_probe_of_each_member_and_the_timed_read() -> TestResult {
    let a = holder().await;
    let b = stranger().await;
    let dir = tempfile::tempdir()?;
    let gateway = Metered::start(
        dir.path(),
        &registry(&a.uri(), &b.uri(), ""),
        ("", ""),
        (2_000, 3_000),
    )?;
    let version = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
    let read = Request::get(format!("/v1/ehr/{EHR_A}/composition/{version}")).body(Body::empty())?;
    let (status, text) = call(gateway.app.clone(), read).await?;
    assert_eq!(StatusCode::OK, status, "{text}");

    let samples = gateway.scraped()?;
    assert_eq!(
        Some("2".to_owned()),
        count(
            &samples,
            REQUESTS,
            &[("endpoint", "node-a-pub"), ("outcome", "active")]
        ),
        "the probe and the read: {samples:?}"
    );
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            REQUESTS,
            &[("endpoint", "node-b-pub"), ("outcome", "active")]
        ),
        "a member that holds no such EHR answered the probe: {samples:?}"
    );
    assert_eq!(
        Some("1".to_owned()),
        count(&samples, DURATION_COUNT, &[("endpoint", "node-a-pub")]),
        "the read is timed and the probe is not"
    );
    assert_eq!(
        None,
        count(&samples, DURATION_COUNT, &[("endpoint", "node-b-pub")]),
        "a probe carries no measurement"
    );
    Ok(())
}
