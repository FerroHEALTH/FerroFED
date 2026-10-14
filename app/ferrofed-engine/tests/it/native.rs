// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The pass-through to a member's own exchange-format endpoint: the
//! `Patient/$summary` request names the member's `ehr_id` alone (§5.4.1,
//! N33), leaves with the member's onward credential, the caller's identity
//! and the minted request id, passes the outbound gate, and returns the
//! member's document byte for byte; a member that does not answer is
//! reported with its §11.1 status, never as an empty document (§11.2).
//! Asserted on what the mock member received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::Contact;
use ferrofed_engine::dispatch::{DispatchOptions, NodeClients, SharedCredentials};
use ferrofed_engine::hygiene::{Part, Withheld};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::forward::ForwardError;
use ferrofed_engine::single_node::native::{Answer, SummaryAsked, Unusable};
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Credentials, ReqwestTransport};
use secrecy::SecretString;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The member's own `ehr_id`.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The identifier system the member's `ehr_id` is written in, in the
/// example arc.
const EHR_SYSTEM: &str = "urn:oid:2.999.4.1";

/// The EPS composition profile.
const EPS: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// The patient identifier resolution consumed, synthetic.
const PATIENT: &str = "SENTINEL-PATIENT-6h2w";

/// The onward credential of the member's Query API endpoint.
const TOKEN: &str = "synthetic-onward-token";

/// A document whose whitespace and member order no serialiser would keep.
const DOCUMENT: &[u8] = b"{ \"type\" :\"document\",\n  \"resourceType\": \"Bundle\" , \"identifier\": {\"value\" : \"synthetic-doc-a\", \"system\":\"urn:oid:2.999.4.2\"}, \"entry\":[ ]\t}\n";

/// The registry of node A: its Query API endpoint at `query`, and its
/// native endpoint at `native`, serving patient summaries.
fn registry(query: &str, native: &str) -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\n\
         system_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\n\
         url = \"{query}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n\n\
         [[endpoint]]\nid = \"node-a-eps\"\nnode = \"node-a\"\nurl = \"{native}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [\"Patient-Summaries\"]\nprofiles = [\"{EPS}\"]\nehr_id_system = \"{EHR_SYSTEM}\"\n"
    ))?)
}

/// A native member answering the summary request with `response`.
async fn member(response: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$summary"))
        .respond_with(response)
        .mount(&server)
        .await;
    server
}

/// Asks node A, whose native endpoint is `native`, for its summary under a
/// deadline `within` from now, withholding `withheld`.
async fn asked(native: &str, within: Duration, withheld: &str) -> Result<Answer, Box<dyn Error>> {
    let query = Server::start().await;
    let snapshot = registry(&query.uri(), native)?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let query_endpoint = EndpointId::new("node-a-pub")?;
    let provider: SharedCredentials = Arc::new(Credentials::bearer(TOKEN));
    let credentials = BTreeMap::from([(query_endpoint.clone(), provider)]);
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &credentials)?;
    let client = clients.get(&query_endpoint).ok_or("node A's client")?;
    let endpoint = snapshot
        .native_endpoint(&EndpointId::new("node-a-eps")?)
        .ok_or("node A's native endpoint")?;
    let ehr_id = EhrId::new(EHR)?;
    let deadline = Instant::now()
        .checked_add(within)
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, crate::conveyed::conveyance())
        .with_request_id(OutboundId::mint())
        .with_withheld(Arc::new(Withheld::new([SecretString::from(
            withheld.to_owned(),
        )])));
    let asked = SummaryAsked {
        native: endpoint,
        ehr_id: &ehr_id,
        profile: EPS,
    };
    let until = tokio::time::Instant::from_std(deadline);
    Ok(client.native_summary(asked, &options, until).await)
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_member_is_asked_by_its_ehr_id_alone_and_its_document_comes_back_unchanged()
-> TestResult {
    let native =
        member(ResponseTemplate::new(200).set_body_raw(DOCUMENT.to_vec(), "application/fhir+json"))
            .await;
    let answer = asked(&native.uri(), Duration::from_secs(5), PATIENT).await?;
    let Answer::Document(document) = &answer else {
        return Err(format!("expected the member's document, got {answer:?}").into());
    };
    assert_eq!(
        document.forwarded().body(),
        DOCUMENT,
        "the member's bytes, unchanged"
    );
    assert_eq!(
        document.identifier(),
        ("urn:oid:2.999.4.2", "synthetic-doc-a"),
        "bdl-9: the document's own identifier"
    );
    assert_eq!(answer.status(), EndpointStatus::Active);
    let requests = native.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("one request, got {}", requests.len()).into());
    };
    let pairs: Vec<(String, String)> = request
        .url
        .query_pairs()
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    assert_eq!(
        pairs,
        [
            ("identifier".to_owned(), format!("{EHR_SYSTEM}|{EHR}")),
            ("profile".to_owned(), EPS.to_owned()),
        ],
        "N33: the member's ehr_id alone, in its declared system"
    );
    let header = |name: &str| {
        request
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    assert_eq!(
        header("authorization"),
        Some(format!("Bearer {TOKEN}")),
        "the member's onward credential"
    );
    assert!(
        header("openEHR-federation-client").is_some(),
        "§13.1, N24: the caller's identity"
    );
    assert!(header("x-request-id").is_some(), "the minted request id");
    assert_eq!(header("accept").as_deref(), Some("application/fhir+json"));
    let mut wire = request.url.as_str().as_bytes().to_vec();
    for (name, value) in &request.headers {
        wire.extend_from_slice(name.as_str().as_bytes());
        wire.extend_from_slice(value.as_bytes());
    }
    assert!(
        !wire
            .windows(PATIENT.len())
            .any(|window| window == PATIENT.as_bytes()),
        "N33: no patient identifier in the path, the query or a header"
    );
    Ok(())
}

