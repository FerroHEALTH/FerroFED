// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node profile checks offline, against stub nodes reached through the
//! endpoint's node client: each check passes a node that meets its
//! obligation, fails one that breaks it, and leaves one it cannot decide not
//! observable (§16.2). The harness runs the same checks against its CDR
//! products, in the testkit's `e2e::node_profile`.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use ferrofed_server::admission::report::Condition;
use ferrofed_server::config::Config;
use ferrofed_server::conformance::node_profile::interface::{Arrangement, Interface, Principal};
use ferrofed_server::conformance::node_profile::{Check, Finding, Profile, Verdict, checks};
use ferrofed_server::conformance::report::FINDINGS_HEADER;
use ferrofed_server::federation::Federation;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::seed::{self, PatientId};
use openehr_its::json::to_canonical_json;
use openehr_its::rest::client::Credentials;
use uuid::Uuid;
use wiremock::matchers::{basic_auth, body_string_contains, method, path, path_regex};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The endpoint every stub node is registered under.
const ENDPOINT: &str = "node-a-pub";

/// The EHR every stub node holds.
const EHR: Uuid = Uuid::from_u128(0x9393_9393_9393_4393_8393_9393_9393_9393);

/// The marker of the query scoped by `e/ehr_id/value`.
const WHERE_FORM: &str = "WHERE e/ehr_id/value";

/// The marker of the query scoped by the `EHR` predicate.
const PREDICATE_FORM: &str = "e[ehr_id/value=";

/// The canonical JSON of the EHR `ehr_id` at a node stamping `cdr.example.org`.
fn ehr_body(ehr_id: Uuid) -> Vec<u8> {
    format!(
        r#"{{"_type":"EHR","system_id":{{"_type":"HIER_OBJECT_ID","value":"cdr.example.org"}},"ehr_id":{{"_type":"HIER_OBJECT_ID","value":"{ehr_id}"}},"ehr_status":{{"_type":"OBJECT_REF","namespace":"local","type":"EHR_STATUS","id":{{"_type":"HIER_OBJECT_ID","value":"7f1e2d3c-4b5a-4968-8776-655443322110"}}}},"ehr_access":{{"_type":"OBJECT_REF","namespace":"local","type":"EHR_ACCESS","id":{{"_type":"HIER_OBJECT_ID","value":"8a2f3e4d-5c6b-4a79-9887-766554433221"}}}},"time_created":{{"_type":"DV_DATE_TIME","value":"2026-10-05T09:00:00Z"}}}}"#
    )
    .into_bytes()
}

/// A `RESULT_SET` of `count` rows of one column.
fn result_set(count: usize) -> Vec<u8> {
    let rows = vec![format!("[\"{EHR}\"]"); count].join(",");
    format!(r##"{{"columns":[{{"name":"#0","path":"/ehr_id/value"}}],"rows":[{rows}]}}"##)
        .into_bytes()
}

/// An answer of `status` carrying `body` as JSON.
fn json(status: u16, body: Vec<u8>) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_raw(body, "application/json")
}

/// Mounts the reads of [`EHR`] and its `EHR_STATUS`, both `200`.
async fn reads(server: &Server) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .respond_with(json(200, ehr_body(EHR)))
        .mount(server)
        .await;
    let status = to_canonical_json(&seed::ehr_status(Some(PatientId::new(1, 93))));
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}/ehr_status")))
        .respond_with(json(200, status.into_bytes()))
        .mount(server)
        .await;
}

/// Mounts the two scoped queries, answering `where_rows` and
/// `predicate_rows` rows.
async fn queries(server: &Server, where_rows: usize, predicate_rows: usize) {
    for (form, count) in [(WHERE_FORM, where_rows), (PREDICATE_FORM, predicate_rows)] {
        Mock::given(method("POST"))
            .and(path("/v1/query/aql"))
            .and(body_string_contains(form))
            .respond_with(json(200, result_set(count)))
            .mount(server)
            .await;
    }
}

