// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Dutch binding's consent pre-filter, `[nl_gf.mitz]`: the closed
//! authorization question asked of the stub Mitz for each candidate's data
//! holder before resolution (Annex B §B.6, N27a, §13.2.1).
//!
//! A member whose holder Mitz denies is `consent-denied`, never asked, and
//! clears `complete` while the query succeeds (§11.1, §11.3, N37). A member
//! Mitz permits is asked, and its node checks consent itself (N27, §14.3).
//! A Mitz that cannot answer leaves the candidates to their nodes, the
//! pre-filter's declared policy. In process: two mock CDRs, the development
//! cross-reference, and the stub Mitz; every value is synthetic, the patient
//! in the `urn:oid:2.999` example arc, which `[nl_gf.mitz] namespaces` lists
//! as standing for the BSN.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::path::Path;

use axum::Router;
use axum::body::Body;
use ferrofed_identity::mitz::MitzConfigError;
use ferrofed_server::config::settings::Settings;
use ferrofed_server::config::{Config, error, transport};
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;
use ferrofed_testkit::mitz::Mitz;
use http::{Method, Request, StatusCode};
use serde::Deserialize;

use crate::facade::{
    Answer, EHR_A, EHR_B, NAMESPACE, PATIENT, PATIENT_TAIL, body, crossref, gateway,
    node_answering, patient_query, post, received, registry, schema, statuses, wire,
};
use crate::support::{Logs, call};

type TestResult = Result<(), Box<dyn Error>>;

/// The URA of node A's and node B's care provider.
const URA_A: &str = "ura-test-0001";
const URA_B: &str = "ura-test-0002";

/// The `[nl_gf.mitz]` table asking the Mitz at `endpoint`, with `holders`
/// as its holders table body.
fn mitz_table(endpoint: &str, holders: &str) -> String {
    format!(
        "\n[nl_gf.mitz]\nurl = \"{endpoint}\"\nnamespaces = [\"{NAMESPACE}\"]\npurpose = \"TREAT\"\ndata_categories = [\"GGC002\"]\ntimeout_ms = 1000\n\n[nl_gf.mitz.data_user]\nura = \"ura-test-0100\"\ntype = \"V6\"\nresponsible_root = \"2.999.10\"\nresponsible = \"professional0001\"\nrole = \"01.015\"\n\n[nl_gf.mitz.holders]\n{holders}"
    )
}

/// The holders of node A and node B, each with its URA.
fn holders() -> String {
    format!(
        "\"node-a\" = {{ type = \"V6\", ura = \"{URA_A}\" }}\n\"node-b\" = {{ type = \"V6\", ura = \"{URA_B}\" }}\n"
    )
}

/// The development gateway over node A and node B, resolving the patient at
/// both, with the Mitz pre-filter asking `mitz`.
fn gateway_over(dir: &Path, (a, b): (&str, &str), mitz: &str) -> Result<Router, Box<dyn Error>> {
    gateway(
        dir,
        &registry(a, b, ""),
        "profile = \"development\"",
        &format!(
            "{}{}",
            crossref(&[("node-a", EHR_A), ("node-b", EHR_B)]),
            mitz_table(mitz, &holders())
        ),
    )
}

/// Runs the patient query, checks the answer against the schema, and
/// returns its status, its text and its typed read.
async fn ask(app: Router) -> Result<(StatusCode, String, Answer), Box<dyn Error>> {
    let (status, text) = call(app, post(body(&patient_query())?)?).await?;
    schema::validate(&text)?;
    let answer: Answer = serde_json::from_str(&text)?;
    Ok((status, text, answer))
}

/// The `meta.federation.consent.error` of an answer, when present.
fn consent_error(text: &str) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct WithConsent {
        meta: ConsentMeta,
    }
    #[derive(Deserialize)]
    struct ConsentMeta {
        federation: ConsentFederation,
    }
    #[derive(Deserialize)]
    struct ConsentFederation {
        consent: Option<ConsentReport>,
    }
    #[derive(Deserialize)]
    struct ConsentReport {
        error: String,
    }
    let read: WithConsent = serde_json::from_str(text)?;
    Ok(read.meta.federation.consent.map(|consent| consent.error))
}

