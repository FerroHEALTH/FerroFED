// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The caller's identity on every request to a node, asserted on what mock
//! nodes received: the federated query, a routed read and write, the EHR
//! calls of the admission check, a stored-query definition and the probe each
//! carry exactly one `openEHR-federation-client` token, signed for that node
//! with the key the gateway publishes, and a caller claim carrying a withheld
//! patient identifier stops the request before it is sent (§13.1, N24, N25,
//! §12.4, §5.4.1, N33, CP-16, CP-17, CP-26).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::Write as _;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::relayed::Relayed;
use ferrofed_engine::conveyance::{Caller, Conveyance, HEADER, LIFETIME, Principal, Signer};
use ferrofed_engine::dispatch::definition::DefinitionAt;
use ferrofed_engine::dispatch::{
    DispatchError, DispatchOptions, NodeClient, NodeClients, NodeQuery,
};
use ferrofed_engine::hygiene::{Part, Withheld};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_engine::single_node::ehr::EhrCallError;
use ferrofed_engine::single_node::forward::{ClientRequest, ForwardError};
use ferrofed_engine::single_node::probe::{self, Probe, ProbedEhrId};
use ferrofed_registry::id::{EhrId, EndpointId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::mock::Server;
use http::{HeaderMap, Method};
use openehr_its::json::from_canonical_json;
use openehr_its::rest::client::ReqwestTransport;
use openehr_rm::v1_2::ehr::ehr_status::EhrStatus;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{
    self, ACT_REASON, CONTACT_POINT, GATEWAY, ORGANISATION, PRACTITIONER_ROLE,
    PROFESSIONAL_AUTHORITY, PROFESSIONAL_ID, PROFESSIONAL_NAME, ReadPurpose, SCOPE, SUBJECT,
    UPSTREAM,
};

type TestResult = Result<(), Box<dyn Error>>;

/// A node-local `ehr_id`.
const EHR: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

/// A synthetic patient identifier the gateway resolved on and withholds.
const PATIENT: &str = "SYNTHETIC-PATIENT-5d2e";

/// A synthetic node query, scoped to [`EHR`].
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str =
    r##"{"q":"node","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A synthetic `EHR_STATUS` a create carries.
const EHR_STATUS: &str = r#"{"_type":"EHR_STATUS","archetype_node_id":"openEHR-EHR-EHR_STATUS.generic.v1","name":{"_type":"DV_TEXT","value":"EHR Status"},"subject":{"_type":"PARTY_SELF"},"is_queryable":true,"is_modifiable":true}"#;

/// The registry of one node per `(endpoint_id, url)`.
fn registry(endpoints: &[(&str, &str)]) -> Result<RegistrySnapshot, Box<dyn Error>> {
    let mut document = String::from("[[organisation]]\nid = \"org-a\"\n");
    for (index, (id, url)) in endpoints.iter().enumerate() {
        write!(
            document,
            "\n[[node]]\nid = \"node-{index}\"\norganisation = \"org-a\"\nsystem_id = \"cdr-{index}.example.org\"\n\n[[endpoint]]\nid = \"{id}\"\nnode = \"node-{index}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
        )?;
    }
    Ok(RegistrySnapshot::from_toml_str(&document)?)
}

/// The client of the one endpoint `node-a-pub` at `url`.
fn client(url: &str) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let snapshot = registry(&[("node-a-pub", url)])?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?)
}

/// Options conveying `conveyance`, withholding [`PATIENT`].
fn options(conveyance: Conveyance) -> Result<DispatchOptions, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    Ok(DispatchOptions::new(deadline, conveyance)
        .with_request_id(OutboundId::mint())
        .with_withheld(Arc::new(Withheld::new([SecretString::from(PATIENT)]))))
}

