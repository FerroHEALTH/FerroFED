// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The XCPD localizer against stub responding gateways: the communities a
//! broadcast names become candidates, and any gateway that fails fails the
//! whole discovery closed (N4, §14.1, Annex A.3; ITI TF-2 §3.55).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::Tls;
use ferrofed_identity::ihe::xcpd::{
    FixedAssertion, GatewayConfig, Transport, XcpdConfig, XcpdConfigError, XcpdLocalizer,
};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::localizer::{Localization, Localizer, LocalizerError};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::tls::MutualTls;
use ferrofed_testkit::xcpd::{Answer, Community, RespondingGateway};
use ihe_iti::xcpd::security::XuaAssertion;
use secrecy::SecretString;

use crate::support::{PATIENT_VALUE, registry};

type TestResult = Result<(), Box<dyn Error>>;

/// The community serving `node-a`.
pub(crate) const COMMUNITY_A: &str = "2.999.50";
/// The community serving `node-b`.
const COMMUNITY_B: &str = "2.999.60";

pub(crate) fn node(id: &str) -> Result<NodeId, Box<dyn Error>> {
    Ok(id.parse()?)
}

pub(crate) fn patient() -> Result<PatientRef, Box<dyn Error>> {
    Ok(PatientRef::new(
        IdentifierNamespace::new("urn:oid:2.999.1")?,
        PATIENT_VALUE.into(),
    )?)
}

pub(crate) fn members() -> Result<Vec<NodeId>, Box<dyn Error>> {
    Ok(vec![node("node-a")?, node("node-b")?])
}

/// The configuration over `gateways`, reached as `transport` allows.
fn config(gateways: &[String], transport: Transport) -> Result<XcpdConfig, Box<dyn Error>> {
    Ok(XcpdConfig {
        sender_device: "2.999.40.1".to_owned(),
        home_community: Some("2.999.40".to_owned()),
        gateways: gateways
            .iter()
            .map(|endpoint| {
                Ok(GatewayConfig {
                    endpoint: SecretUrl::new(endpoint.as_str()),
                    device: "2.999.50.1".to_owned(),
                    community: None,
                })
            })
            .collect::<Result<_, Box<dyn Error>>>()?,
        communities: BTreeMap::from([
            (COMMUNITY_A.to_owned(), node("node-a")?),
            (format!("urn:oid:{COMMUNITY_B}"), node("node-b")?),
        ]),
        namespaces: BTreeMap::new(),
        transport,
        tls: Tls::default(),
    })
}

pub(crate) fn localizer(stubs: &[&RespondingGateway]) -> Result<XcpdLocalizer, Box<dyn Error>> {
    let endpoints: Vec<String> = stubs.iter().map(|stub| stub.endpoint()).collect();
    Ok(XcpdLocalizer::from_config(
        config(&endpoints, Transport::UnencryptedForDevelopment)?,
        None,
        &registry(),
    )?)
}

pub(crate) async fn localize(localizer: &XcpdLocalizer) -> Result<Localization, Box<dyn Error>> {
    Ok(localizer
        .localize(
            &patient()?,
            &members()?,
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_secs(2),
        )
        .await)
}

pub(crate) fn holds(home: &str) -> Answer {
    Answer::Holds(vec![Community::new(
        home,
        &format!("{home}.2"),
        "PID-SYNTH",
    )])
}

// conformance: CP-5
#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_communities_a_broadcast_names_are_the_candidates() -> TestResult {
    let first = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let second = RespondingGateway::answering(Answer::NoMatch).await;
    let answer = localize(&localizer(&[&first, &second])?).await?;
    match answer {
        Localization::Candidates(named) => {
            assert_eq!(BTreeSet::from([node("node-a")?]), named);
        }
        other => return Err(format!("a candidate set (N4): {other:?}").into()),
    }
    for stub in [&first, &second] {
        let requests = stub.requests().await;
        assert_eq!(1, requests.len(), "every gateway is asked once");
        assert!(
            requests[0].contains(PATIENT_VALUE),
            "the identifier reaches the localization service"
        );
    }
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_community_outside_the_federation_adds_no_member() -> TestResult {
    let stub = RespondingGateway::answering(holds("2.999.99")).await;
    match localize(&localizer(&[&stub])?).await? {
        Localization::NoRecords => Ok(()),
        other => Err(format!("no member holds the patient: {other:?}").into()),
    }
}

