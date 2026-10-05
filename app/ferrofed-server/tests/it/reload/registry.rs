// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A valid document replacing the running registry, and a request in flight on the old one.

use std::error::Error;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use wiremock::Mock;
use wiremock::matchers::{method, path};

use crate::facade::{EHR_A, EHR_B, crossref, node_answering};

use super::{EHR_C, Gateway, Holding, TOKEN_C, TestResult, hits, ids, member};

#[tokio::test]
async fn a_reload_adds_removes_and_changes_members() -> TestResult {
    let a1 = node_answering("uid-a1::cdr-a.example.org::1").await;
    let a2 = node_answering("uid-a2::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let c = node_answering("uid-c::cdr-c.example.org::1").await;
    let gateway = Gateway::start(
        &(member("a", &a1.uri()) + &member("b", &b.uri())),
        "",
        &crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
    )?;
    let (status, before) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{before}");

    gateway.write_registry(&(member("a", &a2.uri()) + &member("c", &c.uri())))?;
    gateway.write_config("", &crossref(&[("node-a", EHR_A), ("node-c", EHR_C)]))?;
    let applied = gateway.reloader.reload()?;

    assert_eq!(2, applied.members);
    assert_eq!(ids::<EndpointId>(&["node-c-pub"])?, applied.endpoints_added);
    assert_eq!(
        ids::<EndpointId>(&["node-b-pub"])?,
        applied.endpoints_removed
    );
    assert_eq!(ids::<NodeId>(&["node-b"])?, applied.members_removed);
    assert!(applied.needs_restart.is_empty(), "{applied:?}");
    let federation = gateway.federation()?;
    let snapshot = federation.snapshot();
    assert!(snapshot.node(&"node-b".parse()?).is_none());
    let moved = snapshot
        .endpoint(&"node-a-pub".parse()?)
        .ok_or("node-a-pub stays")?;
    assert_eq!(a2.uri(), moved.url().as_str().trim_end_matches('/'));

    let (status, after) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{after}");
    assert!(
        after.contains("uid-a2::") && after.contains("uid-c::"),
        "{after}"
    );
    assert!(
        !after.contains("uid-a1::") && !after.contains("uid-b::"),
        "{after}"
    );
    assert_eq!(
        (1, 1, 1, 1),
        (
            hits(&a1).await?,
            hits(&b).await?,
            hits(&a2).await?,
            hits(&c).await?
        ),
        "the moved and the removed endpoint are never called again"
    );
    Ok(())
}

#[tokio::test]
async fn credentials_follow_the_reloaded_registry() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let c = node_answering("uid-c::cdr-c.example.org::1").await;
    let gateway = Gateway::start(&member("a", &a.uri()), "", &crossref(&[("node-a", EHR_A)]))?;
    gateway.write_registry(&(member("a", &a.uri()) + &member("c", &c.uri())))?;
    let credentials = format!("[credentials.\"node-c-pub\"]\nbearer_token = \"{TOKEN_C}\"\n");
    gateway.write_config("", &(crossref(&[("node-c", EHR_C)]) + "\n" + &credentials))?;
    gateway.reloader.reload()?;

    let (status, text) = gateway.ask().await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let requests = c.received_requests().await.ok_or("recording is on")?;
    let sent = requests.first().ok_or("node C was asked")?;
    let authorization = sent
        .headers
        .get(http::header::AUTHORIZATION)
        .ok_or("node C's client sends its credentials")?;
    assert_eq!(format!("Bearer {TOKEN_C}"), authorization.to_str()?);
    Ok(())
}

#[tokio::test]
async fn credentials_for_an_endpoint_the_document_dropped_refuse_the_reload() -> TestResult {
    let a = node_answering("uid-a::cdr-a.example.org::1").await;
    let b = node_answering("uid-b::cdr-b.example.org::1").await;
    let credentials = format!("[credentials.\"node-b-pub\"]\nbearer_token = \"{TOKEN_C}\"\n");
    let tables = crossref(&[("node-a", EHR_A)]) + "\n" + &credentials;
    let gateway = Gateway::start(
        &(member("a", &a.uri()) + &member("b", &b.uri())),
        "",
        &tables,
    )?;
    let running = gateway.federation()?;
    gateway.write_registry(&member("a", &a.uri()))?;

    let refused = gateway
        .reloader
        .reload()
        .err()
        .ok_or("credentials naming no endpoint are refused, as at boot")?;
    assert_eq!("credentials", refused.class());
    assert!(Arc::ptr_eq(&running, &gateway.federation()?));
    Ok(())
}

#[tokio::test]
async fn a_request_in_flight_finishes_on_the_registry_it_started_with() -> TestResult {
    let (arrived, mut arrival) = tokio::sync::mpsc::unbounded_channel();
    let (release, held) = mpsc::channel();
    let old = Server::start().await;
    let answer = String::from(
        r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-old::cdr-a.example.org::1"]]}"##,
    );
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(Holding {
            arrived,
            release: Mutex::new(held),
            answer,
        })
        .mount(&old)
        .await;
    let new = node_answering("uid-new::cdr-a.example.org::1").await;
    let gateway = Gateway::start(
        &member("a", &old.uri()),
        "",
        &crossref(&[("node-a", EHR_A)]),
    )?;

    let first = gateway.ask();
    let then = async {
        tokio::time::timeout(Duration::from_secs(5), arrival.recv())
            .await?
            .ok_or("the first request reached the old node")?;
        gateway.write_registry(&member("a", &new.uri()))?;
        gateway.reloader.reload()?;
        let second = gateway.ask().await?;
        release.send(())?;
        Ok::<_, Box<dyn Error>>(second)
    };
    let (first, second) = tokio::join!(first, then);
    let ((first_status, first), (second_status, second)) = (first?, second?);

    assert_eq!(StatusCode::OK, first_status, "{first}");
    assert!(
        first.contains("uid-old::") && !first.contains("uid-new::"),
        "the request in flight finished on its own registry: {first}"
    );
    assert_eq!(StatusCode::OK, second_status, "{second}");
    assert!(
        second.contains("uid-new::") && !second.contains("uid-old::"),
        "a request after the reload sees the new registry: {second}"
    );
    assert_eq!(1, hits(&old).await?, "the old address got only the first");
    Ok(())
}
