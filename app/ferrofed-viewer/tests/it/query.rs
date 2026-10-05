// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query console against a stub gateway: a query over two nodes shows
//! its rows, every endpoint's status and `complete` (§9.4, §11.1, §11.4,
//! N37), a refusal shows its stable code, and an identifier entered in the
//! console reaches no URL and no log (N33).

use std::error::Error;
use std::io::Write;
use std::sync::{Arc, Mutex, PoisonError};

use axum::body::Body;
use ferrofed_viewer::query::model::RenderedAnswer;
use ferrofed_viewer::session::{COOKIE, SessionId};
use http::{Request, StatusCode};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, get_as, send, signed_in};

/// The vendored specification page that carries the §9.4 example.
const RESULT_SET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/result-set.adoc"
);

/// A synthetic patient identifier, as an operator would type it.
pub(crate) const PATIENT: &str = "synthetic-patient-48151623";

/// The one `[source,json]` example of the §9.4 page: a complete answer over
/// two nodes, with a third reported as excluded.
pub(crate) fn complete_example() -> Result<String, Box<dyn Error>> {
    example(RESULT_SET)
}

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
    let text = complete_example()?;
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

/// The vendored specification page that carries the §7a.2 example.
const REST_FACADE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/rest-facade.adoc"
);

/// The first `[source,json]` example of the specification page `page`.
fn example(page: &str) -> Result<String, Box<dyn Error>> {
    let page = std::fs::read_to_string(page)?;
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "[source,json]" && lines.next().map(str::trim) == Some("----") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "----").collect();
            return Ok(body.join("\n"));
        }
    }
    Err("the page carries no JSON example".into())
}