// conformance: CP-36
#[tokio::test]
async fn a_member_mitz_permits_is_asked_and_its_node_still_decides() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer)
    );
    assert!(answer.meta.federation.complete, "N37");
    assert_eq!(None, consent_error(&text)?);
    for node in [&a, &b] {
        assert_eq!(1, received(node).await?.len(), "N27: the node decides");
    }
    let questions = mitz.questions().await;
    assert_eq!(2, questions.len(), "one question per data holder");
    for ura in [URA_A, URA_B] {
        assert!(
            questions.iter().any(|question| question.contains(ura)),
            "Mitz is asked about {ura}"
        );
    }
    assert!(
        questions.iter().all(|question| question.contains(PATIENT)),
        "Annex B §B.6: Mitz is asked by the BSN"
    );
    Ok(())
}

// conformance: CP-36
#[tokio::test]
async fn a_member_mitz_denies_is_consent_denied_and_never_asked() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "§11.3, N37: {text}");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "consent-denied")],
        statuses(&answer),
        "N27a, §11.1: the member Mitz denies is reported"
    );
    assert!(!answer.meta.federation.complete, "§11.3 clears complete");
    assert_eq!(None, consent_error(&text)?, "Mitz answered");
    assert!(wire(&b).await?.is_empty(), "node B receives nothing");
    assert_eq!(1, received(&a).await?.len());
    Ok(())
}

/// Asks through a Mitz that `outage` makes unable to answer, and holds the
/// answer to the pre-filter's declared policy: every candidate is asked, the
/// query succeeds, and the failure is carried in `consent.error`.
async fn through_an_outage(outage: impl FnOnce(&Mitz)) -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    outage(&mitz);
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "N27a, §13.2.1: no consent signal, so each node is the sole gate"
    );
    assert!(
        answer.meta.federation.complete,
        "N37: every member answered"
    );
    for node in [&a, &b] {
        assert_eq!(1, received(node).await?.len(), "each node is asked once");
    }
    let carried = consent_error(&text)?.ok_or("consent.error names the outage")?;
    assert!(
        carried.starts_with("the consent pre-filter could not answer"),
        "{carried}"
    );
    assert!(!carried.contains(PATIENT_TAIL), "{carried}");
    Ok(())
}

#[tokio::test]
async fn a_mitz_answering_503_leaves_every_candidate_to_its_node() -> TestResult {
    through_an_outage(Mitz::refuse).await
}

#[tokio::test]
async fn a_silent_mitz_leaves_every_candidate_to_its_node() -> TestResult {
    through_an_outage(Mitz::go_silent).await
}

#[tokio::test]
async fn an_indeterminate_mitz_leaves_every_candidate_to_its_node() -> TestResult {
    through_an_outage(Mitz::answer_indeterminate).await
}

// conformance: CP-36
#[tokio::test]
async fn a_denial_and_a_failure_together_deny_one_and_leave_the_other_to_its_node() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT, URA_A);
    mitz.refuse_at(URA_B);
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, answer) = ask(app).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        vec![("node-a-pub", "consent-denied"), ("node-b-pub", "active")],
        statuses(&answer)
    );
    assert!(!answer.meta.federation.complete);
    assert!(wire(&a).await?.is_empty(), "node A receives nothing");
    assert_eq!(1, received(&b).await?.len(), "node B is left to its node");
    assert!(
        consent_error(&text)?.is_some(),
        "the failure at node B's holder is carried"
    );
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn the_bsn_reaches_mitz_only_and_no_node_log_or_error() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text, _) = ask(app.clone()).await?;
    let failing = Mitz::start().await;
    failing.refuse();
    let other = tempfile::tempdir()?;
    let refused = gateway_over(other.path(), (&a.uri(), &b.uri()), &failing.endpoint())?;
    let (_, refused_text, _) = ask(refused).await?;
    drop(guard);

    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(
        mitz.questions().await.iter().all(|q| q.contains(PATIENT)),
        "Mitz is asked by the patient's identifier"
    );
    for node in [&a, &b] {
        let seen = wire(node).await?;
        assert!(!seen.contains(PATIENT_TAIL), "§5.4.1, N33: {seen}");
    }
    let logged = logs.text();
    assert!(!logged.is_empty(), "the capture recorded the requests");
    assert!(!logged.contains(PATIENT_TAIL), "no log line names it");
    let carried = consent_error(&refused_text)?.ok_or("the outage is carried")?;
    assert!(!carried.contains(PATIENT_TAIL), "{carried}");
    Ok(())
}

