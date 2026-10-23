// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console against a stub gateway answering the specification's
//! §9.4 result-set example: the query posts to its own page, which answers
//! with the rows, every endpoint's part and whether the answer is
//! complete; a refusal shows its status and code, a form that cannot be
//! sent is refused without quoting it, the federation choices reach the
//! gateway as its headers, and an identifier the operator enters reaches
//! no URL and no log line.

use std::error::Error;

use axum::body::Body;
use ferrofed_console::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::stub::{options_example, result_set_example};
use crate::support::{Captured, OPERATOR_TOKEN, console, get_as, send, signed_in};

/// A synthetic patient identifier, as an operator would type it.
const PATIENT: &str = "synthetic-patient-48151623";

/// `text` with `from` replaced by `to`, which must be there.
fn replaced(text: &str, from: &str, to: &str) -> Result<String, Box<dyn Error>> {
    if text.contains(from) {
        Ok(text.replacen(from, to, 1))
    } else {
        Err(format!("the example no longer carries {from}").into())
    }
}

/// The §9.4 example with `node_2` timed out, as best-effort answers it:
/// `node_1`'s row alone, and `complete` false (§11.4).
fn incomplete_example() -> Result<String, Box<dyn Error>> {
    let text = result_set_example()?;
    let text = replaced(&text, "\"complete\": true", "\"complete\": false")?;
    let text = replaced(
        &text,
        "\"status\": \"active\",\n          \"latency_ms\": 306, \"product\": \"VendorY\", \"version\": \"2026.1\", \"row_count\": 1 }",
        "\"status\": \"time-out\", \"error\": \"no answer within the per-node budget\",\n          \"latency_ms\": 2000, \"product\": \"VendorY\", \"version\": \"2026.1\" }",
    )?;
    replaced(
        &text,
        ",\n    [\"node_2\", \"cdr2.rso.nl\", \"6ba7b810-9dad-11d1-80b4-00c04fd430c8::cdr2.rso.nl::1\"]",
        "",
    )
}

/// The all-or-nothing failure of the same query: no rows, and the
/// diagnostic envelope (§11.4, CP-30).
fn failed_example() -> Result<String, Box<dyn Error>> {
    let text = incomplete_example()?;
    replaced(
        &text,
        "\"rows\": [\n    [\"node_1\", \"cdr1.rso.nl\", \"8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1\"]\n  ]",
        "\"rows\": []",
    )
}

/// A stub gateway answering `POST /v1/query/aql` with `status` and `body`,
/// and `OPTIONS /` with the §7a.2 example, only for the operator's token.
async fn gateway(status: u16, body: String) -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(options_example()?),
        )
        .mount(&gateway)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(&gateway)
        .await;
    Ok(gateway)
}

/// A console over `gateway` with one signed-in operator.
fn signed_in_console(gateway: &MockServer) -> Result<(axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// The form body of a query with `fields`.
fn form(fields: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(name, value);
    }
    form.finish()
}

/// A `POST` of `body` to the query page, as the console's own page sends
/// it, carrying `session`'s cookie when given.
fn run(body: String, session: Option<&SessionId>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post("/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("sec-fetch-site", "same-origin");
    if let Some(session) = session {
        request = request.header("cookie", format!("{COOKIE}={}", session.as_str()));
    }
    Ok(request.body(Body::from(body))?)
}

/// An AQL query naming the patient through the parameter `patient`.
fn patient_query() -> String {
    form(&[
        ("kind", "aql"),
        (
            "aql",
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient",
        ),
        ("parameter-name-1", "patient"),
        ("parameter-value-1", PATIENT),
    ])
}

/// Runs `body` as the signed-in operator and reads the page it answers
/// with.
async fn answered(
    service: &axum::Router,
    session: &SessionId,
    body: String,
) -> Result<String, Box<dyn Error>> {
    let (response, text) = send(service, run(body, Some(session))?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    assert!(!text.contains(OPERATOR_TOKEN), "{text}");
    Ok(text)
}

/// Whether the fan-out (F5) of `html` reports the endpoint `id` with
/// `status`.
fn reports(html: &str, id: &str, status: &str) -> bool {
    let Some((_, fanout)) = html.split_once(r#"class="fui-fanout-list""#) else {
        return false;
    };
    let Some((_, rest)) = fanout.split_once(&format!("<code>{id}<")) else {
        return false;
    };
    rest.split_once("</li>")
        .is_some_and(|(item, _)| item.contains(&format!("<code>{status}</code>")))
}

#[tokio::test]
async fn a_query_over_two_nodes_shows_the_rows_every_endpoint_and_complete()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    for column in ["endpoint_id", "system_id", "composition_id"] {
        assert!(html.contains(column), "{column}: {html}");
    }
    assert!(
        html.contains("8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1"),
        "{html}"
    );
    assert!(
        html.contains("6ba7b810-9dad-11d1-80b4-00c04fd430c8::cdr2.rso.nl::1"),
        "{html}"
    );
    assert!(reports(&html, "node_1", "active"), "{html}");
    assert!(reports(&html, "node_2", "active"), "{html}");
    assert!(reports(&html, "node_3", "excluded"), "{html}");
    assert!(html.contains("118") && html.contains("306"), "{html}");
    let answer = html
        .split_once("query-answer-title")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    assert!(!answer.contains(PATIENT), "{answer}");
    Ok(())
}

#[tokio::test]
async fn a_best_effort_answer_a_node_did_not_complete_is_flagged_incomplete()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, incomplete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    assert!(
        html.contains("8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1"),
        "{html}"
    );
    assert!(
        !html.contains("6ba7b810-9dad-11d1-80b4-00c04fd430c8::cdr2.rso.nl::1"),
        "{html}"
    );
    assert!(reports(&html, "node_2", "time-out"), "{html}");
    assert!(
        html.contains("no answer within the per-node budget"),
        "{html}"
    );
    Ok(())
}