/// The development federation over the one stub node at `base`, reached
/// with the permitted principal's Basic credentials as its onward
/// credentials.
fn federation(base: &str) -> Result<Federation, Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(
        &document,
        format!(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr.example.org\"\n\n[[endpoint]]\nid = \"{ENDPOINT}\"\nnode = \"node-a\"\nurl = \"{base}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
        ),
    )?;
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {}\n\n[federation]\nper_node_timeout_ms = 5000\noverall_timeout_ms = 6000\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n\n[credentials.\"{ENDPOINT}\"]\nuser = \"permitted\"\npassword = \"permitted-example\"\n",
        toml::Value::String(document.display().to_string())
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    Ok(Federation::load(&settings)?.ok_or("a registry is configured")?)
}

/// The interface of the endpoint at `server`.
fn interface(server: &Server) -> Result<Interface, Box<dyn Error>> {
    Ok(Interface::of(
        &federation(&server.uri())?,
        &EndpointId::new(ENDPOINT)?,
    )?)
}

#[tokio::test]
async fn a_node_answering_to_the_ehr_id_alone_is_invocable() -> TestResult {
    let server = Server::start().await;
    reads(&server).await;
    queries(&server, 1, 2).await;

    let finding = checks::invocable_on_ehr_id(&interface(&server)?, EHR).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    assert_eq!(4, finding.evidence().len(), "{finding:?}");
    assert_eq!(&["CP-27"], finding.check().points());

    let requests = server.received_requests().await.unwrap_or_default();
    for request in &requests {
        let named = request.url.path().contains(&EHR.to_string())
            || String::from_utf8_lossy(&request.body).contains(&EHR.to_string());
        assert!(
            named,
            "every request names the EHR by its ehr_id: {request:?}"
        );
        assert!(
            !String::from_utf8_lossy(&request.body).contains("subject"),
            "no request carries a subject: {request:?}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_node_whose_scoped_query_finds_no_composition_is_not_invocable() -> TestResult {
    let server = Server::start().await;
    reads(&server).await;
    queries(&server, 1, 0).await;

    let finding = checks::invocable_on_ehr_id(&interface(&server)?, EHR).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_node_that_does_not_serve_the_ehr_by_its_ehr_id_is_not_invocable() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    queries(&server, 1, 1).await;

    let finding = checks::invocable_on_ehr_id(&interface(&server)?, EHR).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

/// Mounts a node that creates [`EHR`] on `POST /ehr` with no body and serves
/// its `EHR_STATUS` with `subject`.
async fn creating(server: &Server, subject: Option<PatientId>) {
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(json(201, ehr_body(EHR)))
        .mount(server)
        .await;
    let status = to_canonical_json(&seed::ehr_status(subject));
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}/ehr_status")))
        .respond_with(json(200, status.into_bytes()))
        .mount(server)
        .await;
    queries(server, 1, 1).await;
}

#[tokio::test]
async fn a_node_serving_an_ehr_created_without_a_subject_never_requires_one() -> TestResult {
    let server = Server::start().await;
    creating(&server, None).await;

    let finding = checks::subject_not_required(&interface(&server)?).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    let requests = server.received_requests().await.unwrap_or_default();
    let create = requests
        .iter()
        .find(|request| request.method.as_str() == "POST" && request.url.path() == "/v1/ehr")
        .ok_or("the check creates an EHR")?;
    assert!(create.body.is_empty(), "the create carries no EHR_STATUS");
    Ok(())
}

#[tokio::test]
async fn a_node_refusing_an_ehr_without_a_subject_requires_one() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;

    let finding = checks::subject_not_required(&interface(&server)?).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_node_giving_the_ehr_a_subject_of_its_own_is_not_observable() -> TestResult {
    let server = Server::start().await;
    creating(&server, Some(PatientId::new(1, 94))).await;

    let finding = checks::subject_not_required(&interface(&server)?).await?;
    assert_eq!(Verdict::NotObservable, finding.verdict(), "{finding:?}");
    Ok(())
}

/// Mounts a node that answers an unknown `ehr_id` with `unknown` and an
/// unparsable query with `unparsable`.
async fn erring(server: &Server, unknown: u16, unparsable: u16) {
    Mock::given(method("GET"))
        .and(path_regex(r"^/v1/ehr/[0-9a-f-]{36}(/ehr_status)?$"))
        .respond_with(ResponseTemplate::new(unknown))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .respond_with(ResponseTemplate::new(unparsable))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_node_answering_the_its_rest_errors_passes_them_through() -> TestResult {
    let server = Server::start().await;
    erring(&server, 404, 400).await;

    let finding = checks::errors_passed_through(&interface(&server)?).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    assert_eq!(&["CP-18"], finding.check().points());
    Ok(())
}

#[tokio::test]
async fn a_node_answering_success_for_an_unknown_ehr_flattens_its_error() -> TestResult {
    let server = Server::start().await;
    erring(&server, 200, 400).await;

    let finding = checks::errors_passed_through(&interface(&server)?).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_node_answering_another_error_status_than_its_rest_fails() -> TestResult {
    let server = Server::start().await;
    erring(&server, 404, 500).await;

    let finding = checks::errors_passed_through(&interface(&server)?).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

/// The principal a stub node refuses [`EHR`] to; the endpoint's onward
/// credentials present the one it serves it to.
fn refused() -> Principal {
    Principal::new(
        "refused",
        Arc::new(Credentials::basic("refused", "refused-example")),
    )
}

/// Mounts a node serving [`EHR`] to the permitted principal, refusing the
/// read to the refused one with `read`, and answering its scoped queries
/// with `query` and `rows` rows.
async fn arranged(server: &Server, read: u16, query: u16, rows: usize) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .and(basic_auth("permitted", "permitted-example"))
        .respond_with(json(200, ehr_body(EHR)))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(basic_auth("permitted", "permitted-example"))
        .respond_with(json(200, result_set(1)))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .and(basic_auth("refused", "refused-example"))
        .respond_with(ResponseTemplate::new(read))
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/query/aql"))
        .and(basic_auth("refused", "refused-example"))
        .respond_with(json(query, result_set(rows)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_refusal_held_on_the_read_and_both_query_forms_is_the_nodes_decision() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 403, 403, 0).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    assert_eq!(Check::AccessDecidedAtNode, finding.check());
    Ok(())
}

#[tokio::test]
async fn a_scoped_query_answering_no_row_withholds_the_ehr() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 403, 200, 0).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_scoped_query_releasing_what_the_read_refuses_fails_the_decision() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 403, 200, 1).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    let released = finding
        .evidence()
        .iter()
        .filter(|line| line.contains("released 1 row(s)"))
        .count();
    assert_eq!(2, released, "both query forms release: {finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_read_released_to_the_refused_principal_fails_the_decision() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 200, 403, 0).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Fail, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_refusal_the_permitted_principal_gets_too_shows_nothing() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::NotObservable, finding.verdict(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_query_form_that_serves_no_row_to_the_permitted_principal_shows_nothing() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .and(basic_auth("permitted", "permitted-example"))
        .respond_with(json(200, ehr_body(EHR)))
        .mount(&server)
        .await;
    for (form, count) in [(WHERE_FORM, 1), (PREDICATE_FORM, 0)] {
        Mock::given(method("POST"))
            .and(path("/v1/query/aql"))
            .and(basic_auth("permitted", "permitted-example"))
            .and(body_string_contains(form))
            .respond_with(json(200, result_set(count)))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(basic_auth("refused", "refused-example"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(basic_auth("refused", "refused-example"))
        .respond_with(json(200, result_set(0)))
        .mount(&server)
        .await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(
        Verdict::NotObservable,
        finding.verdict(),
        "an empty answer to the refused principal proves nothing where the permitted one gets none either: {finding:?}"
    );
    Ok(())
}

#[tokio::test]
async fn a_consent_refusal_is_observed_under_its_own_point() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 403, 403, 0).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::consent_before_release(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    assert_eq!(Check::ConsentBeforeRelease, finding.check());
    assert_eq!(&["CP-19"], finding.check().points());
    Ok(())
}

#[test]
fn nothing_arranged_is_not_observable_and_never_a_pass() {
    let finding = Finding::not_arranged(Check::ConsentBeforeRelease, "no refusal arranged");
    assert_eq!(Verdict::NotObservable, finding.verdict());
    assert_eq!(
        Verdict::NotObservable,
        Finding::from_observations(Check::InvocableOnEhrId, Vec::new()).verdict(),
        "a finding with no observation is never a pass"
    );
}

#[test]
fn a_failing_observation_fails_the_finding_whatever_else_passed() {
    let finding = Finding::from_observations(
        Check::ErrorsPassedThrough,
        vec![
            (Verdict::Pass, "one".to_owned()),
            (Verdict::NotObservable, "two".to_owned()),
            (Verdict::Fail, "three".to_owned()),
        ],
    );
    assert_eq!(Verdict::Fail, finding.verdict());
    assert_eq!(3, finding.evidence().len());
}

#[test]
fn a_profile_writes_one_row_per_point_with_clean_cells() -> TestResult {
    let mut profile = Profile::new("Example CDR 1.0");
    profile.record(Finding::new(
        Check::IdentifierIntegrity(Condition::EhrIdExchange),
        Verdict::NotObservable,
        vec!["a line\twith a tab".to_owned(), "and\na break".to_owned()],
    ));
    let text = profile.to_tsv();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        vec![
            FINDINGS_HEADER,
            "Example CDR 1.0\tCP-27\tehr_id exchange\tnot-observable\ta line with a tab; and a break",
            "Example CDR 1.0\tCP-33a\tehr_id exchange\tnot-observable\ta line with a tab; and a break",
        ],
        lines
    );

    let dir = tempfile::tempdir()?;
    let written = profile.write(&dir.path().join("node-profile"), "example")?;
    assert_eq!(text, std::fs::read_to_string(written)?);
    Ok(())
}

#[test]
fn a_principal_never_shows_its_credentials() {
    let principal = Principal::new(
        "someone",
        Arc::new(Credentials::basic("someone", "secret-example")),
    );
    let shown = format!("{principal:?}");
    assert!(shown.contains("someone"), "{shown}");
    assert!(!shown.contains("secret-example"), "{shown}");
}

#[tokio::test]
async fn every_request_leaves_through_the_node_client_with_the_onward_credentials() -> TestResult {
    let server = Server::start().await;
    erring(&server, 404, 400).await;

    checks::errors_passed_through(&interface(&server)?).await?;
    let requests = server.received_requests().await.unwrap_or_default();
    assert_eq!(3, requests.len(), "one request per observation");
    for request in &requests {
        assert_eq!(
            Some("Basic cGVybWl0dGVkOnBlcm1pdHRlZC1leGFtcGxl"),
            request
                .headers
                .get("authorization")
                .and_then(|value| value.to_str().ok()),
            "the endpoint's onward credentials: {request:?}"
        );
        for minted in ["x-request-id", "openehr-federation-client"] {
            assert!(
                request.headers.contains_key(minted),
                "the node client sets {minted} on every request: {request:?}"
            );
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_401_to_the_refused_principal_is_its_refusal_and_never_an_error() -> TestResult {
    let server = Server::start().await;
    arranged(&server, 401, 401, 0).await;
    let arrangement = Arrangement::new(EHR, refused());

    let finding = checks::access_decided_at_node(&interface(&server)?, &arrangement).await?;
    assert_eq!(Verdict::Pass, finding.verdict(), "{finding:?}");
    let refusals = finding
        .evidence()
        .iter()
        .filter(|line| line.contains("as the refused principal refused answered 401"))
        .count();
    assert_eq!(3, refusals, "the read and both query forms: {finding:?}");
    Ok(())
}

#[tokio::test]
async fn the_subjectless_check_names_the_ehr_it_created() -> TestResult {
    let server = Server::start().await;
    creating(&server, None).await;

    let finding = checks::subject_not_required(&interface(&server)?).await?;
    assert_eq!(&[EHR], finding.created(), "{finding:?}");
    Ok(())
}

#[tokio::test]
async fn a_node_that_cannot_be_reached_is_an_error_and_never_a_finding() -> TestResult {
    let interface = Interface::of(
        &federation(ferrofed_testkit::unreachable::BASE)?,
        &EndpointId::new(ENDPOINT)?,
    )?;

    let unreached = checks::errors_passed_through(&interface).await;
    assert!(unreached.is_err(), "{unreached:?}");
    Ok(())
}
