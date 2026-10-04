// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A caller confined to one patient who names another (§5.2, §14.1): the
//! named patient is resolved at the bound member alone, and when it is not
//! the token's own patient the request is refused `403` before any localizer,
//! consent pre-filter or other member is asked about it, so the gateway
//! learns nothing of that patient elsewhere. The harness records every call
//! its localizer, pre-filter and resolver receive. No specification defines
//! a patient grant across nodes, so this is FerroFED's own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use axum::Router;
use axum::body::Body;
use ferrofed_engine::dispatch::NodeClients;
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::consent::{ConsentDecision, ConsentPrefilter};
use ferrofed_identity::localizer::{Localization, Localizer, OnFailure};
use ferrofed_identity::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::resolver::{Resolution, Resolver};
use ferrofed_registry::id::{EhrId, EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_server::config::auth::{AuthSettings, PatientBinding};
use ferrofed_server::federation::Federation;
use ferrofed_server::localization::LocalizationPolicy;
use ferrofed_server::state::AppState;
use ferrofed_testkit::mock::Server;
use http::{Request, StatusCode, header};
use openehr_federation::aql::{Context, Targeting};
use openehr_federation::id::FederationId;
use openehr_its::rest::client::ReqwestTransport;

use super::{TestResult, bearing, claims, minted, sent};
use crate::facade::{
    EHR_A, EHR_B, NAMESPACE, PATIENT, body, node_answering, post, registry, settings_with_room,
};
use crate::support::{self, asked, error_body};

/// The identifier system of node A's `ehr_id`s at the cross-reference.
const EHR_SYSTEM: &str = "urn:oid:2.999.9.1";

/// Another synthetic patient, and its `ehr_id` at node A and at node B.
const OTHER: &str = "SENTINEL-OTHER-71xw";
const OTHER_A: &str = "4444dddd-4444-4444-8444-444444444444";
const OTHER_B: &str = "5555eeee-5555-4555-8555-555555555555";

/// The synthetic issuing namespace of [`OTHER`], under the example OID arc,
/// which tells the harness the two patients apart.
const OTHER_NAMESPACE: &str = "urn:oid:2.999.2";

/// Every call one seam received: the patient's namespace and the members
/// asked.
type Calls = Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// Records `patient` and `members` in `calls`.
fn record(calls: &Calls, patient: &PatientRef, members: &[NodeId]) {
    if let Ok(mut held) = calls.lock() {
        held.push((
            patient.namespace().as_str().to_owned(),
            members
                .iter()
                .map(|member| member.as_str().to_owned())
                .collect(),
        ));
    }
}

/// The calls `calls` holds about a patient in `namespace`.
fn about(calls: &Calls, namespace: &str) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let held = calls.lock().map_err(|_poisoned| "the calls lock")?;
    Ok(held
        .iter()
        .filter(|(recorded, _)| recorded == namespace)
        .map(|(_, members)| members.clone())
        .collect())
}

/// A cross-reference that knows the token's patient (by node A's `ehr_id`
/// and by [`PATIENT`]) and [`OTHER`] at both nodes, recording every call.
struct Recording(Calls);

#[async_trait]
impl Resolver for Recording {
    async fn resolve(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _deadline: Instant,
    ) -> BTreeMap<NodeId, Resolution> {
        record(&self.0, patient, members);
        let (at_a, at_b) = if patient.namespace().as_str() == OTHER_NAMESPACE {
            (OTHER_A, OTHER_B)
        } else {
            (EHR_A, EHR_B)
        };
        members
            .iter()
            .filter_map(|member| {
                let ehr_id = if member.as_str() == "node-a" {
                    at_a
                } else {
                    at_b
                };
                Some((
                    member.clone(),
                    Resolution::Resolved(EhrId::new(ehr_id).ok()?),
                ))
            })
            .collect()
    }
}

/// A localizer naming every member, recording every call.
struct Locating(Calls);

#[async_trait]
impl Localizer for Locating {
    async fn localize(
        &self,
        patient: &PatientRef,
        members: &[NodeId],
        _deadline: Instant,
    ) -> Localization {
        record(&self.0, patient, members);
        Localization::Candidates(members.iter().cloned().collect::<BTreeSet<_>>())
    }
}

/// A consent pre-filter with no signal, counting every call.
struct Prefiltering(Arc<AtomicUsize>);

#[async_trait]
impl ConsentPrefilter for Prefiltering {
    async fn prefilter(
        &self,
        _patient: &PatientRef,
        _requester: Option<&ferrofed_identity::consent::Requester>,
        _candidates: &[NodeId],
        _deadline: Instant,
    ) -> ConsentDecision {
        self.0.fetch_add(1, Ordering::SeqCst);
        ConsentDecision::NoSignal
    }

    fn mode(&self) -> &'static str {
        "test-recording"
    }

    fn budget(&self) -> Option<Duration> {
        None
    }
}

