// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver against a stub PIX Manager: one ITI-83 call per Manager
//! with a `targetSystem` per member, the member's `ehr_id` read from its
//! domain, the unknown patient as `Unknown` (N6), every failure as
//! `Unavailable` so the query fails closed (§11.3 covers only an answered
//! lookup; no specification governs this: our own design), and the patient
//! identifier carried to the Manager only (§5.2, §5.4.1, N3, N33, Annex A.1).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ferrofed_identity::patient::{IdentifierNamespace, PatientRef};
use ferrofed_identity::pixm::{ManagerConfig, PixAuth, PixmConfigError, PixmResolver};
use ferrofed_identity::resolver::{Resolution, Resolver, ResolverError};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::mock::Server;
use openehr_its::rest::client::InvalidCredentials;
use secrecy::SecretString;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::support::registry;

/// The synthetic patient identifier, never to reach anything but the Manager.
const SENTINEL: &str = "SENTINEL-PIX-43";

/// The client's issuing namespace, and the PIX assigning authority it maps to.
const NAMESPACE: &str = "2.999.1";
const SOURCE_SYSTEM: &str = "urn:oid:2.999.1";

/// The `ehr_id` domains of node A and node B at the Manager.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The patient's `ehr_id` at node A and at node B.
const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const EHR_B: &str = "1111bbbb-1111-4111-8111-111111111111";

const OPERATION: &str = "/fhir/Patient/$ihe-pix";
const FHIR_JSON: &str = "application/fhir+json";

fn node(id: &str) -> NodeId {
    NodeId::new(id).expect("a node id")
}

fn patient() -> PatientRef {
    PatientRef::new(
        IdentifierNamespace::new(NAMESPACE).expect("a namespace"),
        SecretString::from(SENTINEL),
    )
    .expect("a patient reference")
}

fn members(pairs: &[(&str, &str)]) -> BTreeMap<NodeId, String> {
    pairs
        .iter()
        .map(|(member, domain)| (node(member), (*domain).to_owned()))
        .collect()
}

fn namespaces() -> BTreeMap<IdentifierNamespace, String> {
    BTreeMap::from([(
        IdentifierNamespace::new(NAMESPACE).expect("a namespace"),
        SOURCE_SYSTEM.to_owned(),
    )])
}

fn manager(server: &Server, auth: PixAuth, pairs: &[(&str, &str)]) -> ManagerConfig {
    ManagerConfig {
        base: SecretUrl::new(format!("{}/fhir/", server.uri())),
        auth,
        members: members(pairs),
    }
}

/// The resolver over one stub Manager serving both members.
fn resolver(server: &Server) -> PixmResolver {
    PixmResolver::from_config(
        vec![manager(
            server,
            PixAuth::None,
            &[("node-a", DOMAIN_A), ("node-b", DOMAIN_B)],
        )],
        namespaces(),
        &registry(),
    )
    .expect("the resolver builds")
}

/// A `Parameters` answer with one `targetIdentifier` per `(domain, value)`.
fn parameters(identifiers: &[(&str, &str)]) -> String {
    let parameter: Vec<String> = identifiers
        .iter()
        .map(|(system, value)| {
            format!(
                r#"{{"name":"targetIdentifier","valueIdentifier":{{"system":"{system}","value":"{value}"}}}}"#
            )
        })
        .collect();
    format!(
        r#"{{"resourceType":"Parameters","parameter":[{}]}}"#,
        parameter.join(",")
    )
}

async fn stub(status: u16, body: impl Into<String>) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .respond_with(
            ResponseTemplate::new(status).set_body_raw(body.into().into_bytes(), FHIR_JSON),
        )
        .mount(&server)
        .await;
    server
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(5)
}

async fn resolve(resolver: &PixmResolver) -> BTreeMap<NodeId, Resolution> {
    resolver
        .resolve(&patient(), &[node("node-a"), node("node-b")], soon())
        .await
}

fn resolved_at(resolutions: &BTreeMap<NodeId, Resolution>, member: &str) -> Option<String> {
    match resolutions.get(&node(member)) {
        Some(Resolution::Resolved(ehr_id)) => Some(ehr_id.as_str().to_owned()),
        _ => None,
    }
}

fn is_unknown(resolutions: &BTreeMap<NodeId, Resolution>, member: &str) -> bool {
    matches!(resolutions.get(&node(member)), Some(Resolution::Unknown))
}

fn is_unavailable(resolutions: &BTreeMap<NodeId, Resolution>, member: &str) -> bool {
    matches!(
        resolutions.get(&node(member)),
        Some(Resolution::Unavailable(_))
    )
}

/// Every error message in the chain of every outcome, joined.
fn rendered(resolutions: &BTreeMap<NodeId, Resolution>) -> String {
    let mut text = format!("{resolutions:?}");
    for resolution in resolutions.values() {
        if let Resolution::Unavailable(error) = resolution {
            let mut current: Option<&dyn std::error::Error> = Some(error);
            while let Some(error) = current {
                text.push_str(&error.to_string());
                current = error.source();
            }
        }
    }
    text
}

