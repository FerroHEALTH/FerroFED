// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console's answer as the browser receives it: a node's hostile
//! values arrive escaped in the HTML the page shows, the answer is never
//! cached or compressed, and a plain form post runs no query.

use std::error::Error;

use axum::body::Body;
use ferrofed_viewer::session::COOKIE;
use http::{Request, StatusCode};

use crate::query::{PATIENT, answered, gateway, patient_query, signed_in_console};
use crate::support::{get_as, header, send};

/// A §9.4-shaped answer whose every node-controlled value is hostile: the
/// cells, the column name and path, an endpoint's organisation and error,
/// and the dedup mode.
const HOSTILE: &str = r#"{
  "q": "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
  "columns": [ { "name": "</th><script>x</script>", "path": "\" onmouseover=\"alert(1)" } ],
  "rows": [ ["<script>alert(1)</script>"], ["<img src=x onerror=alert(1)>"] ],
  "meta": {
    "federation": {
      "complete": false,
      "endpoints": [
        { "id": "node_1", "status": "active", "latency_ms": 5, "organisation": "<b>Org</b> & co" },
        { "id": "node_2", "status": "time-out", "latency_ms": 9, "error": "<b>late</b> & gone" }
      ],
      "dedup": { "mode": "<i>mode</i>" }
    }
  }
}"#;

// The answer reaches the page through `inner_html`, so the whole path from a
// node's answer to that sink must escape what the node wrote.
#[tokio::test]
async fn a_nodes_hostile_values_reach_the_page_escaped() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, HOSTILE.to_owned()).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let answer = answered(&service, &session, patient_query()).await?;
    let html = &answer.html;
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(1)&gt;",
        "&lt;/th&gt;&lt;script&gt;x&lt;/script&gt;",
        "&quot; onmouseover=&quot;alert(1)",
        "&lt;b&gt;Org&lt;/b&gt; &amp; co",
        "&lt;b&gt;late&lt;/b&gt; &amp; gone",
        "&lt;i&gt;mode&lt;/i&gt;",
    ] {
        assert!(html.contains(escaped), "{escaped}: {html}");
    }
    for raw in [
        "<script",
        "<img",
        "onmouseover=\"",
        "<b>",
        "<i>",
        "</th><script",
    ] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    Ok(())
}

/// A `POST` of the query function as the console's own page sends it,
/// asking for `encoding`.
fn run_accepting(
    session: &ferrofed_viewer::session::SessionId,
    encoding: &str,
) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::post("/api/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("accept-encoding", encoding)
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(patient_query()))?)
}

// An answer carries what the operator entered beside rows an attacker would
// want, so it is never compressed (BREACH), and never kept by a cache.
#[tokio::test]
async fn an_answer_is_neither_compressed_nor_cached() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, HOSTILE.to_owned()).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, text) = send(&service, run_accepting(&session, "gzip, br")?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(text.len() > 256, "{} bytes", text.len());
    assert_eq!("", header(&response, "content-encoding"));
    assert_eq!("no-store", header(&response, "cache-control"));
    let (page, _body) = send(&service, get_as("/query", &session)?).await?;
    assert_eq!("no-store", header(&page, "cache-control"));
    Ok(())
}

// A page without its bundle posts the form plainly; the console runs no
// query whose answer no page would show, and its redirect quotes nothing.
#[tokio::test]
async fn a_plain_form_post_runs_no_query() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, HOSTILE.to_owned()).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let request = Request::post("/api/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "text/html,application/xhtml+xml,*/*;q=0.8")
        .header("sec-fetch-site", "same-origin")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .body(Body::from(patient_query()))?;
    let (response, text) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status(), "{text}");
    let location = header(&response, "location");
    assert!(!location.contains(PATIENT), "{location}");
    assert!(!text.contains(PATIENT), "{text}");
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(
        asked
            .iter()
            .all(|request| request.method != http::Method::POST),
        "the gateway ran a query"
    );
    Ok(())
}

#[tokio::test]
async fn the_submit_waits_for_the_page_to_load() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, HOSTILE.to_owned()).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (_response, page) = send(&service, get_as("/query", &session)?).await?;
    assert!(page.contains(r#"<button type="submit" disabled"#), "{page}");
    assert!(
        page.contains("The form is ready once the page has loaded."),
        "{page}"
    );
    Ok(())
}