#[tokio::test]
async fn an_all_or_nothing_failure_shows_its_status_and_every_endpoint()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(504, failed_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    assert!(html.contains("504"), "{html}");
    assert!(html.contains("The answer holds no rows."), "{html}");
    for (id, status) in [
        ("node_1", "active"),
        ("node_2", "time-out"),
        ("node_3", "excluded"),
    ] {
        assert!(reports(&html, id, status), "{id}: {html}");
    }
    Ok(())
}

#[tokio::test]
async fn a_refused_query_shows_its_status_and_stable_code() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(
        400,
        String::from(r#"{"message":"the query cannot be federated","code":"aql-not-federable"}"#),
    )
    .await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    assert!(html.contains(r#"role="alert""#), "{html}");
    assert!(html.contains("400"), "{html}");
    assert!(html.contains("aql-not-federable"), "{html}");
    Ok(())
}

#[tokio::test]
async fn a_result_set_with_no_federation_record_is_not_shown_as_an_answer()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(
        200,
        String::from(r#"{"columns":[{"name":"c"}],"rows":[["x"]],"meta":{}}"#),
    )
    .await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    assert!(html.contains(r#"role="alert""#), "{html}");
    assert!(
        !html.contains(">x<"),
        "the unread rows are not shown: {html}"
    );
    Ok(())
}

#[tokio::test]
async fn a_query_without_a_live_session_asks_the_gateway_nothing() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, _session) = signed_in_console(&gateway)?;
    for forged in [None, Some(SessionId::from_cookie("forged"))] {
        let (response, _text) = send(&service, run(patient_query(), forged.as_ref())?).await?;
        assert_eq!(StatusCode::SEE_OTHER, response.status());
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

#[tokio::test]
async fn the_federation_choices_go_to_the_gateway_as_its_headers() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let body = form(&[
        ("kind", "aql"),
        (
            "aql",
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
        ),
        ("endpoint", " node_1 "),
        ("endpoint", "node_2"),
        ("endpoint", ""),
        ("organisation", "Org A"),
        ("dedup", "version-identity"),
        ("partial", "on"),
        ("offset", "0"),
        ("fetch", "10"),
    ]);
    answered(&service, &session, body).await?;
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = asked
        .iter()
        .find(|request| request.method == http::Method::POST)
        .ok_or("the gateway was asked")?;
    let value = |name: &str| {
        request
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!("node_1, node_2", value("openEHR-federation-endpoint"));
    assert_eq!("Org A", value("openEHR-federation-organisation"));
    assert_eq!("version-identity", value("openEHR-federation-dedup"));
    assert_eq!("partial", value("openEHR-federation-completeness"));
    let sent = String::from_utf8(request.body.clone())?;
    assert!(sent.contains(r#""offset":0"#), "{sent}");
    assert!(sent.contains(r#""fetch":10"#), "{sent}");
    Ok(())
}

#[tokio::test]
async fn a_query_without_choices_sends_no_federation_header() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    answered(&service, &session, patient_query()).await?;
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = asked
        .iter()
        .find(|request| request.method == http::Method::POST)
        .ok_or("the gateway was asked")?;
    for name in openehr_federation::headers::ALL {
        assert!(request.headers.get(name).is_none(), "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn a_form_that_cannot_be_sent_is_refused_without_quoting_it() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    for body in [
        form(&[("kind", "aql"), ("aql", " ")]),
        form(&[("kind", "stored"), ("name", "")]),
        form(&[("kind", "neither")]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("parameter-name-1", PATIENT),
            ("parameter-value-1", "x"),
        ]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("parameter-name-1", "patient"),
            ("parameter-value-1", PATIENT),
            ("parameter-name-2", "patient"),
            ("parameter-value-2", PATIENT),
        ]),
        form(&[("kind", "aql"), ("aql", "SELECT 1"), ("offset", "-1")]),
        form(&[("kind", "aql"), ("aql", "SELECT 1"), ("fetch", PATIENT)]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("organisation", "Org\u{7}A"),
        ]),
    ] {
        let html = answered(&service, &session, body.clone()).await?;
        assert!(html.contains(r#"role="alert""#), "{body}: {html}");
        let answer = html
            .split_once("query-answer-title")
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        assert!(!answer.contains(PATIENT), "{body}: {answer}");
    }
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

// The identifier an operator enters travels in the console's request body
// and the gateway's, and reaches no URL and no log line.
#[tokio::test]
async fn an_identifier_entered_in_the_console_reaches_no_url_and_no_log()
-> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let _logging = captured.logging();

    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    answered(&service, &session, patient_query()).await?;

    let refusing = self::gateway(
        400,
        String::from(r#"{"message":"refused","code":"aql-invalid"}"#),
    )
    .await?;
    let (refused_service, refused_session) = signed_in_console(&refusing)?;
    answered(&refused_service, &refused_session, patient_query()).await?;

    // A query string is never read as a query: a GET draws the empty form.
    let in_url = format!(
        "/query?kind=aql&aql=SELECT%201&parameter-name-1=patient&parameter-value-1={PATIENT}"
    );
    let (_response, page) = send(&service, get_as(&in_url, &session)?).await?;
    assert!(!page.contains(PATIENT), "{page}");

    // The page posts its form, so the browser keeps no field in its history.
    let (_response, page) = send(&service, get_as("/query", &session)?).await?;
    assert!(page.contains(r#"method="post""#), "{page}");
    assert!(page.contains(r#"action="/query""#), "{page}");

    let mut asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    asked.extend(
        refusing
            .received_requests()
            .await
            .ok_or("the stub records requests")?,
    );
    for request in &asked {
        assert!(!request.url.as_str().contains(PATIENT), "{}", request.url);
    }
    asked.retain(|request| request.method == http::Method::POST);
    assert_eq!(2, asked.len());
    for request in &asked {
        let body = String::from_utf8(request.body.clone())?;
        assert!(
            body.contains(&format!(r#""query_parameters":{{"patient":"{PATIENT}"}}"#)),
            "{body}"
        );
    }
    let logged = captured.text();
    assert!(
        logged.contains(r#"the gateway refused a query status=400 Bad Request code="aql-invalid""#),
        "the capture saw the refusal: {logged}"
    );
    assert!(!logged.contains("refused a view"), "{logged}");
    for named in [PATIENT, "SELECT c/uid/value", "$patient", OPERATOR_TOKEN] {
        assert!(!logged.contains(named), "{named}: {logged}");
    }
    Ok(())
}

#[tokio::test]
async fn the_query_page_offers_what_the_gateway_declares() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, page) = send(&service, get_as("/query", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{page}");
    assert!(
        page.contains("<title>Query console · FerroFED Console</title>"),
        "{page}"
    );
    for endpoint in crate::stub::example_endpoints()? {
        assert!(
            page.contains(&format!(r#"name="endpoint" value="{endpoint}""#)),
            "each declared endpoint is a checkbox: {endpoint}: {page}"
        );
    }
    assert!(
        page.contains(r#"<option value="version-identity""#),
        "{page}"
    );
    assert!(
        page.contains(r#"id="query-partial" name="partial""#),
        "{page}"
    );
    assert!(page.contains(r#"for="parameter-name-1""#), "{page}");
    assert!(page.contains(r#"for="parameter-value-1""#), "{page}");
    assert!(
        page.contains("Run a query to see its answer here."),
        "{page}"
    );
    assert!(!page.contains(OPERATOR_TOKEN), "{page}");
    Ok(())
}

/// A console over `gateway` that knows its own origin from `[oidc]`, with
/// one signed-in operator.
fn console_with_origin(gateway: &MockServer) -> Result<(axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n{}",
        gateway.uri(),
        crate::support::WITH_OIDC
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// A `POST` of the query carrying `session`'s cookie and only the headers
/// `from` names, as a page of some origin would send it.
fn run_from(session: &SessionId, from: &[(&str, &str)]) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post("/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("cookie", format!("{COOKIE}={}", session.as_str()));
    for (name, value) in from {
        request = request.header(*name, *value);
    }
    Ok(request.body(Body::from(patient_query()))?)
}

// A cross-site page could otherwise spend the operator's gateway credential
// on a query of its choosing, so the run is taken from the console alone.
#[tokio::test]
async fn a_query_from_another_origin_or_from_nowhere_is_refused() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = console_with_origin(&gateway)?;
    for from in [
        vec![("sec-fetch-site", "cross-site")],
        vec![("sec-fetch-site", "same-site")],
        vec![("origin", "https://attacker.example.net")],
        vec![("origin", "null")],
        vec![("referer", "https://attacker.example.net/page")],
        vec![("referer", "not a url")],
        Vec::new(),
    ] {
        let (response, text) = send(&service, run_from(&session, &from)?).await?;
        assert_eq!(StatusCode::FORBIDDEN, response.status(), "{from:?}: {text}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

#[tokio::test]
async fn a_query_from_the_consoles_own_origin_runs() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = console_with_origin(&gateway)?;
    for from in [
        vec![("sec-fetch-site", "same-origin")],
        vec![("origin", "http://127.0.0.1:3000")],
        vec![("referer", "http://127.0.0.1:3000/query")],
    ] {
        let (response, text) = send(&service, run_from(&session, &from)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{from:?}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn a_get_never_runs_a_query() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = console_with_origin(&gateway)?;
    let request = Request::get("/query?kind=aql&aql=SELECT%201")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .header("sec-fetch-site", "same-origin")
        .body(Body::empty())?;
    let (response, _text) = send(&service, request).await?;
    assert_eq!(StatusCode::OK, response.status());
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
async fn the_query_page_needs_a_signed_in_session() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, _session) = signed_in_console(&gateway)?;
    let (response, _page) = send(&service, crate::support::get("/query")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/login", crate::support::header(&response, "location"));
    Ok(())
}

// An answer carries what the operator entered beside data an attacker would
// want, so it is never compressed (BREACH) and never cached.
#[tokio::test]
async fn an_answer_is_neither_compressed_nor_cached() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let mut request = run(patient_query(), Some(&session))?;
    request.headers_mut().insert(
        "accept-encoding",
        http::HeaderValue::from_static("br, gzip"),
    );
    let (response, _text) = send(&service, request).await?;
    assert_eq!("", crate::support::header(&response, "content-encoding"));
    assert_eq!(
        "no-store",
        crate::support::header(&response, "cache-control")
    );
    Ok(())
}

// A node's hostile values reach the page as text, never as markup.
#[tokio::test]
async fn a_nodes_hostile_values_reach_the_page_escaped() -> Result<(), Box<dyn Error>> {
    let text = incomplete_example()?;
    let text = replaced(
        &text,
        "\"8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1\"",
        "\"<script>alert(1)</script>\"",
    )?;
    let text = replaced(
        &text,
        "no answer within the per-node budget",
        "<img src=x onerror=alert(2)>",
    )?;
    let gateway = gateway(200, text).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    for raw in ["<script>alert(1)", "<img src=x"] {
        assert!(!html.contains(raw), "{raw}: {html}");
    }
    assert!(
        html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "{html}"
    );
    assert!(
        html.contains("&lt;img src=x onerror=alert(2)&gt;"),
        "{html}"
    );
    Ok(())
}

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

// Every value a node wrote reaches the answer as text: the cells, the column
// name and path, an endpoint's error and the raw answer.
#[tokio::test]
async fn every_value_a_node_wrote_reaches_the_answer_escaped() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, HOSTILE.to_owned()).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let html = answered(&service, &session, patient_query()).await?;
    let answer = html
        .split_once("query-answer-title")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    for escaped in [
        "&lt;script&gt;alert(1)&lt;/script&gt;",
        "&lt;img src=x onerror=alert(1)&gt;",
        "&lt;/th&gt;&lt;script&gt;x&lt;/script&gt;",
        "&lt;b&gt;late&lt;/b&gt; &amp; gone",
    ] {
        assert!(answer.contains(escaped), "{escaped}: {answer}");
    }
    for raw in [
        "<script>",
        "<img",
        "onmouseover=\"",
        "<b>",
        "<i>",
        "</th><script",
    ] {
        assert!(!answer.contains(raw), "{raw}: {answer}");
    }
    Ok(())
}

// The form posts with no WebAssembly: its Run is a plain submit button,
// never one that waits for the page to hydrate.
#[tokio::test]
async fn the_run_works_before_the_page_hydrates() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, result_set_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (_response, page) = send(&service, get_as("/query", &session)?).await?;
    let form = page
        .split_once(r#"<form id="query-form""#)
        .and_then(|(_, rest)| rest.split_once("</form>"))
        .map(|(form, _)| form)
        .ok_or("the query form")?;
    assert!(form.contains(r#"type="submit""#), "{form}");
    assert!(!form.contains("disabled"), "{form}");
    Ok(())
}
