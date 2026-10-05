// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm localizer: the members whose `ehr_id` domain holds an identifier
//! for the patient are the candidates (§14.2, "demographic-registration"),
//! read from the same ITI-83 call the resolution of the query reuses, and a
//! PIX Manager that does not answer fails closed (§14.1, N4, Annex A.1).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::pixm::{ManagerConfig, PixmResolver, SHARED_CAPACITY};
use ferrofed_identity::role::behalf::{Caller, OnBehalfOf};
use ferrofed_identity::role::localizer::{Localization, Localizer, LocalizerError};
use ferrofed_identity::role::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::role::resolver::{Resolution, Resolver};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::mock::Server;
use ihe_iti::pixm::Invocation;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::support::registry;

const SENTINEL: &str = "SENTINEL-PIX-408";
const NAMESPACE: &str = "urn:oid:2.999.1";
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";
const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const OPERATION: &str = "/fhir/Patient/$ihe-pix";
const FHIR_JSON: &str = "application/fhir+json";

fn node(id: &str) -> NodeId {
    NodeId::new(id).expect("a node id")
}

fn patient(value: &str) -> PatientRef {
    PatientRef::new(
        IdentifierNamespace::new(NAMESPACE).expect("a namespace"),
        SecretString::from(value),
    )
    .expect("a patient reference")
}

fn members() -> Vec<NodeId> {
    vec![node("node-a"), node("node-b")]
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

/// The PIXm resolver over one stub Manager serving both members.
fn pixm(server: &Server) -> PixmResolver {
    PixmResolver::from_config(
        vec![ManagerConfig {
            tls: Tls::default(),
            base: SecretUrl::new(format!("{}/fhir/", server.uri())),
            auth: Authentication::None,
            members: BTreeMap::from([
                (node("node-a"), DOMAIN_A.to_owned()),
                (node("node-b"), DOMAIN_B.to_owned()),
            ]),
            invocation: Invocation::Get,
        }],
        BTreeMap::new(),
        &registry(),
    )
    .expect("the resolver builds")
}

/// A stub Manager answering every call with `status` and `body`, after
/// `delay`.
async fn manager(status: u16, body: &str, delay: Duration) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .respond_with(
            ResponseTemplate::new(status)
                .set_body_raw(body.as_bytes().to_vec(), FHIR_JSON)
                .set_delay(delay),
        )
        .mount(&server)
        .await;
    server
}

/// A `Parameters` answer holding node A's `ehr_id` only.
fn at_a() -> String {
    format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{DOMAIN_A}","value":"{EHR_A}"}}}}]}}"#
    )
}

async fn calls(server: &Server) -> usize {
    server
        .received_requests()
        .await
        .expect("recording is on")
        .len()
}

// conformance: CP-5
#[tokio::test]
async fn the_candidates_are_the_members_whose_domain_holds_the_patient() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let pixm = pixm(&server);
    match pixm
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await
    {
        Localization::Candidates(named) => {
            assert_eq!(BTreeSet::from([node("node-a")]), named);
        }
        other => panic!("node A holds the patient (§14.2): {other:?}"),
    }
}