#[tokio::test]
async fn the_prefilter_is_declared_in_options_as_mitz() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/")
        .body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    schema::validate_options(&text)?;
    let declared: OptionsConsent = serde_json::from_str(&text)?;
    let consent = declared.federation.consent;
    assert_eq!(
        ("nl-gf-mitz", "pass-to-node"),
        (consent.prefilter.as_str(), consent.on_unavailable.as_str())
    );
    assert!(mitz.questions().await.is_empty(), "OPTIONS asks nothing");
    Ok(())
}

/// The `federation.consent` member of `OPTIONS {base}/`.
#[derive(Debug, Deserialize)]
struct OptionsConsent {
    federation: OptionsFederation,
}

#[derive(Debug, Deserialize)]
struct OptionsFederation {
    consent: Declared,
}

#[derive(Debug, Deserialize)]
struct Declared {
    prefilter: String,
    on_unavailable: String,
}

/// The configuration of the gateway under `profile`, with `mitz` as the Mitz
/// table, its registry document written into `dir`.
fn configuration(dir: &Path, profile: &str, mitz: &str) -> Result<String, Box<dyn Error>> {
    let document = dir.join("registry.toml");
    std::fs::write(
        &document,
        registry("https://a.example.org", "https://b.example.org", ""),
    )?;
    let document = toml::Value::String(document.display().to_string());
    Ok(crate::support::signed(&format!(
        "profile = \"{profile}\"\n\n[registry]\ndocument = {document}\n\n[federation]\nnode_selection = \"ask-all\"\nid = \"example-federation\"\n{mitz}"
    )))
}

/// The settings `text` resolves to.
fn resolved(text: &str) -> Result<Settings, error::Error> {
    Config::from_sources(Some(text), &BTreeMap::new())?.resolve()
}

/// The production configuration whose Mitz table is the standard one with
/// `edit` applied.
fn edited(edit: impl Fn(String) -> String) -> Result<(tempfile::TempDir, String), Box<dyn Error>> {
    let dir = tempfile::tempdir()?;
    let table = edit(mitz_table("https://mitz.example.org/vraag", &holders()));
    let text = configuration(dir.path(), "production", &table)?;
    Ok((dir, text))
}

#[test]
fn a_mitz_over_plain_http_is_refused_outside_development() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("https://", "http://"))?;
    match resolved(&text) {
        Err(error::Error::Cleartext(refused)) => {
            assert_eq!("nl_gf.mitz.url", refused.site.url_key);
            Ok(())
        }
        other => Err(format!("the BSN never travels in clear text: {other:?}").into()),
    }
}

#[test]
fn a_mitz_over_plain_http_is_named_under_development() -> TestResult {
    let dir = tempfile::tempdir()?;
    let table = mitz_table("http://mitz.example.org/vraag", &holders());
    let settings = resolved(&configuration(dir.path(), "development", &table)?)?;
    let sites: Vec<String> = transport::check(&settings, None)?
        .into_iter()
        .map(|site| format!("{}: {}", site.url_key, site.payload))
        .collect();
    assert!(
        sites.contains(&"nl_gf.mitz.url: the patient identifiers asked of nl_gf.mitz".to_owned()),
        "{sites:?}"
    );
    Ok(())
}