/// A stub gateway answering `POST {base}/v1/query/aql` with `status` and
/// `body`, and `OPTIONS {base}/` with the §7a.2 example, only for the
/// operator's own token.
pub(crate) async fn gateway(status: u16, body: String) -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    Mock::given(method("OPTIONS"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(example(REST_FACADE)?),
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
pub(crate) fn signed_in_console(
    gateway: &MockServer,
) -> Result<(axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{}\"\n\n[session]\nsecure_cookie = false\n",
        gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((service, session))
}

/// The form body of a query console submission with `fields`.
pub(crate) fn form(fields: &[(&str, &str)]) -> String {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(&format!("form[{name}]"), value);
    }
    form.finish()
}

/// A `POST` of the query server function with `body`, as the console's own
/// page sends it, carrying `session`'s cookie when given.
pub(crate) fn run(
    body: String,
    session: Option<&SessionId>,
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post("/api/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
        .header("sec-fetch-site", "same-origin");
    if let Some(session) = session {
        request = request.header("cookie", format!("{COOKIE}={}", session.as_str()));
    }
    Ok(request.body(Body::from(body))?)
}

/// An AQL query naming the patient through the parameter `patient`.
pub(crate) fn patient_query() -> String {
    form(&[
        ("kind", "aql"),
        (
            "aql",
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_status/subject/external_ref/id/value = $patient",
        ),
        ("parameters", &format!("patient={PATIENT}")),
    ])
}

/// Runs `body` as the signed-in operator and reads the answer.
pub(crate) async fn answered(
    service: &axum::Router,
    session: &SessionId,
    body: String,
) -> Result<RenderedAnswer, Box<dyn Error>> {
    let (response, text) = send(service, run(body, Some(session))?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{text}");
    Ok(serde_json::from_str(&text)?)
}

/// Whether `html` holds the endpoint row of `id` with `status`.
fn reports(html: &str, id: &str, status: &str) -> bool {
    html.contains(&format!(r#"<th scope="row">{id}</th><td>{status}</td>"#))
}

#[tokio::test]
async fn a_query_over_two_nodes_shows_the_rows_every_endpoint_and_complete()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let answer = answered(&service, &session, patient_query()).await?;
    assert_eq!(200, answer.status);
    assert!(answer.complete);
    let html = &answer.html;
    assert!(html.contains("Every node in scope answered."), "{html}");
    for column in ["endpoint_id", "system_id", "composition_id"] {
        assert!(
            html.contains(&format!(">{column}</th>")),
            "{column}: {html}"
        );
    }
    assert!(html.contains("2 rows"), "{html}");
    assert!(
        html.contains("<td>8849182a-1d4b-4e3d-a3f3-f303d2f4f34b::cdr1.rso.nl::1</td>"),
        "{html}"
    );
    assert!(reports(html, "node_1", "active"), "{html}");
    assert!(reports(html, "node_2", "active"), "{html}");
    assert!(reports(html, "node_3", "excluded"), "{html}");
    assert!(html.contains("118 ms") && html.contains("306 ms"), "{html}");
    Ok(())
}

#[tokio::test]
async fn a_best_effort_answer_a_node_did_not_complete_is_flagged_incomplete()
-> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, incomplete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let answer = answered(&service, &session, patient_query()).await?;
    assert_eq!(200, answer.status);
    assert!(!answer.complete, "{answer:?}");
    let html = &answer.html;
    assert!(html.contains("Incomplete answer."), "{html}");
    assert!(html.contains("<caption>1 row</caption>"), "{html}");
    assert!(reports(html, "node_2", "time-out"), "{html}");
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
    let answer = answered(&service, &session, patient_query()).await?;
    assert_eq!(504, answer.status);
    assert!(!answer.complete);
    let html = &answer.html;
    assert!(
        html.contains("The gateway failed the query: 504."),
        "{html}"
    );
    assert!(html.contains("Incomplete answer."), "{html}");
    assert!(html.contains("No rows."), "{html}");
    for (id, status) in [
        ("node_1", "active"),
        ("node_2", "time-out"),
        ("node_3", "excluded"),
    ] {
        assert!(reports(html, id, status), "{id}: {html}");
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
    let (response, text) = send(&service, run(patient_query(), Some(&session))?).await?;
    assert_ne!(StatusCode::OK, response.status(), "{text}");
    assert!(text.contains("Refused"), "{text}");
    assert!(text.contains("400"), "{text}");
    assert!(text.contains("aql-not-federable"), "{text}");
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
    let (response, text) = send(&service, run(patient_query(), Some(&session))?).await?;
    assert_ne!(StatusCode::OK, response.status(), "{text}");
    assert!(text.contains("Unreadable"), "{text}");
    Ok(())
}

#[tokio::test]
async fn the_query_functions_refuse_a_caller_without_a_live_session() -> Result<(), Box<dyn Error>>
{
    let gateway = gateway(200, complete_example()?).await?;
    let (service, _session) = signed_in_console(&gateway)?;
    for forged in [None, Some(SessionId::from_cookie("forged"))] {
        let (_response, text) = send(&service, run(patient_query(), forged.as_ref())?).await?;
        assert!(text.contains("SignedOut"), "{text}");
        let mut request = Request::post("/api/query-options")
            .header("accept", "application/json")
            .header("sec-fetch-site", "same-origin");
        if let Some(forged) = &forged {
            request = request.header("cookie", format!("{COOKIE}={}", forged.as_str()));
        }
        let (_response, text) = send(&service, request.body(Body::empty())?).await?;
        assert!(text.contains("SignedOut"), "{text}");
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
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let body = form(&[
        ("kind", "aql"),
        (
            "aql",
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
        ),
        ("endpoints", " node_1 ,node_2,, "),
        ("organisation", "Org A"),
        ("dedup", "version-identity"),
        ("partial", "partial"),
        ("offset", "0"),
        ("fetch", "10"),
    ]);
    answered(&service, &session, body).await?;
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = asked.first().ok_or("the gateway was asked")?;
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
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    answered(&service, &session, patient_query()).await?;
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    let request = asked.first().ok_or("the gateway was asked")?;
    for name in openehr_federation::headers::ALL {
        assert!(request.headers.get(name).is_none(), "{name}");
    }
    Ok(())
}

#[tokio::test]
async fn a_form_that_cannot_be_sent_is_refused_without_quoting_it() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    for body in [
        form(&[("kind", "aql"), ("aql", " ")]),
        form(&[("kind", "stored"), ("name", "")]),
        form(&[("kind", "neither")]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("parameters", PATIENT),
        ]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            (
                "parameters",
                &format!("patient={PATIENT}\npatient={PATIENT}"),
            ),
        ]),
        form(&[("kind", "aql"), ("aql", "SELECT 1"), ("offset", "-1")]),
        form(&[("kind", "aql"), ("aql", "SELECT 1"), ("fetch", PATIENT)]),
        form(&[
            ("kind", "aql"),
            ("aql", "SELECT 1"),
            ("organisation", "Org\u{7}A"),
        ]),
    ] {
        let (response, text) = send(&service, run(body.clone(), Some(&session))?).await?;
        assert_ne!(StatusCode::OK, response.status(), "{body}");
        assert!(text.contains("Invalid"), "{body}: {text}");
        assert!(!text.contains(PATIENT), "{body}: {text}");
    }
    let asked = gateway
        .received_requests()
        .await
        .ok_or("the stub records requests")?;
    assert!(asked.is_empty(), "the gateway was asked {}", asked.len());
    Ok(())
}

/// Every line the console logs while it is captured.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Captured {
    /// What was logged.
    fn text(&self) -> String {
        let bytes = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        String::from_utf8_lossy(&bytes).into_owned()
    }
}

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

// The identifier an operator enters travels in the console's request body
// and the gateway's, and reaches no URL, no log line and no answer.
#[tokio::test]
async fn an_identifier_entered_in_the_console_reaches_no_url_and_no_log()
-> Result<(), Box<dyn Error>> {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _logging = tracing::subscriber::set_default(subscriber);

    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (_response, answer) = send(&service, run(patient_query(), Some(&session))?).await?;
    assert!(!answer.contains(PATIENT), "{answer}");

    let refusing = self::gateway(
        400,
        String::from(r#"{"message":"refused","code":"aql-invalid"}"#),
    )
    .await?;
    let (refused_service, refused_session) = signed_in_console(&refusing)?;
    let (_response, refusal) = send(
        &refused_service,
        run(patient_query(), Some(&refused_session))?,
    )
    .await?;
    assert!(!refusal.contains(PATIENT), "{refusal}");

    // A URL is never read: the server function takes no query string.
    let in_url = format!("/api/query?form%5Bkind%5D=aql&form%5Bparameters%5D=patient%3D{PATIENT}");
    let (response, _text) = send(&service, get_as(&in_url, &session)?).await?;
    assert_ne!(StatusCode::OK, response.status());

    // The page posts its form, so the browser keeps no field in its history.
    let (_response, page) = send(&service, get_as("/query", &session)?).await?;
    assert!(page.contains(r#"method="post""#), "{page}");
    assert!(page.contains(r#"action="/api/query""#), "{page}");
    assert!(!page.contains(PATIENT));

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
        logged.contains("the gateway refused a view"),
        "the capture saw the refusal: {logged}"
    );
    assert!(!logged.contains(PATIENT), "{logged}");
    Ok(())
}

#[tokio::test]
async fn the_query_page_offers_what_the_gateway_declares() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = signed_in_console(&gateway)?;
    let (response, page) = send(&service, get_as("/query", &session)?).await?;
    assert_eq!(StatusCode::OK, response.status(), "{page}");
    assert!(
        page.contains("<title>Query · FerroFED operator console</title>"),
        "{page}"
    );
    assert!(page.contains("Declared: node_1, node_2"), "{page}");
    assert!(page.contains("Declared: Org A, Org B"), "{page}");
    assert!(
        page.contains(r#"<option value="version-identity">"#),
        "{page}"
    );
    assert!(page.contains(r#"name="form[partial]""#), "{page}");
    assert!(
        !page.contains("which this gateway does not offer"),
        "{page}"
    );
    assert!(page.contains(r#"<label for="query-parameters">"#), "{page}");
    assert!(page.contains("No query has run yet."), "{page}");
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

/// A `POST` of the query function carrying `session`'s cookie and only the
/// headers `from` names, as a page of some origin would send it.
fn run_from(session: &SessionId, from: &[(&str, &str)]) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::post("/api/query")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json")
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
    let gateway = gateway(200, complete_example()?).await?;
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
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = console_with_origin(&gateway)?;
    for from in [
        vec![("sec-fetch-site", "same-origin")],
        vec![("origin", "https://console.example.org")],
        vec![("referer", "https://console.example.org/query")],
    ] {
        let (response, text) = send(&service, run_from(&session, &from)?).await?;
        assert_eq!(StatusCode::OK, response.status(), "{from:?}: {text}");
    }
    Ok(())
}

#[tokio::test]
async fn a_get_never_runs_a_query() -> Result<(), Box<dyn Error>> {
    let gateway = gateway(200, complete_example()?).await?;
    let (service, session) = console_with_origin(&gateway)?;
    let request = Request::get("/api/query?form%5Bkind%5D=aql&form%5Baql%5D=SELECT%201")
        .header("cookie", format!("{COOKIE}={}", session.as_str()))
        .header("sec-fetch-site", "same-origin")
        .body(Body::empty())?;
    let (response, _text) = send(&service, request).await?;
    assert_ne!(StatusCode::OK, response.status());
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
    let gateway = gateway(200, complete_example()?).await?;
    let (service, _session) = signed_in_console(&gateway)?;
    let (response, _page) = send(&service, crate::support::get("/query")?).await?;
    assert_eq!(StatusCode::SEE_OTHER, response.status());
    assert_eq!("/login", crate::support::header(&response, "location"));
    Ok(())
}
