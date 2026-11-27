// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's tutorial, "Your first federated query", run as written.
//!
//! Each `http` block after a `<!-- book-request: NAME ... -->` marker is a
//! request the reader sends, in page order, to a gateway over four mock
//! member nodes with the quickstart's registry document and synthetic
//! patients. A marker states the status it expects (`expect=`), the JSON
//! pointers the answer holds (`holds=/pointer=value`), the number of rows
//! (`rows=`) and the status of a member (`status=<endpoint>:<status>`). A
//! page edit that changes what the gateway answers, or a gateway change that
//! breaks a documented request, fails here. No specification governs the
//! marker format: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::settings_with_room;
use crate::support::call;

type TestResult = Result<(), Box<dyn Error>>;

/// The tutorial page.
const PAGE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/tutorials/first-federated-query.md"
));

/// The quickstart's registry document, whose four endpoints the page names.
const REGISTRY: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docker/quickstart/registry.toml"
));

/// The quickstart's synthetic patients, one EHR per line.
const PATIENTS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docker/quickstart/patients.tsv"
));

/// The path under each node's base URL that answers a query.
const NODE_QUERY: &str = "/ferroehr/rest/openehr/v1/query/aql";

/// The fewest requests the page holds; a page that loses its steps fails.
const MIN_REQUESTS: usize = 6;

/// A request the reader sends and what its answer must hold.
struct Step {
    /// The marker's name.
    name: String,
    /// The status the answer carries.
    expect: Option<StatusCode>,
    /// The JSON pointers the answer holds, each with its value as text.
    holds: Vec<(String, String)>,
    /// How many rows the result set holds.
    rows: Option<usize>,
    /// The status `meta.federation` reports for an endpoint.
    statuses: Vec<(String, String)>,
    /// The `http` block: the request line, the headers and the body.
    block: String,
}

/// Returns the content of the fenced block that opens on the next line of
/// `lines`, leaving `lines` after its closing fence.
fn fenced<'a>(
    lines: &mut impl Iterator<Item = &'a str>,
    marker: &str,
) -> Result<String, Box<dyn Error>> {
    let open = lines
        .next()
        .ok_or_else(|| format!("{marker} is followed by no block"))?;
    if open != "```http" {
        return Err(format!("{marker} is followed by `{open}`, not an http block").into());
    }
    let mut content = String::new();
    for line in lines.by_ref() {
        if line.starts_with("```") {
            return Ok(content);
        }
        content.push_str(line);
        content.push('\n');
    }
    Err(format!("the block after {marker} never closes").into())
}

/// Reads one request marker's attributes.
fn step(marker: &str, block: String) -> Result<Step, Box<dyn Error>> {
    let mut words = marker.split_whitespace();
    let name = words.next().ok_or("a request marker names its request")?;
    let mut parsed = Step {
        name: name.to_owned(),
        expect: None,
        holds: Vec::new(),
        rows: None,
        statuses: Vec::new(),
        block,
    };
    for word in words {
        let (key, value) = word
            .split_once('=')
            .ok_or_else(|| format!("{name}: `{word}` is no key=value"))?;
        match key {
            "expect" => parsed.expect = Some(StatusCode::from_bytes(value.as_bytes())?),
            "rows" => parsed.rows = Some(value.parse()?),
            "holds" => {
                let (pointer, wanted) = value
                    .split_once('=')
                    .ok_or_else(|| format!("{name}: `{value}` is no /pointer=value"))?;
                parsed.holds.push((pointer.to_owned(), wanted.to_owned()));
            }
            "status" => {
                let (endpoint, status) = value
                    .split_once(':')
                    .ok_or_else(|| format!("{name}: `{value}` is no endpoint:status"))?;
                parsed
                    .statuses
                    .push((endpoint.to_owned(), status.to_owned()));
            }
            _ => return Err(format!("{name}: unknown attribute `{key}`").into()),
        }
    }
    if parsed.expect.is_none() {
        return Err(format!("{name} states no expected status").into());
    }
    Ok(parsed)
}

/// Reads the page's requests, in page order.
fn steps(page: &str) -> Result<Vec<Step>, Box<dyn Error>> {
    let mut steps = Vec::new();
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        if let Some(marker) = line
            .trim()
            .strip_prefix("<!-- book-request:")
            .and_then(|rest| rest.strip_suffix("-->"))
        {
            let block = fenced(&mut lines, line)?;
            steps.push(step(marker, block)?);
        }
    }
    Ok(steps)
}

/// Builds the request of an `http` block. A header whose value names
/// `$TOKEN` is left out, so the suite's own token stands for the reader's.
fn request(text: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let (head, body) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut head_lines = head.lines();
    let request_line = head_lines
        .next()
        .ok_or("an http block opens with its request line")?;
    let (verb, target) = request_line
        .split_once(' ')
        .ok_or_else(|| format!("`{request_line}` is no request line"))?;
    let mut builder = Request::builder().method(verb).uri(target.trim());
    for line in head_lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| format!("`{line}` is no header"))?;
        if !value.contains("$TOKEN") {
            builder = builder.header(name.trim(), value.trim());
        }
    }
    Ok(builder.body(Body::from(body.trim_end().to_owned()))?)
}