// conformance: CP-26 track-10
#[tokio::test]
async fn a_request_that_would_carry_a_withheld_identifier_is_never_sent() -> TestResult {
    let native = member(ResponseTemplate::new(200)).await;
    let answer = asked(&native.uri(), Duration::from_secs(5), "2.999.4.1").await?;
    assert!(
        matches!(
            answer,
            Answer::Failed(ForwardError::Withheld {
                part: Part::Url,
                ..
            })
        ),
        "§5.4.1, N33: the gate refuses, naming the part: {answer:?}"
    );
    assert_eq!(answer.status(), EndpointStatus::NodeError);
    assert!(
        native
            .received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty(),
        "nothing was sent"
    );
    Ok(())
}

#[tokio::test]
async fn a_member_that_holds_no_summary_answered() -> TestResult {
    let native = member(ResponseTemplate::new(404)).await;
    let answer = asked(&native.uri(), Duration::from_secs(5), PATIENT).await?;
    assert!(matches!(answer, Answer::Absent), "{answer:?}");
    assert!(answer.answered());
    Ok(())
}

#[tokio::test]
async fn a_member_that_fails_is_a_node_error_never_an_empty_document() -> TestResult {
    let native = member(ResponseTemplate::new(500)).await;
    let answer = asked(&native.uri(), Duration::from_secs(5), PATIENT).await?;
    assert!(
        matches!(answer, Answer::Erred(status) if status.is_server_error()),
        "§11.2: {answer:?}"
    );
    assert!(!answer.answered());
    assert_eq!(answer.status(), EndpointStatus::NodeError);
    Ok(())
}

#[tokio::test]
async fn a_member_that_refuses_the_onward_credential_is_a_node_error() -> TestResult {
    let native = member(ResponseTemplate::new(401)).await;
    let answer = asked(&native.uri(), Duration::from_secs(5), PATIENT).await?;
    assert!(
        matches!(answer, Answer::Failed(ForwardError::Refused { .. })),
        "{answer:?}"
    );
    assert_eq!(answer.status(), EndpointStatus::NodeError);
    Ok(())
}

#[tokio::test]
async fn a_slow_member_is_a_time_out() -> TestResult {
    let native = member(ResponseTemplate::new(200).set_delay(Duration::from_secs(2))).await;
    let answer = asked(&native.uri(), Duration::from_millis(200), PATIENT).await?;
    assert_eq!(
        answer.status(),
        EndpointStatus::TimeOut,
        "§11.2: {answer:?}"
    );
    assert!(!answer.answered());
    Ok(())
}

