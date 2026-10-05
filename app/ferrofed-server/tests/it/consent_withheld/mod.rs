// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A deployment that keeps a consent exclusion out of the answer,
//! `[federation.consent] disclose = false`, for Regulation (EU) 2025/327
//! Art 8: "The fact that a natural person has restricted access ... shall not
//! be visible to healthcare providers."
//!
//! A member the Step-1 pre-filter excludes is never contacted, and is
//! reported exactly as a member the cross-reference does not know the patient
//! at: `not-resolved`, which clears `complete` and fails nothing, as
//! `consent-denied` does (§11.1, §11.3, N16, N37). A read by subject that
//! only a denied member could serve answers as one no member could:
//! `404 subject-unavailable`. The operator still counts every exclusion in
//! the pre-filter metrics. With the setting left at its default, the
//! specification's `consent-denied` stands (N27a). A node's own consent
//! refusal is withheld the same way on every path ([`node`]).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the tests compare emitted endpoint records as JSON values"
)]

mod node;
mod not_asked;
mod routed;

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use ferrofed_engine::dispatch::NodeClients;
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::role::consent::{ConsentDecision, ConsentPrefilter, Requester};
use ferrofed_identity::role::patient::PatientRef;
use ferrofed_identity::role::resolver::{Resolution, Resolver, ResolverError};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::config::settings::ConsentDisclosure;
use ferrofed_server::federation::Federation;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use http::{Method, Request, StatusCode, header};
use openehr_federation::aql::{Context, Targeting};
use openehr_federation::id::FederationId;
use openehr_its::rest::client::ReqwestTransport;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, body, crossref, gateway, node_answering,
    patient_query, post, registry, schema, settings_with_room, statuses, wire,
};
use crate::metrics::{count, parse};
use crate::support::{call, error_body, send};

pub(super) type TestResult = Result<(), Box<dyn Error>>;

/// The Prometheus name of the pre-filter call counter.
const PREFILTER_CALLS: &str = "ferrofed_consent_prefilter_requests_total";

/// A consent pre-filter that denies asking `node-b`, or denies nothing.
#[derive(Debug, Clone, Copy)]
pub(super) struct Denies(pub(super) bool);

#[async_trait]
impl ConsentPrefilter for Denies {
    async fn prefilter(
        &self,
        _patient: &PatientRef,
        _requester: Option<&Requester>,
        candidates: &[NodeId],
        _deadline: Instant,
    ) -> ConsentDecision {
        if !self.0 {
            return ConsentDecision::NoSignal;
        }
        ConsentDecision::Denied(
            candidates
                .iter()
                .filter(|member| member.as_str() == "node-b")
                .cloned()
                .collect(),
        )
    }

    fn mode(&self) -> &'static str {
        "test-scripted"
    }

    fn budget(&self) -> Option<Duration> {
        None
    }
}

