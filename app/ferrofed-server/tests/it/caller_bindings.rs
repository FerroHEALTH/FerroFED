// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2, held per verified caller: a
//! routed follow-up by the caller whose query resolved the patient is routed
//! by that caller's binding first, and another caller never sees it; a
//! consent denial drops the bindings naming the denied member (N27a, N41).
//! The session a binding belongs to is the verified caller's issuer, subject
//! and client; no specification governs that key: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_server::telemetry::{Rendering, subscriber};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use serde::Deserialize;
use wiremock::ResponseTemplate;

use crate::facade::{EHR_A, body, dev_gateway, patient_query, post};
use crate::path_ehr_id::{answer, holder};
use crate::support::{Logs, asked, bearer_as, mount};

type TestResult = Result<(), Box<dyn Error>>;

/// A version uid node A minted, which [`holder`] serves.
const VERSION_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

/// The debug event of a routed path `ehr_id`, read for the step that found
/// its owner.
#[derive(Debug, Deserialize)]
struct Routed {
    message: String,
    step: Option<String>,
}

/// Node A, which holds [`EHR_A`] and answers the query, and node B, which
/// holds nothing, behind a development gateway whose cross-reference
/// resolves the patient at node A.
async fn gateway(dir: &std::path::Path) -> Result<(Router, Server, Server), Box<dyn Error>> {
    let a = holder().await;
    mount(
        &a,
        "POST",
        String::from("/v1/query/aql"),
        ResponseTemplate::new(200).set_body_raw(
            br##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[["uid-a"]]}"##
                .to_vec(),
            "application/json",
        ),
    )
    .await;
    let b = Server::start().await;
    let app = dev_gateway(dir, &a.uri(), &b.uri(), &[("node-a", EHR_A)])?;
    Ok((app, a, b))
}

/// The read of node A's composition, as the default caller or as `subject`.
fn read(subject: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::get(format!("/v1/ehr/{EHR_A}/composition/{VERSION_A}"));
    if let Some(subject) = subject {
        request = request.header(header::AUTHORIZATION, bearer_as(subject)?);
    }
    Ok(request.body(Body::empty())?)
}

/// Sends `requests` in order and returns the step every routed read was
/// found by, with the status of each answer.
async fn steps(
    app: &Router,
    requests: Vec<Request<Body>>,
) -> Result<(Vec<StatusCode>, Vec<String>), Box<dyn Error>> {
    let logs = Logs::default();
    let capture = subscriber(Rendering::Json, "debug", false, logs.clone())?;
    let guard = tracing::subscriber::set_default(capture);
    let mut statuses = Vec::new();
    for request in requests {
        statuses.push(answer(app.clone(), request).await?.0);
    }
    drop(guard);
    let mut found = Vec::new();
    for line in logs.text().lines() {
        let event: Routed = serde_json::from_str(line)?;
        if event.message == "routed a path ehr_id" {
            found.push(event.step.ok_or("a routed read names its step")?);
        }
    }
    Ok((statuses, found))
}

// conformance: CP-33
#[tokio::test]
async fn the_caller_who_queried_is_routed_by_its_binding_first() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _a, b) = gateway(dir.path()).await?;
    let (statuses, found) = steps(&app, vec![post(body(&patient_query())?)?, read(None)?]).await?;
    assert_eq!(vec![StatusCode::OK, StatusCode::OK], statuses);
    assert_eq!(
        vec!["binding".to_owned()],
        found,
        "step 2 answers before the index (§12.5.1, N41)"
    );
    assert_eq!(
        Vec::<(String, String)>::new(),
        asked(&b).await?,
        "a member the binding does not name is never probed"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn another_caller_never_sees_the_binding() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _a, _b) = gateway(dir.path()).await?;
    let (statuses, found) = steps(
        &app,
        vec![
            post(body(&patient_query())?)?,
            read(Some("another-caller"))?,
        ],
    )
    .await?;
    assert_eq!(vec![StatusCode::OK, StatusCode::OK], statuses);
    assert_eq!(
        vec!["index".to_owned()],
        found,
        "another caller has no binding, so the shared ehr_id index answers (step 3)"
    );
    Ok(())
}

// conformance: CP-33
#[tokio::test]
async fn a_caller_with_no_query_has_no_binding() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (app, _a, _b) = gateway(dir.path()).await?;
    let (statuses, found) = steps(&app, vec![read(None)?]).await?;
    assert_eq!(vec![StatusCode::OK], statuses);
    assert_eq!(
        vec!["ask-all".to_owned()],
        found,
        "nothing was resolved, so nothing is bound and a read is probed (step 4)"
    );
    Ok(())
}