/// A node answering `verb` at `at` with `answer`.
async fn node(verb: &str, at: &str, answer: ResponseTemplate) -> Server {
    let server = Server::start().await;
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

/// A node answering every query with an empty result set.
async fn query_node() -> Server {
    node(
        "POST",
        "/v1/query/aql",
        ResponseTemplate::new(200)
            .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
    )
    .await
}

/// The one conveyed token of every request `server` received, in order; a
/// request with none, or with more than one, fails the read.
async fn tokens(server: &Server) -> Result<Vec<String>, Box<dyn Error>> {
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let mut tokens = Vec::new();
    for request in requests {
        let values: Vec<_> = request.headers.get_all(HEADER).iter().collect();
        let [only] = values.as_slice() else {
            return Err(format!("{} carries {} {HEADER} values", request.url, values.len()).into());
        };
        tokens.push(only.to_str()?.to_owned());
    }
    Ok(tokens)
}

/// The one token `server` received, verified for `audience` and read.
async fn one_read(server: &Server, audience: &str) -> Result<conveyed::Read, Box<dyn Error>> {
    let tokens = tokens(server).await?;
    let [token] = tokens.as_slice() else {
        return Err(format!("{} requests", tokens.len()).into());
    };
    conveyed::verified(token, conveyed::shared().keys(), (GATEWAY, audience))
}

/// Asserts that `read` names the synthetic caller.
fn names_the_caller(read: &conveyed::Read) {
    assert_eq!(SUBJECT, read.sub, "sub is the caller");
    assert_eq!(
        Some(UPSTREAM),
        read.iss_upstream.as_deref(),
        "iss_upstream is the issuer that vouched for the caller"
    );
    assert_eq!(
        Some("signature"),
        read.verified_by.as_deref(),
        "verified_by says how"
    );
    assert_eq!(
        Some(ORGANISATION),
        read.subject_organization_id.as_deref(),
        "the caller's organisation"
    );
    assert_eq!(
        vec![ReadPurpose {
            system: Some(ACT_REASON.to_owned()),
            code: "TREAT".to_owned(),
        }],
        read.purpose_of_use,
        "the purpose of use, HL7 v3 coded (§13.4)"
    );
    assert_eq!(
        Some(SCOPE),
        read.scope.as_deref(),
        "the caller's scopes as granted (N26)"
    );
    assert_eq!(
        (Some(PROFESSIONAL_NAME), Some(PROFESSIONAL_ID)),
        (
            read.subject_name.as_deref(),
            read.national_provider_identifier.as_deref()
        ),
        "the professional's identification, as IHE IUA names it"
    );
    assert_eq!(
        (
            Some(PROFESSIONAL_AUTHORITY),
            [ReadPurpose {
                system: Some(PRACTITIONER_ROLE.to_owned()),
                code: "doctor".to_owned(),
            }]
            .as_slice()
        ),
        (
            read.national_provider_identifier_authority.as_deref(),
            read.subject_role.as_slice()
        ),
        "the identifier's issuing authority and the professional's role (2025/327 Art 13(4))"
    );
    assert_eq!(Some("person"), read.acting.as_deref(), "a person acts");
    assert_eq!(
        Some("substantial"),
        read.assurance_level.as_deref(),
        "the authentication's assurance level (Annex II 3.1)"
    );
    assert_eq!(
        None, read.national_contact_point,
        "no contact point relays this caller"
    );
}

/// A relayed caller of [`conveyed::relayed`].
fn relayed_caller() -> Caller {
    let mut caller = conveyed::caller();
    caller.relayed = Some(Box::new(conveyed::relayed()));
    caller
}

/// Implementing Regulation (EU) 2026/2099 Art 7, Annex Tables 1 and 2; §13.4
/// authn-end-user: a node is told every attribute a contact point relays,
/// each under its Annex data identifier, marked as the contact point's
/// assertion.
// conformance: CP-16
#[tokio::test]
async fn a_relayed_caller_conveys_every_annex_attribute_marked_as_asserted() -> TestResult {
    let server = query_node().await;
    client(&server.uri())?
        .query(
            &NodeQuery::new(NODE_AQL),
            &options(conveyed::conveyance_of(relayed_caller()))?,
        )
        .await?;
    let read = one_read(&server, "node-a-pub").await?;
    let told = read
        .national_contact_point
        .ok_or("the relayed attributes")?;
    let relayed = conveyed::relayed();
    assert_eq!(CONTACT_POINT, told.asserted_by);
    let professional = &told.health_professional;
    assert_eq!(
        (
            "Example-Family",
            "Example-Given",
            "XA",
            "XA-HP-0042",
            "Example Professional Register",
            "XA-HCP-0007",
        ),
        (
            professional.family_name.as_str(),
            professional.given_name.as_str(),
            professional.country_code.as_str(),
            professional.hp_identifier.as_str(),
            professional.issuing_authority_name.as_str(),
            professional.healthcare_provider_identifier.as_str(),
        ),
        "Annex Table 1"
    );
    assert_eq!(
        vec![ReadPurpose {
            system: Some("urn:oid:2.999.9".to_owned()),
            code: "physician".to_owned(),
        }],
        professional.hp_professional_role
    );
    let provider = &told.healthcare_provider;
    assert_eq!(
        (
            relayed.provider.identifier.as_str(),
            "Example Provider Register",
            "Example Hospital",
            "1 Example Street, Example City",
        ),
        (
            provider.healthcare_provider_identifier.as_str(),
            provider.issuing_authority_name.as_str(),
            provider.healthcare_provider_name.as_str(),
            provider.healthcare_provider_address.as_str(),
        ),
        "Annex Table 2"
    );
    Ok(())
}

// conformance: CP-16 CP-17
#[tokio::test]
async fn a_query_conveys_the_caller_in_a_token_the_published_key_verifies() -> TestResult {
    let server = query_node().await;
    client(&server.uri())?
        .query(&NodeQuery::new(NODE_AQL), &options(conveyed::conveyance())?)
        .await?;
    let read = one_read(&server, "node-a-pub").await?;
    assert_eq!(GATEWAY, read.iss);
    assert_eq!("node-a-pub", read.aud);
    names_the_caller(&read);
    let lifetime = read.exp.checked_sub(read.iat).ok_or("exp after iat")?;
    assert!(
        lifetime > 0 && u64::try_from(lifetime)? <= LIFETIME.as_secs() && LIFETIME.as_secs() <= 60,
        "exp is at most 60 s after iat: {lifetime}"
    );
    let jti = uuid::Uuid::parse_str(&read.jti)?;
    assert_eq!(Some(uuid::Version::Random), jti.get_version());
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn each_node_is_the_audience_of_its_own_token() -> TestResult {
    let (a, b) = (query_node().await, query_node().await);
    let snapshot = registry(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?;
    let options = options(conveyed::conveyance())?;
    for client in clients.iter() {
        client.query(&NodeQuery::new(NODE_AQL), &options).await?;
    }
    let at_a = one_read(&a, "node-a-pub").await?;
    let at_b = one_read(&b, "node-b-pub").await?;
    assert_eq!(
        ("node-a-pub", "node-b-pub"),
        (at_a.aud.as_str(), at_b.aud.as_str())
    );
    assert_ne!(at_a.jti, at_b.jti, "a fresh jti per token");
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn a_routed_read_and_a_routed_write_convey_the_caller() -> TestResult {
    let read_at = format!("/ehr/{EHR}");
    let write_at = format!("/ehr/{EHR}/composition");
    let server = Server::start().await;
    for (verb, at, status) in [("GET", &read_at, 200), ("POST", &write_at, 201)] {
        Mock::given(method(verb))
            .and(path(format!("/v1{at}")))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
    }
    let client = client(&server.uri())?;
    let mut write_headers = HeaderMap::new();
    write_headers.insert("content-type", "application/json".parse()?);
    for (verb, at, headers, body) in [
        (Method::GET, &read_at, HeaderMap::new(), Vec::new()),
        (
            Method::POST,
            &write_at,
            write_headers,
            b"{\"_type\":\"COMPOSITION\"}".to_vec(),
        ),
    ] {
        let request = ClientRequest {
            method: verb,
            path: at.clone(),
            query: None,
            headers,
            body,
        };
        client
            .forward(request, &options(conveyed::conveyance())?)
            .await?;
    }
    let tokens = tokens(&server).await?;
    assert_eq!(2, tokens.len(), "the read and the write");
    for token in &tokens {
        let read = conveyed::verified(token, conveyed::shared().keys(), (GATEWAY, "node-a-pub"))?;
        names_the_caller(&read);
    }
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn the_admission_checks_ehr_create_and_read_convey_the_gateway() -> TestResult {
    let server = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/ehr"))
        .respond_with(
            ResponseTemplate::new(201).insert_header("ETag", format!("\"{EHR}\"").as_str()),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/ehr/{EHR}")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            format!(r#"{{"system_id":{{"value":"cdr-0.example.org"}},"ehr_id":{{"value":"{EHR}"}},"time_created":{{"value":"2026-10-04T00:00:00Z"}}}}"#)
                .into_bytes(),
            "application/json",
        ))
        .mount(&server)
        .await;
    let client = client(&server.uri())?;
    let gateway = Conveyance::new(conveyed::shared(), Principal::Gateway);
    let status: EhrStatus = from_canonical_json(EHR_STATUS)?;
    client
        .create_ehr(&status, &options(gateway.clone())?)
        .await?;
    let read = client.read_ehr(EHR, &options(gateway)?).await;
    let tokens = tokens(&server).await?;
    assert_eq!(
        2,
        tokens.len(),
        "the create and the read were both sent: {read:?}"
    );
    for token in &tokens {
        let read = conveyed::verified(token, conveyed::shared().keys(), (GATEWAY, "node-a-pub"))?;
        assert_eq!(GATEWAY, read.sub, "the gateway acts for its operator");
        assert_eq!(None, read.iss_upstream);
        assert_eq!(None, read.verified_by);
        assert_eq!(None, read.subject_organization_id);
        assert!(read.purpose_of_use.is_empty());
        assert_eq!(None, read.scope);
        assert_eq!(
            (None, None, None, None),
            (
                read.subject_name,
                read.national_provider_identifier,
                read.acting,
                read.assurance_level
            ),
            "the gateway names no professional and no assurance of its own"
        );
        assert_eq!(None, read.national_provider_identifier_authority);
        assert!(read.subject_role.is_empty());
    }
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn a_stored_query_definition_conveys_the_caller() -> TestResult {
    let server = node(
        "PUT",
        "/v1/definition/query/org.example::conveyed/1.0.0",
        ResponseTemplate::new(200),
    )
    .await;
    let at = DefinitionAt {
        name: "org.example::conveyed",
        version: "1.0.0",
    };
    client(&server.uri())?
        .store_definition(at, NODE_AQL, &options(conveyed::conveyance())?)
        .await?;
    names_the_caller(&one_read(&server, "node-a-pub").await?);
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn a_probe_conveys_the_caller_to_every_member() -> TestResult {
    let at = format!("/v1/ehr/{EHR}");
    let (a, b) = (
        node("GET", &at, ResponseTemplate::new(404)).await,
        node("GET", &at, ResponseTemplate::new(200)).await,
    );
    let snapshot = registry(&[("node-a-pub", &a.uri()), ("node-b-pub", &b.uri())])?;
    let transport = ReqwestTransport::with_timeout(Duration::from_secs(10))?;
    let clients = NodeClients::from_snapshot(&snapshot, &transport, &BTreeMap::new())?;
    let until = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let probe = Probe {
        ehr_id: ProbedEhrId::try_from(&EhrId::new(EHR)?)?,
        headers: HeaderMap::new(),
        per_node: until,
        overall: until,
        request_id: OutboundId::mint(),
        conveyance: conveyed::conveyance(),
    };
    let endpoints = [
        EndpointId::new("node-a-pub")?,
        EndpointId::new("node-b-pub")?,
    ];
    probe::ask_all(&clients, &endpoints, &probe).await?;
    names_the_caller(&one_read(&a, "node-a-pub").await?);
    names_the_caller(&one_read(&b, "node-b-pub").await?);
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn a_conveyance_header_from_the_client_never_reaches_the_node() -> TestResult {
    let at = format!("/ehr/{EHR}");
    let server = node("GET", &format!("/v1{at}"), ResponseTemplate::new(200)).await;
    let mut headers = HeaderMap::new();
    headers.insert(HEADER, "forged-by-the-client".parse()?);
    headers.insert("authorization", "Bearer the-callers-own-token".parse()?);
    let request = ClientRequest {
        method: Method::GET,
        path: at,
        query: None,
        headers,
        body: Vec::new(),
    };
    client(&server.uri())?
        .forward(request, &options(conveyed::conveyance())?)
        .await?;
    names_the_caller(&one_read(&server, "node-a-pub").await?);
    let requests = server.received_requests().await.ok_or("recording is on")?;
    let sent = requests.first().ok_or("one request")?;
    assert!(
        sent.headers.get("authorization").is_none(),
        "the caller's own token is never forwarded (§13.1)"
    );
    Ok(())
}

// conformance: CP-16
#[tokio::test]
async fn a_node_the_gateway_has_a_grant_at_knows_it_by_its_client_id() -> TestResult {
    let server = query_node().await;
    let endpoint = EndpointId::new("node-a-pub")?;
    let signer: Signer = conveyed::signer()?.with_issuer_at(endpoint, "ferrofed-at-node-a");
    let signer = Arc::new(signer);
    let conveyance = Conveyance::new(Arc::clone(&signer), Principal::Caller(conveyed::caller()));
    client(&server.uri())?
        .query(&NodeQuery::new(NODE_AQL), &options(conveyance)?)
        .await?;
    let tokens = tokens(&server).await?;
    let token = tokens.first().ok_or("one token")?;
    let read = conveyed::verified(token, signer.keys(), ("ferrofed-at-node-a", "node-a-pub"))?;
    assert_eq!("ferrofed-at-node-a", read.iss);
    Ok(())
}

/// The synthetic caller with `set` applied, each claim a way the withheld
/// identifier could enter the token.
fn carrying(set: fn(&mut Caller)) -> Caller {
    let mut caller = conveyed::caller();
    set(&mut caller);
    caller
}

/// Every caller whose claims carry [`PATIENT`], one claim each.
fn smuggling() -> Vec<Caller> {
    vec![
        carrying(|caller| PATIENT.clone_into(&mut caller.subject)),
        carrying(|caller| caller.organisation = Some(format!("urn:x:{PATIENT}"))),
        carrying(|caller| caller.issuer = format!("https://{PATIENT}.example.test")),
        carrying(|caller| caller.scope = format!("user/aql-{PATIENT}.s")),
        carrying(|caller| caller.professional.name = Some(PATIENT.to_owned())),
        carrying(|caller| caller.professional.identifier = Some(format!("urn:x:{PATIENT}"))),
        carrying(|caller| caller.professional.issuing_authority = Some(PATIENT.to_owned())),
        carrying(|caller| {
            if let Some(role) = caller.professional.roles.first_mut() {
                PATIENT.clone_into(&mut role.code);
            }
        }),
        carrying(|caller| {
            if let Some(role) = caller.professional.roles.first_mut() {
                role.system = Some(format!("urn:x:{PATIENT}"));
            }
        }),
        carrying(|caller| {
            if let Some(purpose) = caller.purposes.first_mut() {
                PATIENT.clone_into(&mut purpose.code);
            }
        }),
        relaying(|relayed| relayed.contact_point = format!("https://{PATIENT}.example.test")),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.country_code)),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.professional.family_name)),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.professional.given_name)),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.professional.hp_identifier)),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.professional.issuing_authority_name)),
        relaying(|relayed| {
            if let Some(role) = relayed.professional.hp_professional_role.first_mut() {
                PATIENT.clone_into(&mut role.code);
            }
        }),
        relaying(|relayed| {
            PATIENT.clone_into(&mut relayed.provider.identifier);
        }),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.provider.issuing_authority_name)),
        relaying(|relayed| PATIENT.clone_into(&mut relayed.provider.name)),
        relaying(|relayed| {
            relayed.provider.address = format!("{PATIENT} Example Street");
        }),
    ]
}