// conformance: CP-5
#[tokio::test]
async fn no_match_anywhere_is_no_records() -> TestResult {
    let stub = RespondingGateway::answering(Answer::NoMatch).await;
    match localize(&localizer(&[&stub])?).await? {
        Localization::NoRecords => Ok(()),
        other => Err(format!("no records: {other:?}").into()),
    }
}

// conformance: CP-5
#[tokio::test]
async fn one_failing_gateway_fails_the_whole_discovery_closed() -> TestResult {
    for (failing, answered) in [(Answer::Busy, 200), (Answer::Fault, 500)] {
        let answering = RespondingGateway::answering(holds(COMMUNITY_A)).await;
        let down = RespondingGateway::answering(failing.clone()).await;
        match localize(&localizer(&[&answering, &down])?).await? {
            Localization::Unavailable(error @ LocalizerError::Answered { .. }) => {
                if error.status().map(|status| status.as_u16()) != Some(answered) {
                    return Err(format!("the status the gateway answered: {error:?}").into());
                }
                let rendered = format!("{} {error:?}", ferrofed_chain(&error));
                if rendered.contains(PATIENT_VALUE) {
                    return Err(format!("the error carries the identifier: {rendered}").into());
                }
            }
            other => {
                return Err(format!(
                    "a partial broadcast is no answer (§14.1) for {failing:?}: {other:?}"
                )
                .into());
            }
        }
    }
    Ok(())
}

// conformance: CP-5
#[tokio::test]
async fn a_silent_gateway_runs_out_the_budget() -> TestResult {
    let silent = RespondingGateway::answering(Answer::Silent).await;
    let localizer = localizer(&[&silent])?;
    let answer = localizer
        .localize(
            &patient()?,
            &members()?,
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_millis(300),
        )
        .await;
    match answer {
        Localization::Unavailable(LocalizerError::DeadlineExceeded) => Ok(()),
        other => Err(format!("a timeout: {other:?}").into()),
    }
}

#[tokio::test]
#[expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]
async fn the_assertion_source_rides_on_every_request() -> TestResult {
    let stub = RespondingGateway::answering(Answer::NoMatch).await;
    let assertion = XuaAssertion::new(
        r#"<saml2:Assertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion" ID="_s"/>"#,
    )?;
    let localizer = XcpdLocalizer::from_config(
        config(&[stub.endpoint()], Transport::UnencryptedForDevelopment)?,
        Some(Arc::new(FixedAssertion::new(assertion))),
        &registry(),
    )?;
    let _answer = localize(&localizer).await?;
    let requests = stub.requests().await;
    assert!(requests[0].contains(
        r#"<saml2:Assertion xmlns:saml2="urn:oasis:names:tc:SAML:2.0:assertion" ID="_s"/>"#
    ));
    Ok(())
}

#[test]
fn an_http_gateway_is_refused_outside_development() -> TestResult {
    let refused = XcpdLocalizer::from_config(
        config(
            &["http://xcpd.example.org/rg".to_owned()],
            Transport::Encrypted,
        )?,
        None,
        &registry(),
    );
    match refused {
        Err(XcpdConfigError::Endpoint { index: 0, .. }) => Ok(()),
        other => Err(format!("the gateway is reached over https only: {other:?}").into()),
    }
}