/// The gateway and every record its seams keep.
struct Harness {
    app: Router,
    a: Server,
    b: Server,
    resolved: Calls,
    localized: Calls,
    prefiltered: Arc<AtomicUsize>,
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

/// A gateway over node A and node B with a recording cross-reference,
/// localizer and consent pre-filter, its issuer bound to node A.
async fn harness() -> Result<Harness, Box<dyn Error>> {
    let a = node_answering("8849182c-82ad-4088-a07f-48ead4180515::node-a::1").await;
    let b = node_answering("6cb19121-4307-4a29-9c1c-b6d6a2ab3b77::node-b::1").await;
    let snapshot = RegistrySnapshot::from_toml_str(&registry(&a.uri(), &b.uri(), ""))?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(5))?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?;
    let (resolved, localized) = (Calls::default(), Calls::default());
    let prefiltered = Arc::new(AtomicUsize::new(0));
    let federation = Federation::new(
        FederationId::new("example-federation")?,
        snapshot,
        clients,
        Some(Arc::new(Recording(Arc::clone(&resolved)))),
        Context::new(Targeting::AskAll),
        Budget::new(Duration::from_secs(2), Duration::from_secs(3))?,
    )
    .with_consent_prefilter(Arc::new(Prefiltering(Arc::clone(&prefiltered))))
    .with_localization(LocalizationPolicy::new(
        Arc::new(Locating(Arc::clone(&localized))),
        "test-recording",
        OnFailure::Closed,
        Duration::from_millis(500),
    ))
    .with_signer(support::signer("example-federation")?);
    let mut settings = settings_with_room();
    settings.auth = bound()?;
    let app = ferrofed_server::router(Arc::new(AppState::with_federation(federation)), &settings);
    Ok(Harness {
        app,
        a,
        b,
        resolved,
        localized,
        prefiltered,
    })
}

/// `request` bearing the token of the patient whose `ehrId` at node A is
/// [`EHR_A`].
fn as_the_patient(request: Request<Body>) -> Result<Request<Body>, Box<dyn Error>> {
    let mut granted = claims();
    granted.scope = Some("patient/aql-*.s patient/composition-*.r".to_owned());
    granted.ehr_id = Some(EHR_A.to_owned());
    bearing(request, &minted(&granted)?)
}

/// The federated query for `patient` in `namespace`.
fn query_for(patient: &str, namespace: &str) -> Result<Request<Body>, Box<dyn Error>> {
    let aql = format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
         WHERE e/ehr_status/subject/external_ref/id/value = '{patient}' \
         AND e/ehr_status/subject/external_ref/namespace = '{namespace}'"
    );
    Ok(post(body(&aql)?)?)
}

/// The read of the EHR of `patient` in `namespace` by subject.
fn read_of(patient: &str, namespace: &str) -> Result<Request<Body>, Box<dyn Error>> {
    Ok(Request::get(format!(
        "/v1/ehr?subject_id={patient}&subject_namespace={namespace}"
    ))
    .header(header::ACCEPT, "application/json")
    .body(Body::empty())?)
}

/// Sends `request` through `harness` and asserts it is refused `403`
/// `patient-confinement`, with [`OTHER`] resolved at node A alone and
/// neither the localizer, the pre-filter nor any node asked.
async fn assert_learns_nothing(harness: &Harness, request: Request<Body>) -> TestResult {
    let (status, _, text) = sent(&harness.app, as_the_patient(request)?).await?;
    assert_eq!(StatusCode::FORBIDDEN, status, "{text}");
    assert_eq!("patient-confinement", error_body(&text)?.code, "{text}");
    assert_eq!(
        vec![vec!["node-a".to_owned()]],
        about(&harness.resolved, OTHER_NAMESPACE)?,
        "the other patient is resolved at the bound member alone"
    );
    assert!(
        about(&harness.localized, OTHER_NAMESPACE)?.is_empty(),
        "no localizer is asked about the other patient"
    );
    assert_eq!(
        0,
        harness.prefiltered.load(Ordering::SeqCst),
        "no consent pre-filter is asked"
    );
    assert!(
        asked(&harness.a).await?.is_empty(),
        "node A received nothing"
    );
    assert!(
        asked(&harness.b).await?.is_empty(),
        "node B received nothing"
    );
    Ok(())
}

// NOTE: §5.2, §14.1: the named patient is checked at the bound member before localization,
// consent or any other member, so the gateway learns nothing of another patient there.
// conformance: CP-17
#[tokio::test]
async fn a_query_for_another_patient_asks_the_bound_member_alone() -> TestResult {
    let harness = harness().await?;
    assert_learns_nothing(&harness, query_for(OTHER, OTHER_NAMESPACE)?).await
}

// NOTE: §5.2, §14.1: a read by subject of another patient is checked the same way.
// conformance: CP-17
#[tokio::test]
async fn a_read_by_subject_of_another_patient_asks_the_bound_member_alone() -> TestResult {
    let harness = harness().await?;
    assert_learns_nothing(&harness, read_of(OTHER, OTHER_NAMESPACE)?).await
}

// NOTE: §14.1: the token's own patient goes on to localization and consent as any query does,
// which shows the harness records those calls.
// conformance: CP-17
#[tokio::test]
async fn the_token_s_own_patient_is_localized_and_prefiltered() -> TestResult {
    let harness = harness().await?;
    let request = as_the_patient(query_for(PATIENT, NAMESPACE)?)?;
    let (status, _, text) = sent(&harness.app, request).await?;
    assert_eq!(StatusCode::OK, status, "{text}");
    assert_eq!(
        1,
        about(&harness.localized, NAMESPACE)?.len(),
        "localized once"
    );
    assert_eq!(
        1,
        harness.prefiltered.load(Ordering::SeqCst),
        "prefiltered once"
    );
    Ok(())
}
