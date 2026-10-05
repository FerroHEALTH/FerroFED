// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The issuer-bound `patient/` opt-in (`[auth.issuer.patient]`): a token's
//! `ehrId` (SMART on openEHR master04 §Capabilities, master07 §Context
//! Selection) read as an identifier in the bound member's `ehr_id` system,
//! resolved through the cross-reference at every member (§5.2), and every
//! request held to the patient's own `{node, ehr_id}` pairs, never to the
//! bare `ehrId`, which can name another patient's EHR at another node
//! (§12.5, §12.5.2). Each node is told the patient's `ehr_id` there and the
//! covering `patient/` scopes, so it can enforce the grant (N26). Every
//! refusal is asserted on what the nodes received: nothing (§16, track 10).
//! No specification defines a patient grant across nodes, so the opt-in is
//! FerroFED's own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use axum::body::Body;
use ferrofed_engine::onward::conveyance::HEADER;
use ferrofed_identity::patient::IdentifierNamespace;
use ferrofed_registry::id::EndpointId;
use ferrofed_server::EXIT_CONFIG;
use ferrofed_server::auth::refusal::Refusal;
use ferrofed_server::config::auth::{AuthSettings, PatientBinding};
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::headers::ENDPOINT;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{Gateway, TestResult, assert_refused, bearing, claims, minted, query, sent};
use crate::facade::{EHR_A, EHR_B, NAMESPACE, PATIENT, body, post, wire};
use crate::run::binary;
use crate::support::{self, Conveyed, asked, conveyed_claims, error_body};

/// The identifier system under which the cross-reference knows node A's
/// `ehr_id`s, an example OID.
const EHR_SYSTEM: &str = "urn:oid:2.999.9.1";

/// Another synthetic patient, and its `ehr_id` at node A and at node B.
const OTHER: &str = "SENTINEL-OTHER-71xw";
const OTHER_A: &str = "4444dddd-4444-4444-8444-444444444444";
const OTHER_B: &str = "5555eeee-5555-4555-8555-555555555555";

/// An `ehr_id` no member is known to hold.
const UNKNOWN_EHR: &str = "3333cccc-3333-4333-8333-333333333333";

/// The patient-facing grant: a query and a read of the patient's EHR.
const PATIENT_SCOPE: &str = "patient/aql-*.s patient/composition-*.r";

/// The one `[[dev.crossref]]` row placing `value` in `namespace` at
/// `member` under `ehr_id`.
fn row(text: &mut String, (namespace, value): (&str, &str), (member, ehr_id): (&str, &str)) {
    // NOTE: writing to a String cannot fail, so the result is dropped.
    let _written: std::fmt::Result = write!(
        text,
        "\n[[dev.crossref]]\nnamespace = \"{namespace}\"\nvalue = \"{value}\"\nmember = \"{member}\"\nehr_id = \"{ehr_id}\"\n"
    );
}

/// The cross-reference beyond the patient's own rows: the token's `ehrId`
/// in node A's `ehr_id` system, resolving to `at_a` at node A and to the
/// patient's `ehr_id` at node B, and the other patient at both nodes.
fn rows(at_a: &str) -> String {
    let mut text = String::new();
    row(&mut text, (EHR_SYSTEM, EHR_A), ("node-a", at_a));
    row(&mut text, (EHR_SYSTEM, EHR_A), ("node-b", EHR_B));
    row(&mut text, (NAMESPACE, OTHER), ("node-a", OTHER_A));
    row(&mut text, (NAMESPACE, OTHER), ("node-b", OTHER_B));
    text
}

/// The suite's `[auth]`, its issuer's patient tokens bound to node A.
fn bound() -> Result<AuthSettings, Box<dyn Error>> {
    let mut auth = support::auth();
    for issuer in &mut auth.issuers {
        issuer.patient = Some(PatientBinding {
            endpoint: EndpointId::new("node-a-pub")?,
            ehr_id_system: IdentifierNamespace::new(EHR_SYSTEM)?,
        });
    }
    Ok(auth)
}