/// A relayed caller with `set` applied to what the contact point relays.
fn relaying(set: fn(&mut Relayed)) -> Caller {
    let mut relayed = conveyed::relayed();
    set(&mut relayed);
    let mut caller = conveyed::caller();
    caller.relayed = Some(Box::new(relayed));
    caller
}

// conformance: CP-26
#[tokio::test]
async fn a_caller_claim_carrying_the_withheld_identifier_stops_a_query_unsent() -> TestResult {
    for caller in smuggling() {
        let server = query_node().await;
        let sent = client(&server.uri())?
            .query(
                &NodeQuery::new(NODE_AQL),
                &options(conveyed::conveyance_of(caller))?,
            )
            .await;
        assert!(
            matches!(
                sent,
                Err(DispatchError::Withheld {
                    part: Part::Header(HEADER),
                    ..
                })
            ),
            "{sent:?}"
        );
        assert!(
            tokens(&server).await?.is_empty(),
            "nothing reached the node"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_caller_claim_carrying_the_withheld_identifier_stops_a_routed_request_unsent()
-> TestResult {
    for caller in smuggling() {
        let at = format!("/ehr/{EHR}");
        let server = node("GET", &format!("/v1{at}"), ResponseTemplate::new(200)).await;
        let request = ClientRequest {
            method: Method::GET,
            path: at,
            query: None,
            headers: HeaderMap::new(),
            body: Vec::new(),
        };
        let sent = client(&server.uri())?
            .forward(request, &options(conveyed::conveyance_of(caller))?)
            .await;
        assert!(
            matches!(
                sent,
                Err(ForwardError::Withheld {
                    part: Part::Header(HEADER),
                    ..
                })
            ),
            "{sent:?}"
        );
        assert!(
            tokens(&server).await?.is_empty(),
            "nothing reached the node"
        );
    }
    Ok(())
}

// conformance: CP-26
#[tokio::test]
async fn a_caller_claim_carrying_the_withheld_identifier_stops_an_ehr_call_and_a_definition()
-> TestResult {
    let server = Server::start().await;
    let client = client(&server.uri())?;
    let status: EhrStatus = from_canonical_json(EHR_STATUS)?;
    let caller = carrying(|caller| PATIENT.clone_into(&mut caller.subject));
    let created = client
        .create_ehr(&status, &options(conveyed::conveyance_of(caller.clone()))?)
        .await;
    assert!(
        matches!(
            created,
            Err(EhrCallError::Withheld {
                part: Part::Header(HEADER),
                ..
            })
        ),
        "{created:?}"
    );
    let at = DefinitionAt {
        name: "org.example::conveyed",
        version: "1.0.0",
    };
    let stored = client
        .store_definition(at, NODE_AQL, &options(conveyed::conveyance_of(caller))?)
        .await;
    assert!(
        matches!(
            stored,
            Err(DispatchError::Withheld {
                part: Part::Header(HEADER),
                ..
            })
        ),
        "{stored:?}"
    );
    assert!(
        tokens(&server).await?.is_empty(),
        "nothing reached the node"
    );
    Ok(())
}