#[test]
fn a_credential_in_the_mitz_url_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("https://", "https://user:Qz7secret@"))?;
    match resolved(&text) {
        Err(refused @ error::Error::UrlCredentials { .. }) => {
            assert!(!refused.to_string().contains("Qz7secret"), "{refused}");
            Ok(())
        }
        other => Err(format!("a credential goes in its own section: {other:?}").into()),
    }
}

#[test]
fn a_purpose_other_than_treat_or_coc_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("\"TREAT\"", "\"ETREAT\""))?;
    match resolved(&text) {
        Err(error::Error::Mitz { key, .. }) => {
            assert_eq!("nl_gf.mitz.purpose", key);
            Ok(())
        }
        other => Err(format!("§3.2.4.2 takes TREAT or COC: {other:?}").into()),
    }
}

// conformance: CP-26
#[test]
fn the_pseudonym_listed_as_the_bsn_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| {
        table.replace(
            "namespaces = [\"",
            "namespaces = [\"http://fhir.nl/fhir/NamingSystem/pseudo-bsn\", \"",
        )
    })?;
    match resolved(&text) {
        Err(error::Error::Mitz { key, .. }) => {
            assert_eq!("nl_gf.mitz.namespaces", key);
            Ok(())
        }
        other => Err(format!("a pseudonym never reaches Mitz as a BSN: {other:?}").into()),
    }
}

#[test]
fn a_data_user_without_a_role_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("role = \"01.015\"\n", ""))?;
    match resolved(&text) {
        Err(error::Error::Missing { key }) => {
            assert_eq!("nl_gf.mitz.data_user.role", key);
            Ok(())
        }
        other => Err(format!("§3.2.4.2 requires the role: {other:?}").into()),
    }
}

#[test]
fn no_data_category_is_refused() -> TestResult {
    let (_dir, text) = edited(|table| table.replace("[\"GGC002\"]", "[]"))?;
    match resolved(&text) {
        Err(error::Error::Missing { key }) => {
            assert_eq!("nl_gf.mitz.data_categories", key);
            Ok(())
        }
        other => Err(format!("a question asks about a category: {other:?}").into()),
    }
}

#[test]
fn a_member_without_a_holder_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| {
        let cut = format!("\"node-b\" = {{ type = \"V6\", ura = \"{URA_B}\" }}\n");
        table.replace(&cut, "")
    })?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::NoHolder(member))) => {
            assert_eq!("node-b", member.as_str());
            Ok(())
        }
        other => Err(format!("Mitz could never be asked about node-b: {other:?}").into()),
    }
}

#[test]
fn a_holder_with_no_ura_anywhere_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| table.replace(&format!(", ura = \"{URA_B}\""), ""))?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::NoUra(member))) => {
            assert_eq!("node-b", member.as_str());
            Ok(())
        }
        other => Err(format!("the data holder needs a URA: {other:?}").into()),
    }
}

#[test]
fn a_holder_naming_no_member_refuses_to_boot() -> TestResult {
    let (_dir, text) = edited(|table| {
        format!("{table}\"node-z\" = {{ type = \"V6\", ura = \"ura-test-0009\" }}\n")
    })?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::Mitz(MitzConfigError::UnknownMember(member))) => {
            assert_eq!("node-z", member.as_str());
            Ok(())
        }
        other => Err(format!("a holder names a registry member: {other:?}").into()),
    }
}

#[test]
fn development_consent_rows_and_mitz_together_refuse_to_boot() -> TestResult {
    let dir = tempfile::tempdir()?;
    let rows = format!(
        "\n[[dev.consent_denied]]\nnamespace = \"{NAMESPACE}\"\nvalue = \"{PATIENT}\"\nmember = \"node-b\"\n"
    );
    let tables = format!(
        "{}{rows}{}",
        crossref(&[("node-a", EHR_A)]),
        mitz_table("https://mitz.example.org/vraag", &holders())
    );
    let text = configuration(dir.path(), "development", &tables)?;
    match Federation::load(&resolved(&text)?) {
        Err(FederationError::TwoConsentPrefilters) => Ok(()),
        other => Err(format!("N27a: at most one pre-filter is active: {other:?}").into()),
    }
}