/// A resolver that knows the patient at the members it names, or that
/// cannot answer for any member.
#[derive(Debug, Clone, Copy)]
pub(super) enum Crossref {
    /// Knows the patient at these members, with their `ehr_id`s.
    Knows(&'static [(&'static str, &'static str)]),
    /// Cannot answer for any member.
    Down,
}

#[async_trait]
impl Resolver for Crossref {
    async fn resolve(
        &self,
        _patient: &PatientRef,
        members: &[NodeId],
        _on_behalf: &ferrofed_identity::role::behalf::OnBehalfOf,
        _deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        members
            .iter()
            .filter_map(|member| {
                let resolution = match self {
                    Self::Down => Resolution::Unavailable(ResolverError::DeadlineExceeded),
                    Self::Knows(known) => match known.iter().find(|(at, _)| *at == member.as_str())
                    {
                        Some((_, ehr)) => Resolution::Resolved(EhrId::new(*ehr).ok()?),
                        None => Resolution::Unknown,
                    },
                };
                Some((member.clone(), resolution))
            })
            .collect()
    }
}

/// The patient known at both members.
pub(super) const BOTH: &[(&str, &str)] = &[("node-a", EHR_A), ("node-b", EHR_B)];

/// The patient known at node A only.
pub(super) const AT_A: &[(&str, &str)] = &[("node-a", EHR_A)];

/// The patient known at node B only.
pub(super) const AT_B: &[(&str, &str)] = &[("node-b", EHR_B)];

/// The patient known at no member.
pub(super) const NOWHERE: &[(&str, &str)] = &[];

/// A metered gateway over node A and node B resolving through `crossref`,
/// pre-filtering through `prefilter`, under `disclosure`.
fn gateway_over(
    nodes: (&Server, &Server),
    scripted: (Crossref, impl ConsentPrefilter + 'static),
    disclosure: ConsentDisclosure,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    gateway_with(nodes, "", scripted, disclosure)
}

/// The gateway of [`gateway_over`], with `extra` in node B's endpoint entry.
fn gateway_with(
    (a, b): (&Server, &Server),
    extra: &str,
    (crossref, prefilter): (Crossref, impl ConsentPrefilter + 'static),
    disclosure: ConsentDisclosure,
) -> Result<(Router, Arc<AppState>), Box<dyn Error>> {
    let snapshot = RegistrySnapshot::from_toml_str(&registry(&a.uri(), &b.uri(), extra))?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(5))?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?;
    let federation = Federation::new(
        FederationId::new("example-federation")?,
        snapshot,
        clients,
        Some(Arc::new(crossref)),
        Context::new(Targeting::AskAll),
        Budget::new(Duration::from_secs(2), Duration::from_secs(3))?,
    )
    .with_consent_prefilter(Arc::new(prefilter))
    .with_consent_disclosure(disclosure)
    .with_signer(crate::support::signer("example-federation")?);
    let state = Arc::new(AppState::with_federation(federation));
    Ok((
        ferrofed_server::router(Arc::clone(&state), &settings_with_room()),
        state,
    ))
}

/// The pre-filter calls the metrics of `state` counted with `outcome`.
fn prefilter_calls(state: &AppState, outcome: &str) -> Result<Option<String>, Box<dyn Error>> {
    let samples = parse(&state.metrics().render()?)?;
    Ok(count(&samples, PREFILTER_CALLS, &[("outcome", outcome)]))
}

/// One endpoint record of `meta.federation.endpoints[]`, as the wire has it.
#[derive(Debug, Deserialize)]
struct Records {
    meta: RecordsMeta,
}

#[derive(Debug, Deserialize)]
struct RecordsMeta {
    federation: RecordsFederation,
}

#[derive(Debug, Deserialize)]
struct RecordsFederation {
    endpoints: Vec<serde_json::Map<String, serde_json::Value>>,
}

/// The record of `endpoint` in the answer `text`, every member as sent.
pub(super) fn record_of(
    text: &str,
    endpoint: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, Box<dyn Error>> {
    let records: Records = serde_json::from_str(text)?;
    records
        .meta
        .federation
        .endpoints
        .into_iter()
        .find(|record| record.get("id").and_then(serde_json::Value::as_str) == Some(endpoint))
        .ok_or_else(|| format!("{endpoint} is not reported: {text}").into())
}

/// Runs the patient query through `app`, and returns the status, every
/// response header value, and the body.
pub(super) async fn ask(app: Router) -> Result<(StatusCode, Vec<String>, String), Box<dyn Error>> {
    let response = send(app, post(body(&patient_query())?)?).await?;
    let status = response.status();
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| format!("{name}: {}", String::from_utf8_lossy(value.as_bytes())))
        .collect();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    Ok((status, headers, String::from_utf8(bytes.to_vec())?))
}

/// Whether `text` names consent, in any case.
pub(super) fn names_consent(text: &str) -> bool {
    text.to_ascii_lowercase().contains("consent")
}

// conformance: CP-36
#[tokio::test]
async fn a_withheld_exclusion_is_reported_as_a_member_without_the_patient() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let scripted = (Crossref::Knows(BOTH), Denies(true));
    let (app, state) = gateway_over((&a, &b), scripted, ConsentDisclosure::Withheld)?;

    let (status, headers, text) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "§11.3: nothing fails: {text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-resolved")],
        statuses(&answer),
        "Art 8: the excluded member reads as one without the patient"
    );
    assert!(
        !answer.meta.federation.complete,
        "N16, N37: a member that may hold data was not asked, so the answer is not whole"
    );
    assert_eq!(1, answer.rows.len(), "node A's row only");
    assert!(
        !names_consent(&text),
        "Art 8: the body names no consent: {text}"
    );
    for line in &headers {
        assert!(
            !names_consent(line),
            "Art 8: no header names consent: {line}"
        );
    }
    assert!(
        !record_of(&text, "node-b-pub")?.contains_key("latency_ms"),
        "N40: no request existed"
    );
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");
    assert_eq!(
        Some("1".to_owned()),
        prefilter_calls(&state, "denied")?,
        "the operator still counts the exclusion"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_withheld_exclusion_reads_exactly_as_a_member_that_does_not_know_the_patient()
-> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let excluded = (Crossref::Knows(BOTH), Denies(true));
    let (app, _state) = gateway_over((&a, &b), excluded, ConsentDisclosure::Withheld)?;
    let (_, _, hidden) = ask(app).await?;

    let unknown = (Crossref::Knows(AT_A), Denies(false));
    let (app, _state) = gateway_over((&a, &b), unknown, ConsentDisclosure::Withheld)?;
    let (_, _, absent) = ask(app).await?;

    assert_eq!(
        record_of(&absent, "node-b-pub")?,
        record_of(&hidden, "node-b-pub")?,
        "Art 8: node B's record is the same whether it was excluded or does not know the patient"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_withheld_exclusion_reads_as_the_others_when_the_crossref_cannot_answer() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let scripted = (Crossref::Down, Denies(true));
    let (app, _state) = gateway_over((&a, &b), scripted, ConsentDisclosure::Withheld)?;

    let (status, _, text) = ask(app).await?;
    assert_eq!(
        StatusCode::FAILED_DEPENDENCY,
        status,
        "a resolution that could not answer fails the query: {text}"
    );
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-resolved"),
            ("node-b-pub", "not-resolved")
        ],
        statuses(&answer)
    );
    let error = |endpoint: &str| -> Result<_, Box<dyn Error>> {
        Ok(record_of(&text, endpoint)?.get("error").cloned())
    };
    assert_eq!(
        error("node-a-pub")?,
        error("node-b-pub")?,
        "Art 8: the excluded member carries the resolver's failure as every other member does"
    );
    assert!(!names_consent(&text), "{text}");
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn with_disclosure_the_exclusion_stays_consent_denied() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let scripted = (Crossref::Knows(BOTH), Denies(true));
    let (app, state) = gateway_over((&a, &b), scripted, ConsentDisclosure::Disclosed)?;

    let (status, _, text) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer),
        "N27a: the specification's report"
    );
    assert!(!answer.meta.federation.complete);
    assert!(wire(&b).await?.is_empty());
    assert_eq!(Some("1".to_owned()), prefilter_calls(&state, "denied")?);
    Ok(())
}

