// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A pre-filter that did not ask its service, under `[federation.consent]
//! disclose = false`: the client reads exactly the answer of a pre-filter
//! that was asked and denied nothing, so whether the service covered the
//! request is not visible to the client (Regulation (EU) 2025/327 Art 8),
//! while the operator still counts the call by its reason.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use ferrofed_identity::role::consent::{ConsentDecision, ConsentPrefilter, NotAsked, Requester};
use ferrofed_identity::role::patient::PatientRef;
use ferrofed_registry::id::NodeId;
use ferrofed_server::config::settings::ConsentDisclosure;
use http::StatusCode;

use super::{
    BOTH, Crossref, PREFILTER_CALLS, TestResult, ask, gateway_over, names_consent, record_of,
};
use crate::facade::{node_answering, wire};
use crate::metrics::{count, parse};

/// A consent pre-filter that does not ask its service, for the reason it
/// holds, or that is asked and carries no signal.
#[derive(Debug, Clone, Copy)]
struct Skips(Option<NotAsked>);

#[async_trait]
impl ConsentPrefilter for Skips {
    async fn prefilter(
        &self,
        _patient: &PatientRef,
        _requester: Option<&Requester>,
        _candidates: &[NodeId],
        _deadline: Instant,
    ) -> ConsentDecision {
        self.0
            .map_or(ConsentDecision::NoSignal, ConsentDecision::NotAsked)
    }

    fn mode(&self) -> &'static str {
        "test-scripted"
    }

    fn budget(&self) -> Option<Duration> {
        None
    }
}

/// The answer `text` with each endpoint record's `latency_ms` taken out,
/// the one member two runs of one query can differ in.
fn steady(text: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut answer: serde_json::Value = serde_json::from_str(text)?;
    let endpoints = answer
        .pointer_mut("/meta/federation/endpoints")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("the answer reports its endpoints")?;
    for record in endpoints {
        if let Some(record) = record.as_object_mut() {
            record.remove("latency_ms");
        }
    }
    Ok(answer)
}

/// The header names of `headers`, each `name: value`, sorted.
fn names(headers: &[String]) -> Vec<&str> {
    let mut names: Vec<&str> = headers
        .iter()
        .filter_map(|line| line.split_once(':').map(|(name, _)| name))
        .collect();
    names.sort_unstable();
    names
}

// conformance: CP-36
#[tokio::test]
async fn a_withheld_deployment_answers_a_prefilter_that_did_not_ask_as_one_that_found_nothing()
-> TestResult {
    for reason in [
        NotAsked::Namespace,
        NotAsked::CallerClaims,
        NotAsked::CallerClaimsInvalid,
        NotAsked::PatientValue,
    ] {
        let a = node_answering("uid-at-a").await;
        let b = node_answering("uid-at-b").await;
        let skipped = (Crossref::Knows(BOTH), Skips(Some(reason)));
        let (app, state) = gateway_over((&a, &b), skipped, ConsentDisclosure::Withheld)?;
        let (status, headers, text) = ask(app).await?;

        let asked = (Crossref::Knows(BOTH), Skips(None));
        let (app, _state) = gateway_over((&a, &b), asked, ConsentDisclosure::Withheld)?;
        let (asked_status, asked_headers, asked_text) = ask(app).await?;

        assert_eq!(StatusCode::OK, status, "{reason:?}: {text}");
        assert_eq!(asked_status, status, "{reason:?}");
        assert_eq!(
            steady(&asked_text)?,
            steady(&text)?,
            "Art 8: {reason:?}: the client reads the answer of a pre-filter that found nothing"
        );
        assert_eq!(names(&asked_headers), names(&headers), "{reason:?}");
        assert!(!names_consent(&text), "{reason:?}: {text}");
        assert_eq!(
            Some(&serde_json::Value::from("active")),
            record_of(&text, "node-b-pub")?.get("status"),
            "{reason:?}"
        );
        assert!(
            !wire(&b).await?.is_empty(),
            "N27: {reason:?}: node B is asked and checks consent itself"
        );
        let samples = parse(&state.metrics().render()?)?;
        assert_eq!(
            Some("1".to_owned()),
            count(
                &samples,
                PREFILTER_CALLS,
                &[("outcome", "not-asked"), ("reason", reason.as_str())]
            ),
            "{reason:?}: the operator still counts the call"
        );
    }
    Ok(())
}
