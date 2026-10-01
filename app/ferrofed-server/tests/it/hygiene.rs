// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Identifier hygiene on the log (§5.4.3): a façade query carrying a sentinel
//! patient identifier in every place a client can put one leaves the sentinel
//! in no log line at any level, through the router and through a real socket
//! with the whole HTTP stack logging at `trace`
//! (`.claude/rules/identifier-hygiene.md`, the log and telemetry test).

use crate::request_log::logged;
use crate::support::{self, Logs, request_lines};
use axum::body::Body;
use ferrofed_server::serve_until;
use ferrofed_server::telemetry::{Rendering, subscriber};
use http::{Request, header};
use std::error::Error as StdError;
use std::time::Duration;
use tokio::net::TcpListener;

/// The synthetic patient identifier. Never a real identifier of any kind.
const SENTINEL: &str = "SENTINEL-PATIENT-0f3c9a";

/// The filter that admits every line of every crate.
const EVERYTHING: &str = "trace";

/// [`EVERYTHING`] but the test's own HTTP client, which logs the URL it sends
/// at `debug` and is the caller here, not the gateway.
const EVERYTHING_BUT_THE_CLIENT: &str = "trace,reqwest=off";

/// The façade AQL, identifying the patient the openEHR-idiomatic way (§1).
fn aql() -> String {
    format!(
        "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = '{SENTINEL}'"
    )
}

/// Returns `text` percent-encoded for a query string.
fn encoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push('%');
            for nibble in [byte >> 4, byte & 0x0f] {
                out.extend(char::from_digit(u32::from(nibble), 16).map(|c| c.to_ascii_uppercase()));
            }
        }
    }
    out
}

/// The façade queries, one per carrier a client can put the identifier in.
fn queries() -> Result<Vec<Request<Body>>, http::Error> {
    Ok(vec![
        Request::get(format!("/v1/query/aql?q={}", encoded(&aql()))).body(Body::empty())?,
        Request::get(format!(
            "/v1/query/aql?q={}&patient={SENTINEL}&offset=0&fetch=10",
            encoded("SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient")
        ))
        .body(Body::empty())?,
        Request::post("/v1/query/aql")
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-patient", SENTINEL)
            .body(Body::from(format!(
                r#"{{"q":"{}","query_parameters":{{"patient":"{SENTINEL}"}}}}"#,
                aql()
            )))?,
        Request::get(format!("/v1/ehr?subject_id={SENTINEL}&subject_namespace=synthetic"))
            .body(Body::empty())?,
    ])
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_facade_query_through_the_router_leaves_the_sentinel_in_no_log_line()
-> Result<(), Box<dyn StdError>> {
    let requests = queries()?;
    let count = requests.len();
    let text = logged(&support::app(), EVERYTHING, requests)?;
    assert_eq!(
        count,
        request_lines(&text)?.len(),
        "every query was logged, so the check is not vacuous: {text}"
    );
    assert!(
        !text.contains(SENTINEL),
        "the sentinel reached the log: {text}"
    );
    Ok(())
}

#[test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
fn a_facade_query_over_a_real_socket_leaves_the_sentinel_in_no_log_line_of_any_crate()
-> Result<(), Box<dyn StdError>> {
    let logs = Logs::default();
    let capture = subscriber(
        Rendering::Json,
        EVERYTHING_BUT_THE_CLIENT,
        false,
        logs.clone(),
    )?;
    // One thread runs the server, the client and every task they spawn, so the
    // thread's default subscriber sees every line the whole stack writes.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let statuses = tracing::subscriber::with_default(capture, || {
        runtime.block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").await?;
            let address = listener.local_addr()?;
            let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
            let server = tokio::spawn(serve_until(
                listener,
                support::app(),
                Duration::from_secs(5),
                async move {
                    if stopped.await.is_err() {
                        tracing::debug!("the stop channel closed");
                    }
                },
            ));
            let client = reqwest::Client::new();
            let mut statuses = Vec::new();
            for request in queries()? {
                let (parts, body) = request.into_parts();
                let bytes = axum::body::to_bytes(body, 64 * 1024).await?;
                let mut outbound = client
                    .request(parts.method, format!("http://{address}{}", parts.uri))
                    .body(bytes.to_vec());
                for (name, value) in &parts.headers {
                    outbound = outbound.header(name, value);
                }
                statuses.push(outbound.send().await?.status());
            }
            stop.send(()).map_err(|()| "the server is gone")?;
            server.await??;
            Ok::<_, Box<dyn StdError>>(statuses)
        })
    })?;
    let text = logs.text();
    assert_eq!(4, statuses.len(), "every query was answered");
    assert_eq!(
        statuses.len(),
        request_lines(&text)?.len(),
        "every query was logged, so the check is not vacuous: {text}"
    );
    assert!(
        !text.contains(SENTINEL),
        "the sentinel reached the log: {text}"
    );
    Ok(())
}