/// A node answering `GET /v1/ehr/{ehr_id}` with a synthetic `EHR`.
pub(super) async fn ehr_node(system: &str, ehr_id: &str) -> Server {
    let server = Server::start().await;
    let ehr = format!(
        r#"{{"system_id":{{"value":"{system}"}},"ehr_id":{{"value":"{ehr_id}"}},"time_created":{{"value":"2026-01-01T00:00:00Z"}}}}"#
    );
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{ehr_id}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(ehr.into_bytes(), "application/json"))
        .mount(&server)
        .await;
    server
}

/// `GET {base}/v1/ehr` for the patient.
pub(super) fn by_subject() -> Result<Request<Body>, http::Error> {
    Request::get(format!(
        "/v1/ehr?subject_id={PATIENT}&subject_namespace={NAMESPACE}"
    ))
    .header(header::ACCEPT, "application/json")
    .body(Body::empty())
}

/// The status, the code and the message of a read by subject through a
/// gateway over `a` and `b` under `scripted` and `disclosure`.
async fn read_by_subject(
    (a, b): (&Server, &Server),
    scripted: (Crossref, Denies),
    disclosure: ConsentDisclosure,
) -> Result<(StatusCode, String, String, String), Box<dyn Error>> {
    let (app, _state) = gateway_over((a, b), scripted, disclosure)?;
    let (status, text) = call(app, by_subject()?).await?;
    let error = error_body(&text)?;
    Ok((status, error.code, error.message, text))
}

