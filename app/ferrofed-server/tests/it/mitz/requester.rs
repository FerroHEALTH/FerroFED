// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The requester of the closed authorization question: the verified caller,
//! read from the token's mapped claims (§3.2.4.2, §13.4). A token that names
//! none, or one the question does not take, asks Mitz nothing.

use std::error::Error;

use axum::Router;
use axum::body::Body;
use ferrofed_testkit::mitz::Mitz;
use http::{Request, StatusCode};
use serde::Deserialize;

use super::{
    CALLER, PREFILTER_CALLS, TestResult, URA_B, ask_as, consent_error, gateway_over, metered_over,
};
use crate::facade::{PATIENT, node_answering, statuses};
use crate::metrics::{count, parse};
use crate::support::{Logs, call};

#[tokio::test]
async fn the_question_names_the_verified_callers_professional_and_role() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, _) = ask_as(app, Some(("professional0042", "01.016"))).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let questions = mitz.questions().await;
    assert_eq!(2, questions.len());
    for question in &questions {
        assert!(
            question.contains(
                r#"<hl7:InstanceIdentifier root="2.16.528.1.1007.3.1" extension="professional0042"/>"#
            ),
            "§3.2.4.2, §13.4: the professional is the caller's, by UZI number"
        );
        assert!(
            question.contains(
                r#"<hl7:CodedValue code="01.016" codeSystem="2.16.840.1.113883.2.4.15.111"/>"#
            ),
            "the caller's own role"
        );
        assert!(
            question.contains(
                r#"<hl7:InstanceIdentifier root="2.16.528.1.1007.3.3" extension="ura-test-0100"/>"#
            ),
            "the caller's organisation by URA"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_token_without_the_requester_claims_asks_mitz_nothing_and_filters_no_one() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, answer) = ask_as(app, None).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(mitz.questions().await.is_empty(), "Mitz is never asked");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "N27: no signal, so each node checks consent itself"
    );
    assert_eq!(None, consent_error(&text)?, "no signal is no outage");
    Ok(())
}

#[tokio::test]
async fn a_token_without_the_requester_claims_is_counted_not_asked_for_the_caller_claims()
-> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    let dir = tempfile::tempdir()?;
    let (app, state) = metered_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    let (status, text, _) = ask_as(app, None).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    let samples = parse(&state.metrics().render()?)?;
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            PREFILTER_CALLS,
            &[("outcome", "not-asked"), ("reason", "caller-claims")]
        ),
        "Mitz was never asked, and the metric says why"
    );
    assert_eq!(
        None,
        count(&samples, PREFILTER_CALLS, &[("outcome", "no-signal")])
    );
    Ok(())
}

/// The state `GET /health/dependencies` reports of the consent pre-filter.
async fn consent_state(app: Router) -> Result<Option<String>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Report {
        consent: Option<String>,
    }
    let request = Request::get("/health/dependencies").body(Body::empty())?;
    let (status, text) = call(app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    Ok(serde_json::from_str::<Report>(&text)?.consent)
}

/// The professional number of a caller whose token states one the closed
/// authorization question does not take: a UZI number is alphanumeric.
const REFUSED_PROFESSIONAL: &str = "uzi-with-hyphens";

#[tokio::test]
async fn a_requester_the_question_does_not_take_is_not_asked_and_is_no_outage() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    mitz.deny(PATIENT, URA_B);
    let dir = tempfile::tempdir()?;
    let (app, state) = metered_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;
    let logs = Logs::default();
    let capture = ferrofed_server::telemetry::subscriber(
        ferrofed_server::telemetry::Rendering::Json,
        "trace",
        false,
        logs.clone(),
    )?;
    let guard = tracing::subscriber::set_default(capture);
    let (status, text, answer) =
        ask_as(app.clone(), Some((REFUSED_PROFESSIONAL, CALLER.1))).await?;
    drop(guard);

    assert_eq!(StatusCode::OK, status, "{text}");
    assert!(mitz.questions().await.is_empty(), "Mitz is never asked");
    assert_eq!(
        vec![("node-a-pub", "active"), ("node-b-pub", "active")],
        statuses(&answer),
        "N27: no signal, so each node checks consent itself"
    );
    assert_eq!(None, consent_error(&text)?, "not asking is no outage");
    assert_eq!(
        Some("unknown".to_owned()),
        consent_state(app).await?,
        "Mitz showed nothing of itself"
    );
    let rendered = state.metrics().render()?;
    let samples = parse(&rendered)?;
    assert_eq!(
        Some("1".to_owned()),
        count(
            &samples,
            PREFILTER_CALLS,
            &[
                ("outcome", "not-asked"),
                ("reason", "caller-claims-invalid")
            ]
        )
    );
    assert_eq!(
        None,
        count(&samples, PREFILTER_CALLS, &[("outcome", "unavailable")])
    );
    for (surface, shown) in [
        ("answer", text.as_str()),
        ("metrics", rendered.as_str()),
        ("logs", logs.text().as_str()),
    ] {
        assert!(
            !shown.contains(REFUSED_PROFESSIONAL),
            "no claim value reaches the {surface}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn two_callers_with_different_roles_send_different_subjects() -> TestResult {
    let a = node_answering("uid-at-a").await;
    let b = node_answering("uid-at-b").await;
    let mitz = Mitz::start().await;
    let dir = tempfile::tempdir()?;
    let app = gateway_over(dir.path(), (&a.uri(), &b.uri()), &mitz.endpoint())?;

    ask_as(app.clone(), Some(("professional0001", "01.015"))).await?;
    ask_as(app, Some(("professional0002", "30.000"))).await?;
    let questions = mitz.questions().await;
    assert_eq!(4, questions.len());
    let by = |professional: &str, role: &str| {
        questions
            .iter()
            .filter(|question| {
                question.contains(&format!("extension=\"{professional}\""))
                    && question.contains(&format!("code=\"{role}\""))
            })
            .count()
    };
    assert_eq!(2, by("professional0001", "01.015"), "the first caller");
    assert_eq!(2, by("professional0002", "30.000"), "the second caller");
    Ok(())
}
