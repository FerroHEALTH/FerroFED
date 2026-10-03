// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A node that is unreachable, too slow, or refuses the onward credentials.

use std::time::Duration;

use http::{StatusCode, header};
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, PER_NODE_TIMEOUT_MS};
use crate::support::SLACK;

use super::{TestResult, failed_at, names_node_a, node};

#[tokio::test]
async fn an_unreachable_node_is_a_504_naming_the_endpoint() -> TestResult {
    let closed = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = closed.local_addr()?.port();
    drop(closed);
    let (status, code, headers) = failed_at(&format!("http://127.0.0.1:{port}")).await?;
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-unreachable"),
        (status, code.as_str()),
        "§11.2"
    );
    names_node_a(&headers, "unreachable");
    Ok(())
}

#[tokio::test]
async fn a_node_that_does_not_answer_in_time_is_a_504_naming_the_endpoint() -> TestResult {
    let a = node(
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(200).set_delay(Duration::from_millis(PER_NODE_TIMEOUT_MS) + SLACK),
    )
    .await;
    let (status, code, headers) = failed_at(&a.uri()).await?;
    assert_eq!(
        (StatusCode::GATEWAY_TIMEOUT, "node-timeout"),
        (status, code.as_str()),
        "§11.2"
    );
    names_node_a(&headers, "time-out");
    Ok(())
}

#[tokio::test]
async fn a_node_refusing_the_onward_credentials_is_a_424_never_a_challenge_to_the_client()
-> TestResult {
    let a = node(
        "GET",
        format!("/v1/ehr/{EHR_A}"),
        ResponseTemplate::new(401).insert_header("WWW-Authenticate", "Bearer realm=\"cdr-a\""),
    )
    .await;
    let (status, code, headers) = failed_at(&a.uri()).await?;
    assert_eq!(
        (StatusCode::FAILED_DEPENDENCY, "node-refused"),
        (status, code.as_str())
    );
    assert!(
        headers.get(header::WWW_AUTHENTICATE).is_none(),
        "the node's challenge is not the client's"
    );
    names_node_a(&headers, "401");
    Ok(())
}