// conformance: CP-3
#[tokio::test]
async fn one_call_resolves_the_patient_at_every_member_by_its_domain() {
    let server = stub(200, parameters(&[(DOMAIN_A, EHR_A), (DOMAIN_B, EHR_B)])).await;

    let resolutions = resolve(&resolver(&server)).await;
    assert_eq!(Some(EHR_A.to_owned()), resolved_at(&resolutions, "node-a"));
    assert_eq!(Some(EHR_B.to_owned()), resolved_at(&resolutions, "node-b"));

    let requests = server.received_requests().await.expect("recording is on");
    assert_eq!(
        1,
        requests.len(),
        "one ITI-83 call per PIX Manager, with every member's domain"
    );
    let pairs: Vec<(String, String)> = requests
        .first()
        .expect("one request")
        .url
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(
        vec![
            (
                "sourceIdentifier".to_owned(),
                format!("{SOURCE_SYSTEM}|{SENTINEL}")
            ),
            ("targetSystem".to_owned(), DOMAIN_A.to_owned()),
            ("targetSystem".to_owned(), DOMAIN_B.to_owned()),
        ],
        pairs,
        "the namespace maps to the PIX assigning authority, and each member's ehr_id domain is a targetSystem (Annex A.1)"
    );
}

// conformance: CP-3
#[tokio::test]
async fn a_member_whose_domain_holds_nothing_does_not_know_the_patient() {
    let server = stub(200, parameters(&[(DOMAIN_A, EHR_A)])).await;
    let resolutions = resolve(&resolver(&server)).await;
    assert_eq!(Some(EHR_A.to_owned()), resolved_at(&resolutions, "node-a"));
    assert!(
        is_unknown(&resolutions, "node-b"),
        "no identifier in the member's domain is an unknown patient there (N6): {resolutions:?}"
    );
}

// conformance: CP-3
#[tokio::test]
async fn an_unknown_patient_is_unknown_at_every_member() {
    let not_found =
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#;
    let server = stub(404, not_found).await;
    let resolutions = resolve(&resolver(&server)).await;
    assert!(
        is_unknown(&resolutions, "node-a") && is_unknown(&resolutions, "node-b"),
        "the profile's not-found answer is not an outage (§2:3.83.4.2.2.2, N6): {resolutions:?}"
    );
}

#[tokio::test]
async fn an_outage_is_unavailable_and_says_nothing_of_the_patient() {
    let server = stub(500, format!("internal error for {SENTINEL}")).await;
    let resolutions = resolve(&resolver(&server)).await;
    assert!(
        is_unavailable(&resolutions, "node-a") && is_unavailable(&resolutions, "node-b"),
        "a failed exchange fails closed, never reads as unknown (§11.3, N6): {resolutions:?}"
    );
    assert!(
        !rendered(&resolutions).contains(SENTINEL),
        "no outcome, error or source carries the identifier"
    );
}

#[tokio::test]
async fn a_manager_slower_than_the_deadline_is_unavailable() {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(parameters(&[]).into_bytes(), FHIR_JSON)
                .set_delay(Duration::from_secs(5)),
        )
        .mount(&server)
        .await;
    let resolutions = resolver(&server)
        .resolve(
            &patient(),
            &[node("node-a"), node("node-b")],
            Instant::now() + Duration::from_millis(150),
        )
        .await;
    assert!(
        matches!(
            resolutions.get(&node("node-a")),
            Some(Resolution::Unavailable(ResolverError::DeadlineExceeded))
        ),
        "the resolution budget bounds the exchange: {resolutions:?}"
    );
}

#[tokio::test]
async fn two_identifiers_in_one_domain_are_never_guessed_between() {
    let server = stub(
        200,
        parameters(&[
            (DOMAIN_A, EHR_A),
            (DOMAIN_A, "3333cccc-3333-4333-8333-333333333333"),
        ]),
    )
    .await;
    let resolutions = resolve(&resolver(&server)).await;
    assert!(
        is_unavailable(&resolutions, "node-a"),
        "an ambiguous ehr_id fails closed: {resolutions:?}"
    );
}

#[tokio::test]
async fn an_identifier_that_is_no_ehr_id_is_unavailable_and_not_echoed() {
    let server = stub(200, parameters(&[(DOMAIN_A, "not an ehr id!")])).await;
    let resolutions = resolve(&resolver(&server)).await;
    assert!(is_unavailable(&resolutions, "node-a"), "{resolutions:?}");
    assert!(
        !rendered(&resolutions).contains("not an ehr id!"),
        "the Manager's value is not repeated in an error"
    );
}