#[test]
fn every_member_needs_a_community_and_every_community_a_member() -> TestResult {
    let endpoint = vec!["https://xcpd.example.org/rg".to_owned()];
    let mut uncovered = config(&endpoint, Transport::Encrypted)?;
    uncovered.communities.remove(COMMUNITY_A);
    match XcpdLocalizer::from_config(uncovered, None, &registry()) {
        Err(XcpdConfigError::UnlocatedMember(member)) if member.as_str() == "node-a" => {}
        other => return Err(format!("node-a could never be localized: {other:?}").into()),
    }
    let mut unknown = config(&endpoint, Transport::Encrypted)?;
    unknown
        .communities
        .insert("2.999.70".to_owned(), node("node-z")?);
    match XcpdLocalizer::from_config(unknown, None, &registry()) {
        Err(XcpdConfigError::UnknownMember(_)) => {}
        other => return Err(format!("an unknown member is refused: {other:?}").into()),
    }
    let mut doubled = config(&endpoint, Transport::Encrypted)?;
    doubled
        .communities
        .insert(format!("urn:oid:{COMMUNITY_A}"), node("node-a")?);
    match XcpdLocalizer::from_config(doubled, None, &registry()) {
        Err(XcpdConfigError::DuplicateCommunity(_)) => Ok(()),
        other => Err(format!("a community mapped twice is refused: {other:?}").into()),
    }
}

#[test]
fn identifiers_that_are_not_oids_are_refused() -> TestResult {
    let endpoint = vec!["https://xcpd.example.org/rg".to_owned()];
    let mut sender = config(&endpoint, Transport::Encrypted)?;
    sender.sender_device = "not-an-oid".to_owned();
    assert_refused(sender, |error| {
        matches!(error, XcpdConfigError::SenderDevice(_))
    })?;
    let mut device = config(&endpoint, Transport::Encrypted)?;
    device.gateways[0].device = "x".to_owned();
    assert_refused(device, |error| {
        matches!(error, XcpdConfigError::GatewayOid { index: 0, .. })
    })?;
    let mut community = config(&endpoint, Transport::Encrypted)?;
    community
        .communities
        .insert("x".to_owned(), node("node-a")?);
    assert_refused(community, |error| {
        matches!(error, XcpdConfigError::Community { .. })
    })?;
    let mut namespace = config(&endpoint, Transport::Encrypted)?;
    namespace
        .namespaces
        .insert(IdentifierNamespace::new("bsn-like")?, "x".to_owned());
    assert_refused(namespace, |error| {
        matches!(error, XcpdConfigError::Namespace(_))
    })
}

fn assert_refused(config: XcpdConfig, expected: impl Fn(&XcpdConfigError) -> bool) -> TestResult {
    match XcpdLocalizer::from_config(config, None, &registry()) {
        Err(error) if expected(&error) => Ok(()),
        other => Err(format!("refused: {other:?}").into()),
    }
}

#[test]
fn no_rendering_shows_the_tls_identity() -> TestResult {
    let front = MutualTls::front("http://127.0.0.1:9")?;
    let identity = SecretString::from(front.client_identity());
    let tls = Tls::from_pem(Some(&identity), Some(front.trust_roots()))?;
    let rendered = format!("{tls:?}");
    if rendered.contains("PRIVATE KEY") || rendered.contains("CERTIFICATE") {
        return Err(format!("the key shows: {rendered}").into());
    }
    Ok(())
}