#[tokio::test]
async fn an_unreachable_member_is_offline() -> TestResult {
    let answer = asked(
        ferrofed_testkit::unreachable::BASE,
        Duration::from_secs(5),
        PATIENT,
    )
    .await?;
    assert_eq!(
        answer.status(),
        EndpointStatus::Offline,
        "§11.2: {answer:?}"
    );
    Ok(())
}

#[tokio::test]
async fn the_query_names_the_identifier_and_the_profile() -> TestResult {
    let native = Server::start().await;
    Mock::given(method("GET"))
        .and(path("/fhir/Patient/$summary"))
        .and(query_param("identifier", format!("{EHR_SYSTEM}|{EHR}")))
        .and(query_param("profile", EPS))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(DOCUMENT.to_vec(), "application/fhir+json"),
        )
        .mount(&native)
        .await;
    let answer = asked(&native.uri(), Duration::from_secs(5), PATIENT).await?;
    assert!(matches!(answer, Answer::Document(_)), "{answer:?}");
    Ok(())
}

/// What a member's success `body` is read as.
async fn read_as(body: &[u8]) -> Result<Answer, Box<dyn Error>> {
    let native =
        member(ResponseTemplate::new(200).set_body_raw(body.to_vec(), "application/fhir+json"))
            .await;
    asked(&native.uri(), Duration::from_secs(5), PATIENT).await
}

/// Holds `answer` to be a success that is no document, for a reason
/// `expected` accepts: a node error, never a document (FHIR R4 `bdl-9`).
fn unusable(answer: &Answer, expected: fn(&Unusable) -> bool) {
    assert!(
        matches!(answer, Answer::Unusable { status, reason } if status.is_success() && expected(reason)),
        "{answer:?}"
    );
    assert!(!answer.answered(), "§11.2: {answer:?}");
    assert_eq!(
        answer.status(),
        EndpointStatus::NodeError,
        "§11.1: {answer:?}"
    );
    assert_eq!(
        answer.contact(),
        Contact::Answered(http::StatusCode::OK),
        "the member answered"
    );
}

#[tokio::test]
async fn a_document_with_no_identifier_is_a_node_error() -> TestResult {
    let answer =
        read_as(b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"entry\":[]}").await?;
    unusable(&answer, |reason| matches!(reason, Unusable::NoIdentifier));
    Ok(())
}

#[tokio::test]
async fn a_document_whose_identifier_lacks_a_system_or_a_value_is_a_node_error() -> TestResult {
    let bodies: [&[u8]; 3] = [
        b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"value\":\"synthetic-doc-x\"}}",
        b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":\"urn:oid:2.999.4.3\"}}",
        b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":{\"system\":\"\",\"value\":\"synthetic-doc-x\"}}",
    ];
    for body in bodies {
        let answer = read_as(body).await?;
        unusable(&answer, |reason| matches!(reason, Unusable::NoIdentifier));
    }
    Ok(())
}

#[tokio::test]
async fn an_answer_that_is_no_fhir_resource_is_a_node_error() -> TestResult {
    let bodies: [&[u8]; 3] = [
        b"synthetic text that is no FHIR resource",
        b"[]",
        b"{\"resourceType\":\"Bundle\",\"type\":\"document\",\"identifier\":[]}",
    ];
    for body in bodies {
        let answer = read_as(body).await?;
        unusable(&answer, |reason| matches!(reason, Unusable::Unreadable(_)));
    }
    Ok(())
}

#[tokio::test]
async fn a_resource_other_than_a_document_bundle_is_a_node_error() -> TestResult {
    let identified =
        "\"identifier\":{\"system\":\"urn:oid:2.999.4.3\",\"value\":\"synthetic-doc-x\"}";
    let patient = format!("{{\"resourceType\":\"Patient\",{identified}}}");
    let answer = read_as(patient.as_bytes()).await?;
    unusable(&answer, |reason| matches!(reason, Unusable::NotBundle));
    let collection =
        format!("{{\"resourceType\":\"Bundle\",\"type\":\"collection\",{identified}}}");
    let answer = read_as(collection.as_bytes()).await?;
    unusable(&answer, |reason| matches!(reason, Unusable::NotDocument));
    Ok(())
}