#[tokio::test]
async fn a_namespace_with_no_pix_domain_asks_nobody() {
    let server = stub(200, parameters(&[(DOMAIN_A, EHR_A)])).await;
    let resolver = PixmResolver::from_config(
        vec![manager(
            &server,
            PixAuth::None,
            &[("node-a", DOMAIN_A), ("node-b", DOMAIN_B)],
        )],
        BTreeMap::new(),
        &registry(),
    )
    .expect("the resolver builds");
    let resolutions = resolve(&resolver).await;
    assert!(
        is_unavailable(&resolutions, "node-a") && is_unavailable(&resolutions, "node-b"),
        "an unmapped, non-URI namespace fails closed: {resolutions:?}"
    );
    assert!(
        server
            .received_requests()
            .await
            .expect("recording is on")
            .is_empty(),
        "nothing is sent for a namespace the Manager has no domain for"
    );
}

#[tokio::test]
async fn the_bearer_credential_travels_to_the_manager() {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .and(header("authorization", "Bearer synthetic-pix-token"))
        .and(query_param("targetSystem", DOMAIN_A))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(parameters(&[(DOMAIN_A, EHR_A)]).into_bytes(), FHIR_JSON),
        )
        .mount(&server)
        .await;
    let resolver = PixmResolver::from_config(
        vec![manager(
            &server,
            PixAuth::Bearer(SecretString::from("synthetic-pix-token")),
            &[("node-a", DOMAIN_A), ("node-b", DOMAIN_B)],
        )],
        namespaces(),
        &registry(),
    )
    .expect("the resolver builds");
    let resolutions = resolve(&resolver).await;
    assert_eq!(
        Some(EHR_A.to_owned()),
        resolved_at(&resolutions, "node-a"),
        "the stub answers only an authorised request: {resolutions:?}"
    );
    assert!(
        !format!("{resolver:?}").contains("synthetic-pix-token"),
        "the resolver's Debug shows no credential"
    );
}

#[test]
fn every_member_must_have_exactly_one_domain() {
    let server_uri = "http://127.0.0.1:9";
    let config = |pairs: &[(&str, &str)]| ManagerConfig {
        base: SecretUrl::new(format!("{server_uri}/fhir/")),
        auth: PixAuth::None,
        members: members(pairs),
    };
    let built = |managers| PixmResolver::from_config(managers, namespaces(), &registry());
    assert!(matches!(
        built(vec![config(&[("node-a", DOMAIN_A)])]),
        Err(PixmConfigError::UnresolvedMember(_))
    ));
    assert!(matches!(
        built(vec![config(&[
            ("node-a", DOMAIN_A),
            ("node-b", DOMAIN_B),
            ("node-c", DOMAIN_B)
        ])]),
        Err(PixmConfigError::UnknownMember(_))
    ));
    assert!(matches!(
        built(vec![
            config(&[("node-a", DOMAIN_A), ("node-b", DOMAIN_B)]),
            config(&[("node-b", DOMAIN_B)])
        ]),
        Err(PixmConfigError::DuplicateMember(_))
    ));
    assert!(matches!(
        built(vec![config(&[
            ("node-a", "not a uri"),
            ("node-b", DOMAIN_B)
        ])]),
        Err(PixmConfigError::Domain { .. })
    ));
    assert!(matches!(built(Vec::new()), Err(PixmConfigError::NoManager)));
}

#[test]
fn a_credential_no_authorization_value_carries_is_refused_with_its_cause() {
    let refused = |auth| {
        let config = ManagerConfig {
            base: SecretUrl::new("http://127.0.0.1:9/fhir/"),
            auth,
            members: members(&[("node-a", DOMAIN_A), ("node-b", DOMAIN_B)]),
        };
        PixmResolver::from_config(vec![config], namespaces(), &registry())
            .expect_err("the credential is refused")
    };
    let bearer = refused(PixAuth::Bearer(SecretString::from("Qz7left Qz7right")));
    assert!(
        matches!(
            &bearer,
            PixmConfigError::Credentials(InvalidCredentials::NotB64Token)
        ),
        "a bearer token is the b64token of RFC 6750 §2.1: {bearer:?}"
    );
    let basic = refused(PixAuth::Basic {
        user: "gate:way".to_owned(),
        password: SecretString::from("Qz7left"),
    });
    assert!(
        matches!(
            &basic,
            PixmConfigError::Credentials(InvalidCredentials::ColonInUserId)
        ),
        "a basic user-id carries no colon (RFC 7617 §2): {basic:?}"
    );
    for error in [&bearer, &basic] {
        let mut rendered = format!("{error}\n{error:?}");
        let mut cause = std::error::Error::source(error);
        while let Some(source) = cause {
            rendered.push_str(&source.to_string());
            cause = source.source();
        }
        assert!(
            !rendered.contains("Qz7"),
            "the refusal quotes no secret: {rendered}"
        );
    }
}
