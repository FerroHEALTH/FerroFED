// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A stub gateway the page tests read: the self-description of the
//! specification's §7a.2 example, the health of every member and service,
//! the deployment, the incidents, the routing table and the stored queries,
//! with an admin listener on the same server holding revision 2 saved over
//! the running revision 1. Every answer is synthetic.

use std::error::Error;

use axum::body::Body;
use ferrofed_console::server::ConsoleState;
use ferrofed_console::session::SessionId;
use http::Request;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::support::{OPERATOR_TOKEN, console, signed_in};

/// The vendored specification page that carries the §7a.2 example.
const REST_FACADE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/rest-facade.adoc"
);

/// The vendored specification page that carries the §9.4 example.
const RESULT_SET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/pages/result-set.adoc"
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

/// The one `[source,json]` example of the §7a.2 page.
pub(crate) fn options_example() -> Result<String, Box<dyn Error>> {
    example(REST_FACADE)
}

/// The one `[source,json]` example of the §9.4 page: a complete answer over
/// two nodes, with a third reported as excluded.
pub(crate) fn result_set_example() -> Result<String, Box<dyn Error>> {
    example(RESULT_SET)
}

/// The `endpoint_id`s the §7a.2 example declares.
pub(crate) fn example_endpoints() -> Result<Vec<String>, Box<dyn Error>> {
    let body: openehr_federation::options::OptionsRoot = serde_json::from_str(&options_example()?)?;
    Ok(body
        .endpoints
        .iter()
        .map(|endpoint| endpoint.id.to_string())
        .collect())
}

/// Answers `verb` of `route` with `status` and the JSON `body`, only for
/// the operator's own token.
pub(crate) async fn answers(
    gateway: &MockServer,
    (verb, route): (&str, &str),
    status: u16,
    body: &str,
) {
    Mock::given(method(verb))
        .and(path(route))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OPERATOR_TOKEN}").as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(status)
                .insert_header("content-type", "application/json")
                .set_body_string(body),
        )
        .mount(gateway)
        .await;
}

/// The running revision 1, applied by the file, and revision 2 saved.
pub(crate) const STANDING: &str = r#"{"ui":"managed","head":2,"current":{"revision":1,"previous":null,
    "outcome":"applied","needs_restart":[],"applied_by":{"kind":"file"},
    "applied_at":"2026-10-15T08:00:00Z"},"running":1,"in_step":true,"refused":null}"#;

/// Member A the file provides and member C the console holds.
pub(crate) const RESOURCES: &str = r#"{"running":1,"head":2,"members":[
    {"node":"node-a","organisation":"org-a","system_id":"cdr-a.example.org","endpoints":["node-a-pub"],"owner":"file"},
    {"node":"node-c","organisation":"org-c","system_id":"cdr-c.example.org","endpoints":["node-c-pub"],
     "owner":"console","state":"pending",
     "member":{"organisation":{"id":"org-c","name":"Synthetic Clinic C"},
               "node":{"id":"node-c","system_id":"cdr-c.example.org"},
               "endpoints":[{"id":"node-c-pub","url":"https://cdr-c.example.org/openehr",
                             "connection_type":"openehr-rest-query"}]}}],
    "stored_query_registry":"writable",
    "stored_queries":[{"name":"org.example::compositions","version":"1.0.0","owner":"store"}]}"#;

/// The history: revision 2 by an operator, then the file.
pub(crate) const HISTORY: &str = r#"{"items":[
    {"id":2,"base":1,"origin":"operator","restores":null,
     "author":{"kind":"operator","issuer":"https://idp.example.org","subject":"op-1"},
     "at":"2026-10-15T09:00:00Z","reason":"add C","toml":"ignored = 1"},
    {"id":1,"base":null,"origin":"file","restores":null,"author":{"kind":"file"},
     "at":"2026-10-15T08:00:00Z","reason":"the configuration file was loaded","toml":""}],
    "offset":0,"total":2}"#;

/// What revision 2 changes against revision 1.
pub(crate) const DIFF: &str = r#"{"from":1,"to":2,"changes":[
    {"key":"registry.members[0].node.id","change":"added","from":null,"to":"\"node-c\""}]}"#;