/// The gateway over node A and node B, its issuer bound to node A.
async fn gateway() -> Result<Gateway, Box<dyn Error>> {
    Gateway::with_rows(bound()?, &rows(EHR_A)).await
}

/// A token granting `scope` with `ehrId` `ehr_id`, when set.
fn patient_token(scope: &str, ehr_id: Option<&str>) -> Result<String, Box<dyn Error>> {
    let mut granted = claims();
    granted.scope = Some(scope.to_owned());
    granted.ehr_id = ehr_id.map(str::to_owned);
    minted(&granted)
}

/// `request` bearing the token of the patient whose `ehrId` at node A is
/// [`EHR_A`].
fn as_the_patient(request: Request<Body>) -> Result<Request<Body>, Box<dyn Error>> {
    bearing(request, &patient_token(PATIENT_SCOPE, Some(EHR_A))?)
}

/// The federated query for `patient` in the suite's namespace.
fn query_for(patient: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{patient}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    Ok(post(body(&aql)?)?)
}

/// The federated query scoped to `ehr_id`, naming `target` in
/// `openEHR-federation-endpoint` when set.
fn scoped_to(ehr_id: &str, target: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{ehr_id}'"
    );
    let mut request = post(body(&aql)?)?;
    if let Some(target) = target {
        request.headers_mut().insert(ENDPOINT, target.parse()?);
    }
    Ok(request)
}

/// `GET` of `uri`, naming `target` in `openEHR-federation-endpoint` when
/// set.
fn get(uri: &str, target: Option<&str>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut request = Request::get(uri)
        .header(header::ACCEPT, "application/json")
        .body(Body::empty())?;
    if let Some(target) = target {
        request.headers_mut().insert(ENDPOINT, target.parse()?);
    }
    Ok(request)
}

/// Sends `request` to `gateway` and asserts it is refused `403`
/// `patient-confinement`, naming neither patient's `ehr_id`, with nothing
/// sent to either node.
async fn assert_confined(gateway: &Gateway, request: Request<Body>) -> TestResult {
    let (status, _, text) = sent(&gateway.app, request).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!("patient-confinement", error_body(&text)?.code, "{text}");
    for value in [EHR_A, EHR_B, OTHER, OTHER_A, OTHER_B, UNKNOWN_EHR] {
        assert!(!text.contains(value), "the answer names no ehr_id: {text}");
    }
    gateway.nobody_asked().await
}

/// The conveyed caller of every request `server` received.
async fn conveyed_at(server: &Server) -> Result<Vec<Conveyed>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut conveyed = Vec::new();
    for request in requests {
        let value = request
            .headers
            .get(HEADER)
            .ok_or("every request carries the caller")?;
        conveyed.push(conveyed_claims(value.to_str()?)?);
    }
    Ok(conveyed)
}

/// Mounts `GET /v1/ehr/{ehr_id}` answering `200` on `server`.
async fn answering_ehr(server: &Server, ehr_id: &str) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"ehr_id":{{"value":"{ehr_id}"}}}}"#).into_bytes(),
            "application/json",
        ))
        .mount(server)
        .await;
}

