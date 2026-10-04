// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! An XCPD discovery whose audit message is still being stored when the
//! localization budget runs out fails closed under ask-all too: ask-all
//! covers a localizer outage, never an exchange the gateway could not audit
//! (§14.1, N4; ITI TF-2 §3.55.5.1; CP-5).
//!
//! In process: two mock CDRs, the development cross-reference resolving the
//! patient at both, the testkit's stub responding gateway naming node A's
//! community, and an XCPD localizer whose audit trail stalls as a spool on a
//! disk that does not answer does.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_identity::localizer::OnFailure;
use ferrofed_identity::xcpd::{GatewayConfig, Tls, Transport, XcpdConfig, XcpdLocalizer};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::localization::{LocalizationPolicy, XCPD};
use ferrofed_server::state::AppState;
use ferrofed_testkit::xcpd::{Answer, Community, RespondingGateway};
use http::StatusCode;
use ihe_iti::xcpd::audit::{AuditError, AuditEvent, AuditRecorder};
use openehr_federation::outcome::ErrorDetail;

use crate::facade::{
    Answer as Federated, PATIENT, body, crossref, node_answering, patient_query, post, received,
    registry, settings_with_room, statuses, wire,
};
use crate::support::{SLACK, call};

type TestResult = Result<(), Box<dyn Error>>;

/// The localization budget the gateway declares.
const BUDGET: Duration = Duration::from_millis(1_000);

/// The community node A serves.
const COMMUNITY_A: &str = "2.999.50";

/// An audit trail whose spool never stores: the disk under it has stalled.
struct Stalled;

#[async_trait::async_trait]
impl AuditRecorder for Stalled {
    async fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        std::future::pending().await
    }
}

// conformance: CP-5
#[tokio::test]
async fn a_discovery_still_recording_its_audit_at_the_budget_fails_closed_under_ask_all()
-> TestResult {
    let node_a = node_answering("a-uid::node-a.example.org::1").await;
    let node_b = node_answering("b-uid::node-b.example.org::1").await;
    let holding = RespondingGateway::answering(Answer::Holds(vec![Community::new(
        COMMUNITY_A,
        "2.999.50.2",
        "PID-SYNTH-A",
    )]))
    .await;
    let dir = tempfile::tempdir()?;
    let document = dir.path().join("registry.toml");
    std::fs::write(&document, registry(&node_a.uri(), &node_b.uri(), ""))?;
    let text = format!(
        "profile = \"development\"\n\n[registry]\ndocument = {document}\n\n[federation]\nper_node_timeout_ms = 2000\noverall_timeout_ms = 3000\nnode_selection = \"localized\"\nid = \"example-federation\"\n\n[federation.localization]\ntimeout_ms = {budget}\n\n{rows}",
        document = toml::Value::String(document.display().to_string()),
        budget = BUDGET.as_millis(),
        rows = crossref(&[
            ("node-a", "7a7a7a7a-7a7a-4a7a-8a7a-7a7a7a7a7a7a"),
            ("node-b", "7b7b7b7b-7b7b-4b7b-8b7b-7b7b7b7b7b7b"),
        ]),
    );
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let federation = Federation::load(&settings)?.ok_or("a registry is configured")?;
    let localizer = XcpdLocalizer::from_config(
        XcpdConfig {
            sender_device: "2.999.40.1".to_owned(),
            home_community: Some("2.999.40".to_owned()),
            gateways: vec![GatewayConfig {
                endpoint: SecretUrl::new(holding.endpoint().as_str()),
                device: "2.999.50.1".to_owned(),
                community: None,
            }],
            communities: BTreeMap::from([
                (COMMUNITY_A.to_owned(), "node-a".parse()?),
                ("2.999.60".to_owned(), "node-b".parse()?),
            ]),
            namespaces: BTreeMap::new(),
            transport: Transport::UnencryptedForDevelopment,
            tls: Tls::default(),
        },
        None,
        federation.snapshot(),
    )?
    .audited(Arc::new(Stalled));
    let federation = federation.with_localization(LocalizationPolicy::new(
        Arc::new(localizer),
        XCPD,
        OnFailure::AskAll,
        BUDGET,
    ));
    let app = ferrofed_server::router(
        Arc::new(AppState::with_federation(federation)),
        &settings_with_room(),
    );

    let asked = Instant::now();
    let (status, text_body) = call(app, post(body(&patient_query())?)?).await?;
    assert!(
        asked.elapsed() < BUDGET + SLACK,
        "the localization budget bounds the wait: {:?}",
        asked.elapsed()
    );
    assert_eq!(StatusCode::OK, status, "{text_body}");
    let answer: Federated = serde_json::from_str(&text_body)?;
    assert_eq!(
        vec![
            ("node-a-pub", "not-localized"),
            ("node-b-pub", "not-localized")
        ],
        statuses(&answer),
        "an exchange not audited in time is never widened to ask-all (§14.1, ITI TF-2 §3.55.5.1)"
    );
    for endpoint in &answer.meta.federation.endpoints {
        let message = match endpoint.error.as_ref() {
            Some(ErrorDetail::Text(message)) => message.as_str(),
            _ => "",
        };
        assert!(
            message.contains("could not be audited")
                && message.contains("audit record was not stored"),
            "every member names the audit failure: {message:?}"
        );
    }
    assert!(
        received(&node_a).await?.is_empty() && received(&node_b).await?.is_empty(),
        "no member is asked"
    );
    assert_eq!(1, holding.requests().await.len(), "the discovery was made");
    for node in [&node_a, &node_b] {
        assert!(!wire(node).await?.contains(PATIENT), "N33");
    }
    Ok(())
}
