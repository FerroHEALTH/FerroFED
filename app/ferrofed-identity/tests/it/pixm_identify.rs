// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The PIXm resolver names the patient behind an `ehr_id` for the access log
//! (Regulation (EU) 2025/327 Art 9(1)): one ITI-83 whose source identifier is
//! the `ehr_id` in the member's domain and whose target system is the
//! assigning authority of each namespace asked (PIXm 3.1.0 §2:3.83.4.1.2),
//! the unknown source as `Unknown`, and every failure as `Unavailable` with
//! no identifier in its error.
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use ferrofed_identity::fhir::{Authentication, Tls};
use ferrofed_identity::ihe::pixm::{ManagerConfig, PixmResolver};
use ferrofed_identity::role::behalf::OnBehalfOf;
use ferrofed_identity::role::patient::IdentifierNamespace;
use ferrofed_identity::role::resolver::{Identification, Resolver};
use ferrofed_registry::id::{EhrId, NodeId};
use ferrofed_registry::secret::SecretUrl;
use ferrofed_testkit::mock::Server;
use ihe_iti::pixm::Invocation;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

use crate::support::registry;

/// The synthetic patient identifier the Manager answers with.
const SENTINEL: &str = "SENTINEL-PIX-IDENTIFY-44";

/// The namespace the access log asks in, and the assigning authority it maps
/// to.
const NAMESPACE: &str = "2.999.1";
const PATIENT_SYSTEM: &str = "urn:oid:2.999.1";

/// The `ehr_id` domains of node A and node B.
const DOMAIN_A: &str = "urn:oid:2.999.10";
const DOMAIN_B: &str = "urn:oid:2.999.20";

/// The `ehr_id` at node A the access reached.
const EHR_A: &str = "3333aaaa-3333-4333-8333-333333333333";

const OPERATION: &str = "/fhir/Patient/$ihe-pix";
const FHIR_JSON: &str = "application/fhir+json";

fn node(id: &str) -> NodeId {
    NodeId::new(id).expect("a node id")
}

fn namespace(value: &str) -> IdentifierNamespace {
    IdentifierNamespace::new(value).expect("a namespace")
}

fn resolver(server: &Server) -> PixmResolver {
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
        BTreeMap::from([(namespace(NAMESPACE), PATIENT_SYSTEM.to_owned())]),
        &registry(),
    )
    .expect("the resolver builds")
}

async fn stub(status: u16, body: &str) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .respond_with(
            ResponseTemplate::new(status).set_body_raw(body.as_bytes().to_vec(), FHIR_JSON),
        )
        .mount(&server)
        .await;
    server
}

async fn identify(resolver: &PixmResolver, namespaces: &[&str]) -> Identification {
    let asked: Vec<IdentifierNamespace> = namespaces.iter().map(|value| namespace(value)).collect();
    resolver
        .identify(
            &node("node-a"),
            &EhrId::new(EHR_A).expect("an ehr_id"),
            &asked,
            &OnBehalfOf::Gateway,
            Instant::now() + Duration::from_secs(5),
        )
        .await
}

/// Every error message in the chain of `identification`, with its `Debug`.
fn rendered(identification: &Identification) -> String {
    let mut text = format!("{identification:?}");
    if let Identification::Unavailable(error) = identification {
        let mut current: Option<&dyn std::error::Error> = Some(error);
        while let Some(error) = current {
            text.push_str(&error.to_string());
            current = error.source();
        }
    }
    text
}

#[tokio::test]
async fn the_ehr_id_in_the_members_domain_names_the_patient() {
    let body = format!(
        r#"{{"resourceType":"Parameters","parameter":[{{"name":"targetIdentifier","valueIdentifier":{{"system":"{PATIENT_SYSTEM}","value":"{SENTINEL}"}}}}]}}"#
    );
    let server = stub(200, &body).await;
    let identification = identify(&resolver(&server), &[NAMESPACE]).await;
    let Identification::Named(named) = &identification else {
        panic!("the patient is named: {identification:?}");
    };
    assert_eq!(1, named.len());
    assert_eq!(NAMESPACE, named[0].namespace().as_str());
    assert!(!format!("{named:?}").contains(SENTINEL));
    let requests = server.received_requests().await.expect("recording is on");
    let [request] = requests.as_slice() else {
        panic!("one ITI-83, got {}", requests.len());
    };
    let pairs: Vec<(String, String)> = request
        .url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    assert!(
        pairs.contains(&("sourceIdentifier".to_owned(), format!("{DOMAIN_A}|{EHR_A}"))),
        "{pairs:?}"
    );
    assert!(
        pairs.contains(&("targetSystem".to_owned(), PATIENT_SYSTEM.to_owned())),
        "{pairs:?}"
    );
}

#[tokio::test]
async fn an_ehr_id_the_manager_does_not_know_names_no_patient() {
    let body =
        r#"{"resourceType":"OperationOutcome","issue":[{"severity":"error","code":"not-found"}]}"#;
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path(OPERATION))
        .and(query_param(
            "sourceIdentifier",
            format!("{DOMAIN_A}|{EHR_A}"),
        ))
        .respond_with(ResponseTemplate::new(404).set_body_raw(body.as_bytes().to_vec(), FHIR_JSON))
        .mount(&server)
        .await;
    let identification = identify(&resolver(&server), &[NAMESPACE]).await;
    assert!(
        matches!(identification, Identification::Unknown),
        "{identification:?}"
    );
}

#[tokio::test]
async fn an_answer_with_no_identifier_in_the_namespace_names_no_patient() {
    let server = stub(200, r#"{"resourceType":"Parameters"}"#).await;
    let identification = identify(&resolver(&server), &[NAMESPACE]).await;
    assert!(
        matches!(identification, Identification::Unknown),
        "{identification:?}"
    );
}

#[tokio::test]
async fn a_failed_exchange_is_unavailable_and_names_no_identifier() {
    let server = stub(500, r#"{"resourceType":"OperationOutcome"}"#).await;
    let identification = identify(&resolver(&server), &[NAMESPACE]).await;
    assert!(
        matches!(identification, Identification::Unavailable(_)),
        "{identification:?}"
    );
    let text = rendered(&identification);
    assert!(!text.contains(EHR_A), "{text}");
    assert!(!text.contains(SENTINEL), "{text}");
}

#[tokio::test]
async fn a_namespace_with_no_assigning_authority_asks_nothing() {
    let server = stub(200, r#"{"resourceType":"Parameters"}"#).await;
    let identification = identify(&resolver(&server), &["2.999.9"]).await;
    assert!(
        matches!(identification, Identification::Unavailable(_)),
        "{identification:?}"
    );
    let requests = server.received_requests().await.expect("recording is on");
    assert!(requests.is_empty(), "no ITI-83 is sent");
}
