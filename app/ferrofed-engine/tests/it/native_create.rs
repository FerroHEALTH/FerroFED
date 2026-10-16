// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The create of one resource at a member's own exchange-format endpoint:
//! the body leaves byte for byte, once (§5.4 scope note, N22), the path,
//! query and headers pass the outbound gate (§5.4.1, N33), and the member's
//! answer is read by its status, an answer past the engine's body bound
//! included. Asserted on what the mock member received.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{DispatchOptions, NodeClients};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::native_create::{CreateAsked, Created};
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use http::StatusCode;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{ReqwestTransport, Transport, TransportError};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

type TestResult = Result<(), Box<dyn Error>>;

/// The member's own `ehr_id`.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// The patient identifier resolution consumed, synthetic.
const PATIENT: &str = "SENTINEL-PATIENT-4q8d";

/// A dispensation whose whitespace and member order no serialiser would
/// keep, carrying the patient identifier as clinical content.
const BODY: &[u8] = b"{ \"status\":\"completed\",\n \"resourceType\" : \"MedicationDispense\", \"subject\":{\"identifier\":{\"value\":\"SENTINEL-PATIENT-4q8d\"}} }\n";

/// The registry of node A: its Query API endpoint at `query` and its own
/// endpoint at `native`, serving dispensations.
fn registry(query: &str, native: &str) -> Result<RegistrySnapshot, Box<dyn Error>> {
    Ok(RegistrySnapshot::from_toml_str(&format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\n\
         system_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\n\
         url = \"{query}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n\n\
         [[endpoint]]\nid = \"node-a-mpd\"\nnode = \"node-a\"\nurl = \"{native}/fhir\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n\
         categories = [\"Electronic-Dispensations\"]\n\
         profiles = [\"http://hl7.eu/fhir/mpd/StructureDefinition/MedicationDispense-eu-mpd\"]\n\
         ehr_id_system = \"urn:oid:2.999.4.1\"\n"
    ))?)
}

/// Asks node A, whose own endpoint is `native`, over `transport`, to create
/// [`BODY`], withholding [`PATIENT`].
async fn created<T: Transport + Clone>(
    transport: &T,
    native: &str,
) -> Result<Created, Box<dyn Error>> {
    let snapshot = registry("https://cdr-a.example.org/openehr", native)?;
    let clients = NodeClients::from_snapshot(&snapshot, transport, &BTreeMap::new())?;
    let client = clients
        .get(&EndpointId::new("node-a-pub")?)
        .ok_or("node A's client")?;
    let endpoint = snapshot
        .native_endpoint(&EndpointId::new("node-a-mpd")?)
        .ok_or("node A's own endpoint")?;
    let ehr_id = EhrId::new(EHR)?;
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, crate::conveyed::conveyance())
        .with_request_id(OutboundId::mint())
        .with_withheld(Arc::new(Withheld::new([SecretString::from(PATIENT)])));
    let asked = CreateAsked {
        native: endpoint,
        ehr_id: &ehr_id,
        resource: "MedicationDispense",
        body: BODY,
    };
    let until = tokio::time::Instant::from_std(deadline);
    Ok(client.native_create(asked, &options, until).await)
}

// conformance: CP-26 track-10
#[tokio::test]
async fn the_body_leaves_byte_for_byte_once_and_nothing_else_carries_the_patient() -> TestResult {
    let member = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(
            ResponseTemplate::new(201).insert_header("location", "MedicationDispense/md-1"),
        )
        .mount(&member)
        .await;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let created = created(&transport, &member.uri()).await?;
    assert!(matches!(created, Created::Made(_)), "{created:?}");
    let requests = member.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("one request, got {}", requests.len()).into());
    };
    assert_eq!(
        BODY,
        request.body.as_slice(),
        "§5.4 scope note: byte for byte"
    );
    assert!(
        !request.url.as_str().contains(PATIENT),
        "N33: not in the URL"
    );
    for (name, value) in &request.headers {
        assert!(
            !value
                .as_bytes()
                .windows(PATIENT.len())
                .any(|window| window == PATIENT.as_bytes()),
            "N33: not in {name}"
        );
    }
    Ok(())
}

#[tokio::test]
async fn a_refusal_is_read_by_its_status() -> TestResult {
    let member = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/fhir/MedicationDispense"))
        .respond_with(ResponseTemplate::new(409))
        .mount(&member)
        .await;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let created = created(&transport, &member.uri()).await?;
    assert!(
        matches!(created, Created::Refused(StatusCode::CONFLICT)),
        "{created:?}"
    );
    assert_eq!(EndpointStatus::Active, created.status());
    Ok(())
}

/// An engine that stops every answer at its body bound with its status,/// `0`, as a bounded engine does.
#[derive(Debug, Clone)]
struct Bounded(StatusCode);

#[async_trait::async_trait]
impl Transport for Bounded {
    async fn send(
        &self,
        _request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        Err(TransportError::BodyTooLarge {
            status: self.0,
            limit: 1024,
        })
    }
}

#[tokio::test]
async fn an_answer_past_the_body_bound_is_read_by_its_status() -> TestResult {
    let native = "https://cdr-a.example.org";
    let made = created(&Bounded(StatusCode::CREATED), native).await?;
    assert!(
        matches!(&made, Created::Made(made) if made.status() == StatusCode::CREATED),
        "a create the member made stays made: {made:?}"
    );
    let refused = created(&Bounded(StatusCode::UNPROCESSABLE_ENTITY), native).await?;
    assert!(
        matches!(refused, Created::Refused(StatusCode::UNPROCESSABLE_ENTITY)),
        "{refused:?}"
    );
    let erred = created(&Bounded(StatusCode::BAD_GATEWAY), native).await?;
    assert!(
        matches!(erred, Created::Erred(StatusCode::BAD_GATEWAY)),
        "{erred:?}"
    );
    Ok(())
}
