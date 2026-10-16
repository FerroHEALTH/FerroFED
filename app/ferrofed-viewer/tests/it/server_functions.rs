// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every server function the views, the shell and the admin actions call:
//! each is a public endpoint of the console, so each is a `POST` alone, and
//! each refuses a caller without a live session or from another origin
//! before it asks the gateway anything.

use std::error::Error;

use axum::body::Body;
use http::{Request, StatusCode};

use crate::support::{OPERATOR_TOKEN, get_as, send};
use crate::views::{gateway, signed_in_console};

/// Every server function a view or the shell loads through or an admin action posts to,
/// with the form body it takes.
const SERVER_FUNCTIONS: [(&str, &str); 15] = [
    ("/api/gateway", ""),
    ("/api/deployment", ""),
    ("/api/theme", "theme=dark"),
    ("/api/members", ""),
    ("/api/integrity", "show="),
    ("/api/member", "key=node_1"),
    ("/api/routing", "offset=0"),
    ("/api/dashboard", ""),
    ("/api/stored-queries", "offset=0"),
    ("/api/stored-query", "name=org.example%3A%3Acompositions"),
    ("/api/federation", ""),
    ("/api/clear-ehr-id", "form%5Bconfirm%5D=yes"),
    ("/api/repair-drift", "form%5Bconfirm%5D=yes"),
    ("/api/configuration", ""),
    ("/api/configure", "form%5Bconfirm%5D=yes"),
];

/// A `POST` of the server function at `route` with `form`, carrying the
/// session cookie `cookie` when given.
fn call(route: &str, form: &str, cookie: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post(route)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin");
    if let Some(cookie) = cookie {
        request = request.header(
            "cookie",
            format!("{}={cookie}", ferrofed_viewer::session::COOKIE),
        );
    }
    Ok(request.body(Body::from(form.to_owned()))?)
}

// Each server function is a public endpoint of the console, so each refuses
// a caller with no live session before it asks the gateway anything.
#[tokio::test]
async fn every_server_function_refuses_a_caller_without_a_live_session()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, _session) = signed_in_console(&gateway)?;
    for (route, form) in SERVER_FUNCTIONS {
        for cookie in [None, Some("forged")] {
            let (response, body) = send(&service, call(route, form, cookie)?).await?;
            assert_ne!(StatusCode::OK, response.status(), "{route} {cookie:?}");
            assert!(body.contains("SignedOut"), "{route} {cookie:?}: {body}");
        }
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "the gateway was asked {} times",
        asked.len()
    );
    Ok(())
}

#[tokio::test]
async fn every_server_function_answers_a_live_session() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (route, form) in SERVER_FUNCTIONS {
        let (response, body) = send(&service, call(route, form, Some(session.as_str()))?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{route}: {body}");
        assert!(!body.contains(OPERATOR_TOKEN), "{route}: {body}");
    }
    Ok(())
}

// Each server function is a public endpoint a cross-site page could post
// to, so each refuses a request that does not come from the console's own
// pages before it reads the session or asks the gateway.
#[tokio::test]
async fn every_server_function_refuses_a_request_from_another_origin() -> Result<(), Box<dyn Error>>
{
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    for (route, form) in SERVER_FUNCTIONS {
        for from in [
            vec![("sec-fetch-site", "cross-site")],
            vec![("origin", "https://attacker.example.net")],
            Vec::new(),
        ] {
            let mut request = Request::post(route)
                .header("content-type", "application/x-www-form-urlencoded")
                .header("accept", "application/json")
                .header(
                    "cookie",
                    format!("{}={}", ferrofed_viewer::session::COOKIE, session.as_str()),
                );
            for (name, value) in &from {
                request = request.header(*name, *value);
            }
            let (response, body) =
                send(&service, request.body(Body::from(form.to_owned()))?).await?;
            assert_eq!(
                StatusCode::FORBIDDEN,
                response.status(),
                "{route} {from:?}: {body}"
            );
        }
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "the gateway was asked {} times",
        asked.len()
    );
    Ok(())
}

// A server function is a POST alone: a GET would pass the origin check, and
// the session cookie travels on a cross-site top-level GET.
#[tokio::test]
async fn no_server_function_answers_a_get() -> Result<(), Box<dyn Error>> {
    let gateway = gateway().await?;
    let (_state, service, session) = signed_in_console(&gateway)?;
    let routes = SERVER_FUNCTIONS
        .iter()
        .map(|(route, form)| (*route, *form))
        .chain([
            ("/api/query", "form%5Bkind%5D=aql&form%5Baql%5D=SELECT%201"),
            ("/api/query-options", ""),
        ]);
    for (route, form) in routes {
        let (response, body) =
            send(&service, get_as(&format!("{route}?{form}"), &session)?).await?;
        assert!(
            response.status().is_client_error(),
            "{route}: {} {body}",
            response.status()
        );
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked.is_empty(),
        "the gateway was asked {} times",
        asked.len()
    );
    Ok(())
}