/// A stub gateway answering every surface the pages read, with an admin
/// listener on the same server.
pub(crate) async fn gateway() -> Result<MockServer, Box<dyn Error>> {
    let gateway = MockServer::start().await;
    answers(&gateway, ("OPTIONS", "/"), 200, &options_example()?).await;
    let endpoints = example_endpoints()?
        .iter()
        .map(|id| format!("\"{id}\":\"up\""))
        .collect::<Vec<_>>()
        .join(",");
    answers(
        &gateway,
        ("GET", "/operator/dependencies"),
        200,
        &format!(r#"{{"endpoints":{{{endpoints}}},"resolver":"failing","consent":"up","replica":"ferrofed-0"}}"#),
    )
    .await;
    answers(
        &gateway,
        ("GET", "/operator/deployment"),
        200,
        r#"{"mode":"production","product":"FerroFED","version":"0.0.12","replica":"ferrofed-0"}"#,
    )
    .await;
    answers(
        &gateway,
        ("GET", "/operator/incidents"),
        200,
        r#"{"counts":{"EhrIdCollision":2},
           "recent":[{"kind":"EhrIdCollision","at":"2026-10-05T08:00:00Z",
           "description":"ehr_id 7d44b88c-4199-4bad-97dc-d78268e01398 is claimed by two endpoints",
           "creating_system_id":null,"ehr_id":"7d44b88c-4199-4bad-97dc-d78268e01398","detection":"ask-all",
           "endpoints":["node-a-pub","node-b-pub"],"nodes":["node-a","node-b"]},
           {"kind":"EhrIdCollision","at":"2026-10-04T08:00:00Z",
           "description":"ehr_id 1d44b88c-4199-4bad-97dc-d78268e01398 is claimed by two endpoints",
           "creating_system_id":null,"ehr_id":"1d44b88c-4199-4bad-97dc-d78268e01398","detection":"ask-all",
           "endpoints":["node-a-pub","node-b-pub"],"nodes":["node-a","node-b"]}],
           "cleared":[{"at":"2026-10-04T09:00:00Z","ehr_id":"1d44b88c-4199-4bad-97dc-d78268e01398",
           "claimants":["node-a","node-b"]}],"replica":"ferrofed-0"}"#,
    )
    .await;
    answers(
        &gateway,
        ("GET", "/operator/creating-systems"),
        200,
        r#"{"items":[{"creating_system_id":"cdr-a.example.org","source":"member","node":"node-a","endpoint":null},
           {"creating_system_id":"legacy-a.example.org","source":"learned","node":"node-a","endpoint":"node-a-pub"}],
           "offset":0,"total":2,"replica":"ferrofed-1"}"#,
    )
    .await;
    let stored = r#"{"items":[{"name":"org.example::compositions","type":"AQL","version":"1.0.0","saved":"2026-10-05T08:00:00Z",
           "q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","read_only":false}],
           "offset":0,"total":1,"replica":"ferrofed-0"}"#;
    answers(&gateway, ("GET", "/operator/stored-queries"), 200, stored).await;
    answers(
        &gateway,
        ("GET", "/operator/stored-queries/org.example::compositions"),
        200,
        stored,
    )
    .await;
    answers(&gateway, ("GET", "/admin/config"), 200, STANDING).await;
    answers(&gateway, ("GET", "/admin/config/resources"), 200, RESOURCES).await;
    answers(&gateway, ("GET", "/admin/config/revisions"), 200, HISTORY).await;
    answers(
        &gateway,
        ("GET", "/admin/config/revisions/1/diff/2"),
        200,
        DIFF,
    )
    .await;
    Ok(gateway)
}

/// A console over `gateway`, its admin listener the same server, with one
/// signed-in operator.
pub(crate) fn signed_in_console(
    gateway: &MockServer,
) -> Result<(ConsoleState, axum::Router, SessionId), Box<dyn Error>> {
    let (state, service) = console(&format!(
        "[gateway]\nbase_url = \"{uri}\"\n\n[session]\nsecure_cookie = false\n\n\
         [[admin.replicas]]\nname = \"gateway-0\"\nadmin_url = \"{uri}\"\n",
        uri = gateway.uri()
    ))?;
    let session = state.sessions().establish(signed_in())?;
    Ok((state, service, session))
}

/// A form post of `fields` from the query console at `path`, carrying the
/// session cookie of `session`, as the console's own page sends it with no
/// WebAssembly: to the query's server function, with the page in the
/// answer field.
pub(crate) fn post_as(
    path: &str,
    session: &SessionId,
    fields: &[(&str, &str)],
) -> Result<Request<Body>, Box<dyn Error>> {
    let mut form = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in fields {
        form.append_pair(name, value);
    }
    crate::support::answer_post(
        path,
        &form.finish(),
        Some(session),
        &[("sec-fetch-site", "same-origin")],
    )
}
