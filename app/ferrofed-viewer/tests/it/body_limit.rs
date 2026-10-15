// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every request body is held to a limit before anything reads it: a body
//! over the limit is `413` with or without a session, and no server
//! function runs, while an import of TOML at its own limit still runs.

use std::error::Error;

use axum::body::Body;
use ferrofed_testkit::mock::Server;
use ferrofed_viewer::configure::import::IMPORT_LIMIT;
use ferrofed_viewer::server::{BODY_LIMIT, CONFIGURE_BODY_LIMIT};
use ferrofed_viewer::session::COOKIE;
use http::{Request, StatusCode};

use crate::configure::{answers, form, signed_in_console};
use crate::support::send;

/// What revision 2 changes against the running revision 1.
const IMPORTED: &str = r#"{"revision":{"id":2,"base":1,"origin":"import","restores":null,
    "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
    "at":"2026-10-15T09:01:00Z","reason":"from git","toml":""},
    "diff":{"from":1,"to":2,"changes":[]}}"#;

/// A form post of `body` to `path`, as the hydrated page sends one, with
/// its `Content-Length` and the session `cookie` when one is given.
fn post(path: &str, body: String, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(path)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("content-length", body.len())
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin");
    if let Some(session) = cookie {
        request = request.header("cookie", format!("{COOKIE}={session}"));
    }
    Ok(request.body(Body::from(body))?)
}

/// How many requests `listener` received.
async fn asked(listener: &Server) -> Result<usize, Box<dyn Error>> {
    Ok(listener
        .received_requests()
        .await
        .ok_or("the stub records requests")?
        .len())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_with_no_session_is_refused_before_any_function_runs()
-> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, _session) = signed_in_console(&listener)?;
    let toml = "#".repeat(CONFIGURE_BODY_LIMIT);
    let body = form(&[("action", "import"), ("toml", &toml), ("confirm", "yes")]);
    let (response, _) = send(&service, post("/api/configure", body, None)?).await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    let (response, _) = send(
        &service,
        post("/api/configure", "x".repeat(CONFIGURE_BODY_LIMIT + 1), None)?,
    )
    .await?;
    assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_to_any_other_route_is_refused() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    for path in ["/api/configuration", "/api/query", "/logout"] {
        let request = post(path, "x".repeat(BODY_LIMIT + 1), Some(session.as_str()))?;
        let (response, _) = send(&service, request).await?;
        assert_eq!(StatusCode::PAYLOAD_TOO_LARGE, response.status(), "{path}");
    }
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_oversized_body_sent_without_its_length_runs_no_function() -> Result<(), Box<dyn Error>>
{
    let listener = Server::start().await;
    let (service, session) = signed_in_console(&listener)?;
    let request = Request::post("/api/configure")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(form(&[
            ("action", "import"),
            ("toml", &"#".repeat(CONFIGURE_BODY_LIMIT)),
            ("reason", "r"),
            ("confirm", "yes"),
        ])))?;
    let (response, _) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status());
    assert_eq!(0, asked(&listener).await?, "no replica is asked");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn an_import_at_its_own_limit_still_runs() -> Result<(), Box<dyn Error>> {
    let listener = Server::start().await;
    answers(&listener, ("POST", "/admin/config/import"), 201, IMPORTED).await;
    let (service, session) = signed_in_console(&listener)?;
    let toml = "#".repeat(IMPORT_LIMIT);
    let body = form(&[
        ("action", "import"),
        ("base", "1"),
        ("toml", &toml),
        ("reason", "from git"),
        ("confirm", "yes"),
    ]);
    assert!(
        body.len() > BODY_LIMIT,
        "the import is over the default limit"
    );
    let (response, text) = send(
        &service,
        post("/api/configure", body, Some(session.as_str()))?,
    )
    .await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(
        text.contains("Replica gateway-0 imported revision 2 as a dry run."),
        "{text}"
    );
    Ok(())
}