/// `error` and every cause behind it, as one line.
fn ferrofed_chain(error: &(dyn Error + 'static)) -> String {
    let mut line = error.to_string();
    let mut cause = error.source();
    while let Some(source) = cause {
        line.push_str(": ");
        line.push_str(&source.to_string());
        cause = source.source();
    }
    line
}

#[tokio::test]
async fn an_unreachable_gateway_has_no_status() -> TestResult {
    let localizer = XcpdLocalizer::from_config(
        config(
            &["http://127.0.0.1:0/rg".to_owned()],
            Transport::UnencryptedForDevelopment,
        )?,
        None,
        &registry(),
    )?;
    match localize(&localizer).await? {
        Localization::Unavailable(error) if error.status().is_none() => Ok(()),
        other => Err(format!("no answer, no status: {other:?}").into()),
    }
}

/// A recorder that accepts nothing.
struct Refusing;

#[derive(Debug, thiserror::Error)]
#[error("synthetic audit repository outage")]
struct AuditOutage;

#[async_trait::async_trait]
impl ihe_iti::xcpd::audit::AuditRecorder for Refusing {
    async fn record(
        &self,
        _event: ihe_iti::xcpd::audit::AuditEvent,
    ) -> Result<(), ihe_iti::xcpd::audit::AuditError> {
        Err(ihe_iti::xcpd::audit::AuditError(Box::new(AuditOutage)))
    }
}

// conformance: CP-5
#[tokio::test]
async fn a_discovery_whose_audit_is_refused_fails_closed() -> TestResult {
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(Arc::new(Refusing));
    match localize(&localizer).await? {
        Localization::Unavailable(error @ LocalizerError::AuditFailed(_)) => {
            let rendered = ferrofed_chain(&error);
            if rendered.contains("audit") {
                Ok(())
            } else {
                Err(format!("the audit failure is named: {rendered}").into())
            }
        }
        other => Err(format!("no candidate without its audit (§3.55.5.1): {other:?}").into()),
    }
}

/// A recorder that never accepts, as a spool whose disk stalled does not.
struct Stalled;

#[async_trait::async_trait]
impl ihe_iti::xcpd::audit::AuditRecorder for Stalled {
    async fn record(
        &self,
        _event: ihe_iti::xcpd::audit::AuditEvent,
    ) -> Result<(), ihe_iti::xcpd::audit::AuditError> {
        std::future::pending().await
    }
}

// conformance: CP-5
#[tokio::test]
async fn a_discovery_whose_audit_is_not_stored_by_the_deadline_fails_closed_in_time() -> TestResult
{
    let stub = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&stub])?.audited(Arc::new(Stalled));
    let budget = Duration::from_millis(300);
    let asked = Instant::now();
    let answer = localizer
        .localize(
            &patient()?,
            &members()?,
            &OnBehalfOf::Gateway,
            asked + budget,
        )
        .await;
    let waited = asked.elapsed();
    if waited >= budget + Duration::from_secs(3) {
        return Err(format!("answered within the budget: {waited:?}").into());
    }
    match answer {
        Localization::Unavailable(error @ LocalizerError::AuditFailed(_)) => {
            let rendered = ferrofed_chain(&error);
            if rendered.contains("audit record was not stored") {
                Ok(())
            } else {
                Err(format!("the late record is named: {rendered}").into())
            }
        }
        other => Err(format!("no candidate without its audit (§3.55.5.1): {other:?}").into()),
    }
}

/// A recorder that refuses the audit of every exchange that was answered,
/// and accepts the others.
struct RefusingAnswers;

#[async_trait::async_trait]
impl ihe_iti::xcpd::audit::AuditRecorder for RefusingAnswers {
    async fn record(
        &self,
        event: ihe_iti::xcpd::audit::AuditEvent,
    ) -> Result<(), ihe_iti::xcpd::audit::AuditError> {
        if event.outcome == ihe_iti::xcpd::audit::EventOutcome::Success {
            return Err(ihe_iti::xcpd::audit::AuditError(Box::new(AuditOutage)));
        }
        Ok(())
    }
}

// conformance: CP-5
#[tokio::test]
async fn an_audit_failure_outranks_another_gateway_s_timeout() -> TestResult {
    let silent = RespondingGateway::answering(Answer::Silent).await;
    let answering = RespondingGateway::answering(holds(COMMUNITY_A)).await;
    let localizer = localizer(&[&silent, &answering])?.audited(Arc::new(RefusingAnswers));
    let answer = localizer
        .localize(
            &patient()?,
            &members()?,
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_millis(500),
        )
        .await;
    match answer {
        Localization::Unavailable(LocalizerError::AuditFailed(_)) => Ok(()),
        other => Err(format!("an unaudited exchange is never a plain outage: {other:?}").into()),
    }
}