// NOTE: §5.2, §12.5: the token's ehrId resolves to the patient's pairs, and the patient's
// own query goes to each member under its own ehr_id with no identifier on the wire (N33).
// conformance: CP-17
#[tokio::test]
async fn the_token_s_own_patient_is_admitted_at_every_member() -> TestResult {
    let gateway = gateway().await?;
    let (status, _, text) = sent(&gateway.app, as_the_patient(query()?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (server, ehr_id) in [(&gateway.a, EHR_A), (&gateway.b, EHR_B)] {
        assert_eq!(1, asked(server).await?.len(), "each member is asked once");
        let captured = wire(server).await?;
        assert!(
            captured.contains(ehr_id),
            "under its own ehr_id: {captured}"
        );
        assert!(!captured.contains(PATIENT), "{captured}");
    }
    Ok(())
}

// NOTE: N26, §12.5: each node is told its own ehr_id for the patient and only the patient/
// scopes that cover the operation, so it can enforce the grant itself.
// conformance: CP-16
#[tokio::test]
async fn each_node_is_told_its_own_ehr_id_and_the_covering_patient_scope() -> TestResult {
    let gateway = gateway().await?;
    let (status, _, text) = sent(&gateway.app, as_the_patient(query()?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    for (server, ehr_id) in [(&gateway.a, EHR_A), (&gateway.b, EHR_B)] {
        let [conveyed] = conveyed_at(server)
            .await?
            .try_into()
            .map_err(|all: Vec<_>| format!("one conveyed caller, {} received", all.len()))?;
        assert_eq!(
            Some(ehr_id),
            conveyed.ehr_id.as_deref(),
            "the node's own ehr_id"
        );
        assert_eq!(
            Some("patient/aql-*.s"),
            conveyed.scope.as_deref(),
            "the covering patient/ scope, never the whole grant"
        );
    }
    Ok(())
}

// NOTE: §5.2, §12.5: a query for another patient resolves to pairs outside the token's.
// conformance: CP-17
#[tokio::test]
async fn a_query_for_another_patient_is_refused() -> TestResult {
    let gateway = gateway().await?;
    assert_confined(&gateway, as_the_patient(query_for(OTHER)?)?).await
}

// NOTE: SMART on openEHR master08 §Resource Scopes: a patient grant reaches "data within that
// patient's EHR", so a query that names no patient and no ehr_id is beyond it.
// conformance: CP-17
#[tokio::test]
async fn a_population_query_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let population = post(body(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c",
    )?)?;
    assert_confined(&gateway, as_the_patient(population)?).await
}

/// The query `from`, scoped by `e/ehr_id/value` to the token's own `ehr_id`
/// at node A, which is all the confinement routes by.
fn own_scope_from(from: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let aql = format!("SELECT c/uid/value FROM {from} WHERE e/ehr_id/value = '{EHR_A}'");
    Ok(post(body(&aql)?)?)
}

// NOTE: master08 §Resource Scopes, §7.1: a class beside the scoped EHR in an OR containment
// is not contained in it, so the node would answer other patients' rows.
// conformance: CP-17
#[tokio::test]
async fn a_class_beside_the_scoped_ehr_under_or_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let request = own_scope_from("EHR e OR COMPOSITION c")?;
    assert_confined(&gateway, as_the_patient(request)?).await
}

// NOTE: master08 §Resource Scopes, §7.1: a class beside the scoped EHR in an AND containment
// is not contained in it either.
// conformance: CP-17
#[tokio::test]
async fn a_class_beside_the_scoped_ehr_under_and_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let request = own_scope_from("EHR e AND COMPOSITION c")?;
    assert_confined(&gateway, as_the_patient(request)?).await
}

// NOTE: master08 §Resource Scopes, §7.1: an EHR under NOT CONTAINS selects by another EHR.
// conformance: CP-17
#[tokio::test]
async fn an_ehr_under_not_contains_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let request = own_scope_from("EHR e CONTAINS COMPOSITION c NOT CONTAINS EHR x")?;
    assert_confined(&gateway, as_the_patient(request)?).await
}

// NOTE: master08 §Resource Scopes, §7.1: a second EHR variable reads an EHR the scope does
// not name.
// conformance: CP-17
#[tokio::test]
async fn a_second_ehr_variable_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let request = own_scope_from("EHR e CONTAINS COMPOSITION c AND EHR x CONTAINS COMPOSITION d")?;
    assert_confined(&gateway, as_the_patient(request)?).await?;
    let request = own_scope_from("EHR e CONTAINS COMPOSITION c CONTAINS EHR x")?;
    assert_confined(&gateway, as_the_patient(request)?).await
}

// NOTE: master08 §Resource Scopes, §7.1: a patient query with a class beside its EHR reads
// beyond the patient too.
// conformance: CP-17
#[tokio::test]
async fn a_patient_query_with_a_class_beside_its_ehr_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let aql = format!(
        "SELECT c/uid/value FROM EHR e OR COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{PATIENT}' \
         AND e/ehr_status/subject/external_ref/namespace = '{NAMESPACE}'"
    );
    assert_confined(&gateway, as_the_patient(post(body(&aql)?)?)?).await
}