// conformance: CP-5
#[tokio::test]
async fn the_resolution_of_the_same_query_reuses_the_localization_call() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let pixm = pixm(&server);
    let _named = pixm
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await;
    let resolutions = pixm
        .resolve(
            &patient(SENTINEL),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert!(
        matches!(resolutions.get(&node("node-a")), Some(Resolution::Resolved(ehr)) if ehr.as_str() == EHR_A)
    );
    assert_eq!(1, calls(&server).await, "one ITI-83 call per query");

    let _again = pixm
        .resolve(
            &patient(SENTINEL),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert_eq!(
        2,
        calls(&server).await,
        "a kept answer serves one resolution"
    );
}

/// A recorder that keeps every exchange it is given.
#[derive(Default)]
struct Kept(std::sync::Mutex<Vec<ihe_iti::balp::Exchange>>);

impl Kept {
    /// Whom each kept exchange was made for, in order: the user's `sub`,
    /// or `None` for the system's own.
    fn subjects(&self) -> Vec<Option<String>> {
        self.0
            .lock()
            .expect("the kept exchanges")
            .iter()
            .map(|exchange| {
                exchange
                    .on_behalf
                    .user()
                    .map(|user| user.subject().to_owned())
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl ihe_iti::balp::AuditRecorder for Kept {
    async fn record(
        &self,
        exchange: ihe_iti::balp::Exchange,
    ) -> Result<(), ihe_iti::balp::AuditError> {
        self.0.lock().expect("the kept exchanges").push(exchange);
        Ok(())
    }
}

/// A verified caller `subject` of the suite's synthetic issuer and client.
fn caller(subject: &str) -> OnBehalfOf {
    OnBehalfOf::Caller(Caller::new(
        "https://issuer.example.test".to_owned(),
        subject.to_owned(),
        "Qz7-app-42".to_owned(),
    ))
}

/// The PIXm resolver over `server`, recording every exchange in `kept`.
fn audited(server: &Server, kept: &std::sync::Arc<Kept>) -> PixmResolver {
    let recorder: std::sync::Arc<dyn ihe_iti::balp::AuditRecorder> = kept.clone();
    pixm(server).audited(&recorder)
}

#[tokio::test]
async fn another_caller_never_reads_a_kept_answer_and_each_access_is_recorded_naming_its_caller() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let kept = std::sync::Arc::new(Kept::default());
    let pixm = audited(&server, &kept);
    let _named = pixm
        .localize(
            &patient(SENTINEL),
            &members(),
            &caller("Qz7-caller-a"),
            soon(),
        )
        .await;
    let resolutions = pixm
        .resolve(
            &patient(SENTINEL),
            &[node("node-a")],
            &caller("Qz7-caller-b"),
            soon(),
        )
        .await;
    assert!(
        matches!(resolutions.get(&node("node-a")), Some(Resolution::Resolved(ehr)) if ehr.as_str() == EHR_A)
    );
    // NOTE: PIXm §2:3.83.5.2.1: each record names the caller whose request sent the ITI-83,
    // so caller B asks the Manager itself rather than reading caller A's answer.
    assert_eq!(2, calls(&server).await, "caller B sends its own ITI-83");
    assert_eq!(
        vec![
            Some("Qz7-caller-a".to_owned()),
            Some("Qz7-caller-b".to_owned())
        ],
        kept.subjects(),
        "one record per ITI-83 sent, each naming the caller of its request"
    );
}

#[tokio::test]
async fn the_same_caller_reuses_its_answer_and_no_record_claims_a_second_exchange() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let kept = std::sync::Arc::new(Kept::default());
    let pixm = audited(&server, &kept);
    let _named = pixm
        .localize(
            &patient(SENTINEL),
            &members(),
            &caller("Qz7-caller-a"),
            soon(),
        )
        .await;
    let _resolutions = pixm
        .resolve(
            &patient(SENTINEL),
            &[node("node-a")],
            &caller("Qz7-caller-a"),
            soon(),
        )
        .await;
    assert_eq!(1, calls(&server).await, "one ITI-83 for the query");
    assert_eq!(
        vec![Some("Qz7-caller-a".to_owned())],
        kept.subjects(),
        "one record, for the one ITI-83 sent, naming the caller who caused it"
    );
}

#[tokio::test]
async fn a_resolution_the_gateway_makes_is_recorded_as_its_own() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let kept = std::sync::Arc::new(Kept::default());
    let _resolutions = audited(&server, &kept)
        .resolve(
            &patient(SENTINEL),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert_eq!(
        vec![None],
        kept.subjects(),
        "the record is made, as the system's own, naming no user"
    );
}

#[tokio::test]
async fn another_patient_never_reads_a_kept_answer() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let pixm = pixm(&server);
    let _named = pixm
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await;
    let _other = pixm
        .resolve(
            &patient("SENTINEL-OTHER"),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert_eq!(2, calls(&server).await, "the other patient is asked about");
    assert!(
        !format!("{pixm:?}").contains(SENTINEL),
        "no rendering shows a kept identifier"
    );
}

#[tokio::test]
async fn a_localization_past_the_capacity_keeps_nothing_and_its_resolution_asks_again() {
    let server = manager(200, &at_a(), Duration::ZERO).await;
    let pixm = pixm(&server);
    let kept = format!("shared: {SHARED_CAPACITY}");
    for index in 0..SHARED_CAPACITY {
        let named = pixm
            .localize(
                &patient(&format!("SENTINEL-CAP-{index}")),
                &members(),
                &OnBehalfOf::Gateway,
                soon(),
            )
            .await;
        assert!(matches!(named, Localization::Candidates(_)), "{named:?}");
    }
    assert!(format!("{pixm:?}").contains(&kept), "{pixm:?}");

    let named = pixm
        .localize(
            &patient("SENTINEL-OVER"),
            &members(),
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert!(
        matches!(&named, Localization::Candidates(set) if set.contains(&node("node-a"))),
        "the localization answers past the capacity: {named:?}"
    );
    assert!(
        format!("{pixm:?}").contains(&kept),
        "the capacity holds: {pixm:?}"
    );
    let asked = calls(&server).await;
    assert_eq!(SHARED_CAPACITY + 1, asked);

    let resolutions = pixm
        .resolve(
            &patient("SENTINEL-OVER"),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert!(
        matches!(resolutions.get(&node("node-a")), Some(Resolution::Resolved(ehr)) if ehr.as_str() == EHR_A),
        "the resolution past the capacity resolves: {resolutions:?}"
    );
    assert_eq!(
        asked + 1,
        calls(&server).await,
        "it asks the Manager itself"
    );

    let _first = pixm
        .resolve(
            &patient("SENTINEL-CAP-0"),
            &[node("node-a")],
            &OnBehalfOf::Gateway,
            soon(),
        )
        .await;
    assert_eq!(
        asked + 1,
        calls(&server).await,
        "a kept answer still serves"
    );
}

// conformance: CP-5
#[tokio::test]
async fn a_patient_the_manager_does_not_know_has_no_records() {
    let server = manager(
        404,
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#,
        Duration::ZERO,
    )
    .await;
    let answer = pixm(&server)
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(matches!(answer, Localization::NoRecords), "{answer:?}");
}

#[tokio::test]
async fn a_localization_no_resolution_follows_keeps_nothing() {
    let unknown = manager(
        404,
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#,
        Duration::ZERO,
    )
    .await;
    let pixm = pixm(&unknown);
    for index in 0..64 {
        let answer = pixm
            .localize(
                &patient(&format!("SENTINEL-UNKNOWN-{index}")),
                &members(),
                &OnBehalfOf::Gateway,
                soon(),
            )
            .await;
        assert!(matches!(answer, Localization::NoRecords), "{answer:?}");
    }
    assert!(
        format!("{pixm:?}").contains("shared: 0"),
        "a burst of unknown patients leaves the memo empty: {pixm:?}"
    );

    let failing = manager(503, "{}", Duration::ZERO).await;
    let pixm = self::pixm(&failing);
    let answer = pixm
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await;
    assert!(matches!(answer, Localization::Unavailable(_)), "{answer:?}");
    assert!(
        format!("{pixm:?}").contains("shared: 0"),
        "a failed localization keeps nothing: {pixm:?}"
    );
}

// conformance: CP-5
#[tokio::test]
async fn a_manager_that_fails_leaves_the_localization_unavailable() {
    let failing = manager(503, "{}", Duration::ZERO).await;
    match pixm(&failing)
        .localize(&patient(SENTINEL), &members(), &OnBehalfOf::Gateway, soon())
        .await
    {
        Localization::Unavailable(error) => {
            assert_eq!(
                Some(503),
                error.status().map(|status| status.as_u16()),
                "the status the Manager answered with"
            );
        }
        other => panic!("§14.1: no candidate set from a failing Manager: {other:?}"),
    }

    let silent = manager(200, &at_a(), Duration::from_secs(10)).await;
    let answer = pixm(&silent)
        .localize(
            &patient(SENTINEL),
            &members(),
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_millis(200),
        )
        .await;
    assert!(
        matches!(
            answer,
            Localization::Unavailable(LocalizerError::DeadlineExceeded)
        ),
        "{answer:?}"
    );
}