// conformance: CP-36
#[tokio::test]
async fn a_read_by_subject_only_a_withheld_member_holds_answers_as_one_no_member_holds()
-> TestResult {
    let a = ehr_node("cdr-a.example.org", EHR_A).await;
    let b = ehr_node("cdr-b.example.org", EHR_B).await;
    let withheld = ConsentDisclosure::Withheld;

    let excluded = (Crossref::Knows(AT_B), Denies(true));
    let (status, code, message, text) = read_by_subject((&a, &b), excluded, withheld).await?;
    assert_eq!(StatusCode::NOT_FOUND, status, "RFC 9110 §15.5.5: {text}");
    assert_eq!("subject-unavailable", code);
    assert!(!names_consent(&text), "Art 8: {text}");
    assert!(!text.contains("node-b"), "no endpoint is named: {text}");
    assert!(!text.contains(PATIENT), "§5.4.3: {text}");
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");

    let nowhere = (Crossref::Knows(NOWHERE), Denies(false));
    let absent = read_by_subject((&a, &b), nowhere, withheld).await?;
    assert_eq!(
        (status, code, message),
        (absent.0, absent.1, absent.2),
        "Art 8: a restricted subject reads as one no member holds"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_read_by_subject_another_member_holds_is_served_with_the_exclusion_withheld() -> TestResult
{
    let a = ehr_node("cdr-a.example.org", EHR_A).await;
    let b = ehr_node("cdr-b.example.org", EHR_B).await;
    let scripted = (Crossref::Knows(BOTH), Denies(true));
    let (app, _state) = gateway_over((&a, &b), scripted, ConsentDisclosure::Withheld)?;

    let (status, text) = call(app, by_subject()?).await?;
    assert_eq!(
        StatusCode::OK,
        status,
        "the denied holder is no holder, so node A alone serves the read: {text}"
    );
    assert!(wire(&b).await?.is_empty(), "N27a: node B receives nothing");
    Ok(())
}

#[tokio::test]
async fn with_disclosure_a_read_by_subject_only_a_denied_member_holds_stays_consent_denied()
-> TestResult {
    let a = ehr_node("cdr-a.example.org", EHR_A).await;
    let b = ehr_node("cdr-b.example.org", EHR_B).await;
    let scripted = (Crossref::Knows(AT_B), Denies(true));
    let (status, code, _, _) =
        read_by_subject((&a, &b), scripted, ConsentDisclosure::Disclosed).await?;
    assert_eq!(
        (StatusCode::FORBIDDEN, "consent-denied"),
        (status, code.as_str())
    );

    let nowhere = (Crossref::Knows(NOWHERE), Denies(false));
    let (status, code, _, _) =
        read_by_subject((&a, &b), nowhere, ConsentDisclosure::Disclosed).await?;
    assert_eq!(
        (StatusCode::NOT_FOUND, "no-destination"),
        (status, code.as_str())
    );
    Ok(())
}

/// The `[[dev.consent_denied]]` row denying asking node B about the patient.
fn denied_at_b() -> String {
    format!(
        "\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-b\"\n"
    )
}

/// A development gateway over node A and node B resolving the patient at
/// both, node B denied by the static pre-filter, with `consent` as the
/// `[federation.consent]` table.
fn configured(
    dir: &std::path::Path,
    (a, b): (&Server, &Server),
    consent: &str,
) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(&a.uri(), &b.uri(), ""),
        "profile = \"development\"",
        &format!(
            "{consent}\n{}{}",
            crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
            denied_at_b()
        ),
    )
}

/// The `federation.consent` member of `OPTIONS {base}/`.
#[derive(Debug, Deserialize)]
struct Declared {
    federation: DeclaredFederation,
}

#[derive(Debug, Deserialize)]
struct DeclaredFederation {
    consent: DeclaredConsent,
}

#[derive(Debug, Deserialize)]
struct DeclaredConsent {
    disclose: bool,
}

/// The schema-validated `OPTIONS {base}/` body of `app`, with what it
/// declares of consent disclosure.
async fn declared(app: Router) -> Result<bool, Box<dyn Error>> {
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    Ok(serde_json::from_str::<Declared>(&text)?
        .federation
        .consent
        .disclose)
}

// conformance: CP-23 CP-36
#[tokio::test]
async fn the_configured_setting_withholds_the_exclusion_and_is_declared() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let consent = "[federation.consent]\ndisclose = false\n";
    let app = configured(dir.path(), (&a, &b), consent)?;

    assert!(
        !declared(app.clone()).await?,
        "§7a.2: the deployment says it withholds consent exclusions"
    );
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "not-resolved")],
        statuses(&answer)
    );
    assert!(!answer.meta.federation.complete, "N16, N37");
    assert!(!names_consent(&text), "{text}");
    assert!(wire(&b).await?.is_empty());
    Ok(())
}

// conformance: CP-23 CP-36
#[tokio::test]
async fn by_default_the_exclusion_is_disclosed_and_declared() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let dir = tempfile::tempdir()?;
    let app = configured(dir.path(), (&a, &b), "")?;

    assert!(declared(app.clone()).await?, "§7a.2: the default discloses");
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let answer: Answer = serde_json::from_str(&text)?;
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer),
        "N27a"
    );
    Ok(())
}

#[test]
fn an_unknown_key_in_the_consent_table_refuses_to_start_and_is_named() -> TestResult {
    let outcome = ferrofed_server::config::Config::from_sources(
        Some("[federation.consent]\nhide = true\n"),
        &BTreeMap::new(),
    );
    let Err(error) = outcome else {
        return Err("the configuration was accepted".into());
    };
    let message = error.to_string();
    assert!(
        message.contains("federation.consent.hide"),
        "the refusal names the key: {message}"
    );
    Ok(())
}

#[test]
fn the_consent_table_defaults_to_disclosure() -> TestResult {
    let settings =
        ferrofed_server::config::Config::from_sources(None, &BTreeMap::new())?.resolve()?;
    assert_eq!(
        ConsentDisclosure::Disclosed,
        settings.federation.consent_disclosure,
        "N27a: the default is the specification's consent-denied"
    );
    Ok(())
}