// NOTE: §7.1: every class contained, conjunctively, under the scoped EHR reads that EHR alone.
// conformance: CP-17
#[tokio::test]
async fn a_containment_nested_under_the_scoped_ehr_is_admitted() -> TestResult {
    let gateway = gateway().await?;
    let aql = format!(
        "SELECT o/uid/value FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o \
         WHERE e/ehr_id/value = '{EHR_A}'"
    );
    let (status, _, text) = sent(&gateway.app, as_the_patient(post(body(&aql)?)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(1, asked(&gateway.a).await?.len(), "node A, which holds it");
    assert!(asked(&gateway.b).await?.is_empty(), "node B is not asked");
    Ok(())
}

// NOTE: SMART on openEHR master04 §Capabilities conveys the context in the ehrId claim, so a
// token without one names no patient to confine its grant to.
// conformance: CP-17
#[tokio::test]
async fn a_patient_token_without_an_ehr_id_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let token = patient_token(PATIENT_SCOPE, None)?;
    assert_refused(
        &gateway,
        bearing(query()?, &token)?,
        Refusal::PatientContext,
    )
    .await?;
    let token = patient_token(PATIENT_SCOPE, Some("no ehr id"))?;
    assert_refused(
        &gateway,
        bearing(query()?, &token)?,
        Refusal::PatientContext,
    )
    .await
}

// NOTE: §12.5, §12.5.2: the token's ehrId is node A's; the same value at node B may be
// another patient's EHR, so it is compared as a pair and never matched bare.
// conformance: CP-17
#[tokio::test]
async fn the_token_s_ehr_id_at_another_member_is_refused() -> TestResult {
    let gateway = gateway().await?;
    let to_b = Some("node-b-pub");
    assert_confined(&gateway, as_the_patient(scoped_to(EHR_A, to_b)?)?).await?;
    let route = get(&format!("/v1/ehr/{EHR_A}"), to_b)?;
    assert_confined(&gateway, as_the_patient(route)?).await
}

// NOTE: §12.5.1: the patient's own pairs place an ehr_id at its member, with no probe.
// conformance: CP-17
#[tokio::test]
async fn the_patient_s_ehr_id_routes_to_its_own_member() -> TestResult {
    let gateway = gateway().await?;
    answering_ehr(&gateway.b, EHR_B).await;
    let (status, _, text) = sent(
        &gateway.app,
        as_the_patient(get(&format!("/v1/ehr/{EHR_B}"), None)?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(asked(&gateway.a).await?.is_empty(), "node A is not asked");
    let [conveyed] = conveyed_at(&gateway.b)
        .await?
        .try_into()
        .map_err(|all: Vec<_>| format!("one conveyed caller, {} received", all.len()))?;
    assert_eq!(Some(EHR_B), conveyed.ehr_id.as_deref());
    let (status, _, text) = sent(&gateway.app, as_the_patient(scoped_to(EHR_A, None)?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        asked(&gateway.a).await?.len(),
        "the scoped query goes to node A"
    );
    Ok(())
}

// NOTE: §12.5, §12.5.2: an ehr_id the patient's pairs do not place is refused before the
// index or the ask-all probe is consulted, so no member is asked about it.
// conformance: CP-17
#[tokio::test]
async fn an_ehr_id_outside_the_patient_s_pairs_is_refused_with_no_probe() -> TestResult {
    let gateway = gateway().await?;
    let route = get(&format!("/v1/ehr/{UNKNOWN_EHR}"), None)?;
    assert_confined(&gateway, as_the_patient(route)?).await?;
    assert_confined(&gateway, as_the_patient(scoped_to(UNKNOWN_EHR, None)?)?).await?;
    let route = get(&format!("/v1/ehr/{OTHER_A}/ehr_status"), Some("node-a-pub"))?;
    assert_confined(&gateway, as_the_patient(route)?).await
}

// NOTE: §5.2, §12.5: a read by subject reaches the patient's own EHR alone.
// conformance: CP-17
#[tokio::test]
async fn a_read_by_subject_reaches_the_patient_s_own_ehr_alone() -> TestResult {
    let gateway = gateway().await?;
    let other = format!("/v1/ehr?subject_id={OTHER}&subject_namespace={NAMESPACE}");
    assert_confined(&gateway, as_the_patient(get(&other, Some("node-a-pub"))?)?).await?;
    answering_ehr(&gateway.a, EHR_A).await;
    let own = format!("/v1/ehr?subject_id={PATIENT}&subject_namespace={NAMESPACE}");
    let (status, _, text) = sent(
        &gateway.app,
        as_the_patient(get(&own, Some("node-a-pub"))?)?,
    )
    .await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("GET".to_owned(), format!("/v1/ehr/{EHR_A}"))],
        asked(&gateway.a).await?
    );
    Ok(())
}

// NOTE: §12.5.1 step 2, N41: a refused read records no session binding and no index entry
// for the other patient, so the same caller's later read of that ehr_id is probed.
// conformance: CP-17
#[tokio::test]
async fn a_refused_read_by_subject_records_nothing_of_the_other_patient() -> TestResult {
    let gateway = gateway().await?;
    let other = format!("/v1/ehr?subject_id={OTHER}&subject_namespace={NAMESPACE}");
    assert_confined(&gateway, as_the_patient(get(&other, Some("node-a-pub"))?)?).await?;
    answering_ehr(&gateway.a, OTHER_A).await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{OTHER_A}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&gateway.b)
        .await;
    let read = get(&format!("/v1/ehr/{OTHER_A}"), None)?;
    let (status, _, text) = sent(&gateway.app, bearing(read, &minted(&claims())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("GET".to_owned(), format!("/v1/ehr/{OTHER_A}"))],
        asked(&gateway.b).await?,
        "neither a binding nor the index places it, so the same caller's read probes every member"
    );
    Ok(())
}

// NOTE: SMART on openEHR master08 §Resource Scopes: a patient scope reaches its patient's
// own EHR, so a listed demographic client's patient grant never reaches a party.
// conformance: CP-17
#[tokio::test]
async fn a_patient_grant_never_reaches_the_demographic_api() -> TestResult {
    let bound = gateway().await?;
    let unbound = Gateway::with_rows(support::auth(), &rows(EHR_A)).await?;
    for gateway in [bound, unbound] {
        let read =
            Request::get("/v1/demographic/person/8849182c-82ad-4088-a07f-48ead4180515::node-a::1")
                .body(Body::empty())?;
        assert_refused(&gateway, as_the_patient(read)?, Refusal::PatientDemographic).await?;
    }
    Ok(())
}

// NOTE: §12.5, N33: node B is told the patient's ehr_id at node B, never the token's ehrId,
// which is node A's.
// conformance: CP-16
#[tokio::test]
async fn a_member_other_than_the_token_s_own_is_told_its_own_ehr_id() -> TestResult {
    let gateway = gateway().await?;
    let (status, _, text) = sent(&gateway.app, as_the_patient(query()?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let told: Vec<Option<String>> = conveyed_at(&gateway.b)
        .await?
        .into_iter()
        .map(|conveyed| conveyed.ehr_id)
        .collect();
    assert_eq!(
        vec![Some(EHR_B.to_owned())],
        told,
        "node B's own ehr_id, not the token's"
    );
    Ok(())
}

// NOTE: SMART on openEHR master08 §Resource Scopes: a patient grant reaches data in its
// patient's existing EHRs, so it creates none.
// conformance: CP-17
#[tokio::test]
async fn a_patient_grant_creates_no_ehr() -> TestResult {
    let gateway = gateway().await?;
    let scope = "patient/composition-*.crud patient/aql-*.s";
    let token = patient_token(scope, Some(EHR_A))?;
    for create in [
        Request::post("/v1/ehr").body(Body::empty())?,
        Request::put(format!("/v1/ehr/{EHR_A}")).body(Body::empty())?,
    ] {
        let mut create = bearing(create, &token)?;
        create.headers_mut().insert(ENDPOINT, "node-a-pub".parse()?);
        assert_confined(&gateway, create).await?;
    }
    Ok(())
}

// NOTE: §5.2, §11.2: a cross-reference that places the token's patient under another
// ehr_id at the member that issued the token contradicts it, so nothing is sent.
// conformance: CP-17
#[tokio::test]
async fn a_cross_reference_contradicting_the_token_is_a_424() -> TestResult {
    let gateway = Gateway::with_rows(bound()?, &rows(OTHER_A)).await?;
    let (status, _, text) = sent(&gateway.app, as_the_patient(query()?)?).await?;
    assert_eq!(StatusCode::FAILED_DEPENDENCY, status, "{text}");
    assert_eq!(
        "patient-context-unavailable",
        error_body(&text)?.code,
        "{text}"
    );
    gateway.nobody_asked().await
}

// NOTE: SMART on openEHR master07 §Context Selection, §12.5: without the opt-in nothing binds
// the ehrId to a member, so the grant admits nothing, its own patient included.
// conformance: CP-17
#[tokio::test]
async fn without_the_opt_in_a_patient_grant_still_admits_nothing() -> TestResult {
    let gateway = Gateway::with_rows(support::auth(), &rows(EHR_A)).await?;
    assert_refused(&gateway, as_the_patient(query()?)?, Refusal::Scope).await?;
    let route = get(&format!("/v1/ehr/{EHR_A}"), None)?;
    assert_refused(&gateway, as_the_patient(route)?, Refusal::Scope).await
}

/// A configuration over node A alone, its registry document written to
/// `dir`, whose issuer's patient tokens are bound to `endpoint`, with the
/// `[dev]` cross-reference when `resolving`.
fn configured(dir: &Path, endpoint: &str, resolving: bool) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        "[[organisation]]\nid = \"org-a\"\n\n\
         [[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n\
         [[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"http://127.0.0.1:9/openehr\"\n\
         connection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
    )?;
    let path = toml::Value::String(document.display().to_string());
    let (profile, crossref) = if resolving {
        let mut text = String::new();
        row(&mut text, (EHR_SYSTEM, EHR_A), ("node-a", EHR_A));
        ("profile = \"development\"\n", text)
    } else {
        ("", String::new())
    };
    Ok(format!(
        "{profile}[server]\nlisten = \"127.0.0.1:1\"\n\n[registry]\ndocument = {path}\n\n\
         [federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n{crossref}\n\
         [auth]\naudience = \"urn:example:gateway\"\n\n\
         [[auth.issuer]]\nissuer = \"https://issuer.example.test\"\n\
         jwks_uri = \"https://issuer.example.test/jwks\"\n\n\
         [auth.issuer.patient]\nendpoint = \"{endpoint}\"\nehr_id_system = \"{EHR_SYSTEM}\"\n"
    ))
}

/// `config check` over `toml`: its exit code and its standard error.
fn checked(toml: &str) -> Result<(Option<i32>, String), Box<dyn Error>> {
    let output = binary(&["config", "check"], toml)?;
    Ok((
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    ))
}

// NOTE: no specification governs this: our own design; a patient token's member is held
// to the registry as every configured endpoint is.
#[test]
fn config_check_refuses_a_binding_to_an_endpoint_the_registry_lacks() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (code, stderr) = checked(&configured(dir.path(), "node-z-pub", true)?)?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), code, "{stderr}");
    assert!(
        stderr.contains("auth.issuer[0].patient") && stderr.contains("node-z-pub"),
        "{stderr}"
    );
    let (code, stderr) = checked(&configured(dir.path(), "node-a-pub", true)?)?;
    assert_eq!(Some(0), code, "{stderr}");
    Ok(())
}

// NOTE: §5.2: the token's ehrId is resolved through the cross-reference, so a binding
// without one cannot confine any grant.
#[test]
fn config_check_refuses_a_binding_without_a_resolver() -> TestResult {
    let dir = tempfile::tempdir()?;
    let (code, stderr) = checked(&configured(dir.path(), "node-a-pub", false)?)?;
    assert_eq!(Some(i32::from(EXIT_CONFIG)), code, "{stderr}");
    assert!(
        stderr.contains("auth.issuer[0].patient") && stderr.contains("resolver"),
        "{stderr}"
    );
    Ok(())
}
