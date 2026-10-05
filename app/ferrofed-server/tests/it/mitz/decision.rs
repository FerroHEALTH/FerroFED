// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the pre-filter decides for each candidate: a member Mitz permits is
//! asked, one it denies is `consent-denied`, and a Mitz that cannot answer
//! leaves the candidates to their nodes (Annex B §B.6, N27a, §13.2.1).

use axum::body::Body;
use ferrofed_testkit::mitz::Mitz;
use http::{Method, Request, StatusCode};
use serde::Deserialize;

use super::{TestResult, URA_A, URA_B, ask, consent_error, gateway_over};
use crate::facade::{PATIENT, PATIENT_TAIL, node_answering, received, schema, statuses, wire};
use crate::support::{Logs, call};

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
    assert!(
        text.contains(r#""consent_ms":1000"#),
        "§11.5: the pre-filter's budget is declared with the others: {text}"
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