/// The text a JSON value compares as: a string as itself, anything else as
/// its JSON form.
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: a JSON pointer of the page reads the answer as values"
)]
fn text_of(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned)
}

/// A node answering every query with one composition its `system_id`
/// created.
async fn node(system_id: &str) -> Server {
    let server = Server::start().await;
    let answer = format!(
        r##"{{"q":"node","columns":[{{"name":"#0","path":"c/uid/value"}}],"rows":[["8d9c1f0e-0b6a-4c3e-9d1f-2a7b5e4c6d80::{system_id}::1"]]}}"##
    );
    Mock::given(method("POST"))
        .and(path(NODE_QUERY))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(answer.into_bytes(), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// The `[[testkit.crossref]]` rows of every EHR the quickstart's patients
/// file names.
fn crossref() -> Result<String, Box<dyn Error>> {
    let mut rows = String::new();
    for line in PATIENTS.lines().filter(|line| !line.starts_with('#')) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [member, ehr_id, namespace, value, _domain] = fields.as_slice() else {
            return Err(format!("`{line}` is no patients line").into());
        };
        write!(
            rows,
            "\n[[testkit.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
        )?;
    }
    Ok(rows)
}

/// The gateway over the quickstart's registry, with each node at its mock.
fn gateway(dir: &std::path::Path, nodes: &[(&str, &Server)]) -> Result<Router, Box<dyn Error>> {
    let mut registry = REGISTRY.to_owned();
    for (port, server) in nodes {
        let quickstart = format!("http://localhost:{port}");
        assert!(
            registry.contains(&quickstart),
            "the quickstart registry names {quickstart}"
        );
        registry = registry.replace(&quickstart, &server.uri());
    }
    let document = dir.join("registry.toml");
    std::fs::write(&document, registry)?;
    let document = toml::Value::String(document.display().to_string());
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nid = \"quickstart\"\nnode_selection = \"ask-all\"\nper_node_timeout = '2s'\noverall_timeout = '3s'\n{}",
        crossref()?
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let state = Arc::new(AppState::with_federation(federation));
    Ok(ferrofed_server::router(state, &settings_with_room()))
}

/// Checks the answer to `step` against its marker.
#[expect(
    clippy::disallowed_types,
    reason = "the test seam: a JSON pointer of the page reads the answer as values"
)]
fn check(step: &Step, status: StatusCode, body: &str) -> TestResult {
    assert_eq!(
        step.expect,
        Some(status),
        "{}: the status, answer {body}",
        step.name
    );
    let answer: serde_json::Value = serde_json::from_str(body)?;
    for (pointer, wanted) in &step.holds {
        let found = answer
            .pointer(pointer)
            .ok_or_else(|| format!("{}: the answer holds no {pointer}: {body}", step.name))?;
        assert_eq!(wanted, &text_of(found), "{}: {pointer}", step.name);
    }
    if let Some(rows) = step.rows {
        let held = answer
            .pointer("/rows")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("{}: the answer holds no rows: {body}", step.name))?;
        assert_eq!(rows, held.len(), "{}: the rows", step.name);
    }
    let endpoints = answer
        .pointer("/meta/federation/endpoints")
        .and_then(serde_json::Value::as_array);
    for (endpoint, wanted) in &step.statuses {
        let found = endpoints
            .and_then(|all| {
                all.iter()
                    .find(|one| one.get("id").and_then(serde_json::Value::as_str) == Some(endpoint))
            })
            .and_then(|one| one.get("status"))
            .ok_or_else(|| format!("{}: no status for {endpoint}: {body}", step.name))?;
        assert_eq!(wanted, &text_of(found), "{}: {endpoint}", step.name);
    }
    Ok(())
}

#[tokio::test]
async fn every_request_of_the_tutorial_answers_as_the_page_says() -> TestResult {
    let steps = steps(PAGE)?;
    assert!(
        steps.len() >= MIN_REQUESTS,
        "the tutorial holds {} requests, fewer than {MIN_REQUESTS}",
        steps.len()
    );
    let a = node("node-a.quickstart.local").await;
    let b = node("node-b.quickstart.local").await;
    let c = node("node-c.quickstart.local").await;
    let d = node("node-d.quickstart.local").await;
    let dir = tempfile::tempdir()?;
    let app = gateway(
        dir.path(),
        &[("8081", &a), ("8082", &b), ("8083", &c), ("8084", &d)],
    )?;
    for step in &steps {
        let mut sent = request(&step.block)?;
        if !sent.headers().contains_key(header::ACCEPT) {
            sent.headers_mut()
                .insert(header::ACCEPT, "application/json".parse()?);
        }
        let (status, body) = call(app.clone(), sent).await?;
        check(step, status, &body)?;
    }
    Ok(())
}
