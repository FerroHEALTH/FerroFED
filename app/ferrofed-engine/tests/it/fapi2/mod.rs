// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FAPI 2.0 grant against the harness FAPI 2.0 authorization server and
//! a mock node: the server is discovered from its issuer, the client
//! authenticates with an ES256 `private_key_jwt` assertion naming the
//! issuer, asks for the configured `authorization_details`, and the
//! `DPoP`-bound token it is issued reaches the node with a proof of its key
//! (§13.1, §13.3, N25, CP-17; Annex B §B.4a; FAPI 2.0 Security Profile
//! §5.3.2.1, §5.3.3.1, §5.4.1; RFC 8414, RFC 9396, RFC 9449).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::reported::UNAUTHENTICATED;
use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, NodeQuery, NodeReply};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::authorization_details::AuthorizationDetails;
use ferrofed_engine::onward::conveyance::{Conveyance, Principal, Verification};
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::exchange::SubjectToken;
use ferrofed_engine::onward::fapi2::{Fapi2Credentials, Fapi2Exchange, Fapi2Grant};
use ferrofed_engine::onward::keys::SigningKey;
use ferrofed_engine::onward::token::{CLIENT_ASSERTION_TYPE, GRANT_TYPE};
use ferrofed_engine::onward::{Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::dpop::{self, HEADER};
use ferrofed_testkit::fapi::{AuthorizationServer, DETAILS_TYPE, METADATA_PATH};
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, Verdict};
use jsonwebtoken::Algorithm;
use nl_generic_functions::oauth_metadata;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{CredentialsProvider as _, ReqwestTransport};
use secrecy::SecretString;
use serde::Deserialize;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, conveyance, shared};

mod metadata;
mod mtls;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as:
/// a URA-shaped identifier under the example arc, no real organisation.
const CLIENT_ID: &str = "urn:oid:2.999.3.3.12345678";

/// The scope every token is requested with.
const SCOPE: &str = "system/aql-*.s";

/// The `authorization_details` of the Annex B §B.4a.3 example, with
/// synthetic values.
const DETAILS: &str = r#"[{"type":"nl-gis-v1","purpose_of_use":"http://terminology.hl7.org/CodeSystem/v3-ActReason|TREAT","locations":["https://fhir.cdr-a.example.org/fhir"],"locations_organization_id":"urn:oid:2.999.3.3.87654321"}]"#;

/// The target service a token exchange names.
const RESOURCE: &str = "https://cdr-a.example.org/openehr";

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A synthetic patient identifier the gateway resolved on and withholds.
const PATIENT: &str = "SYNTHETIC-SUBJECT-9c2e";

/// The issuer of the callers' tokens, its key generated once per process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static CALLERS: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(UPSTREAM).expect("a test key pair should generate"));

/// A fresh P-256 client key.
fn client_key() -> Result<SigningKey, Box<dyn Error>> {
    Ok(SigningKey::from_p256_pem(&SecretString::from(
        oauth::p256_pem()?,
    ))?)
}

/// A fresh P-256 `DPoP` key.
fn prover() -> Result<Arc<Prover>, Box<dyn Error>> {
    Ok(Arc::new(Prover::from_pem(&SecretString::from(
        oauth::p256_pem()?,
    ))?))
}

/// A harness server trusting `key`, requiring the [`DETAILS_TYPE`] details.
async fn server(key: &SigningKey) -> AuthorizationServer {
    let server = AuthorizationServer::start(CLIENT_ID, Some(300)).await;
    server.endpoint().trust(jsonwebtoken::jwk::JwkSet {
        keys: vec![key.public().clone()],
    });
    server
        .endpoint()
        .require_authorization_details([DETAILS_TYPE]);
    server
}

/// The grant at `server`, with `key` and `prover`, asking for [`SCOPE`] and
/// [`DETAILS`].
fn grant(
    server: &AuthorizationServer,
    key: SigningKey,
    prover: &Arc<Prover>,
) -> Result<Fapi2Grant, Box<dyn Error>> {
    Ok(Fapi2Grant::new(
        oauth_metadata::Issuer::parse(&server.issuer())?,
        CLIENT_ID,
        (key, Arc::clone(prover)),
        (
            Some(Scope::parse(SCOPE)?),
            Some(AuthorizationDetails::parse(DETAILS)?),
        ),
    )?)
}

/// The engine every request is sent through.
fn transport() -> Result<ReqwestTransport, Box<dyn Error>> {
    Ok(ReqwestTransport::with_timeout(Duration::from_secs(10))?)
}

/// The client-credentials provider of `grant`.
fn provider(grant: Fapi2Grant) -> Result<Arc<Fapi2Credentials<ReqwestTransport>>, Box<dyn Error>> {
    Ok(Arc::new(Fapi2Credentials::new(
        EndpointId::new("node-a-pub")?,
        grant,
        (Duration::from_secs(300), Duration::from_secs(5)),
        transport()?,
        Arc::new(SystemClock),
    )))
}

/// The exchange of `grant`.
fn exchange(grant: Fapi2Grant) -> Result<Arc<Fapi2Exchange<ReqwestTransport>>, Box<dyn Error>> {
    Ok(Arc::new(Fapi2Exchange::new(
        EndpointId::new("node-a-pub")?,
        grant,
        (Duration::from_secs(300), Duration::from_secs(5)),
        transport()?,
        Arc::new(SystemClock),
    )))
}

/// A mock node that answers `200` to a request carrying a token `server`
/// bound and a proof of its key, and `401` to every other.
async fn node(server: &AuthorizationServer) -> Server {
    let node = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .and(server.endpoint().dpop_bound(None))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(&node)
        .await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&node)
        .await;
    node
}

/// The client of the node at `server`, proving with `prover`.
fn client(
    server: &Server,
    prover: &Arc<Prover>,
) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
        server.uri()
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(endpoint, transport()?)?.with_dpop(prover))
}

/// Sends the node query through `client` conveying `conveyance`, withholding
/// [`PATIENT`].
async fn query(
    client: &NodeClient<ReqwestTransport>,
    conveyance: Conveyance,
) -> Result<NodeReply, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, conveyance)
        .with_withheld(Arc::new(Withheld::new([SecretString::from(PATIENT)])));
    Ok(client.query(&NodeQuery::new(NODE_AQL), &options).await?)
}

/// The text of a reply's `error`.
fn error_text(reply: &NodeReply) -> Result<String, Box<dyn Error>> {
    match reply.outcome().error() {
        Some(ErrorDetail::Text(text)) => Ok(text.clone()),
        other => Err(format!("an unexpected error: {other:?}").into()),
    }
}

/// The value of `name` in `form`.
fn field<'a>(form: &'a [(String, String)], name: &str) -> Option<&'a str> {
    form.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The claims of an assertion as the server reads its audience: `aud`
/// must be one string.
#[derive(Debug, Deserialize)]
struct AssertionAudience {
    aud: String,
}

/// A caller verified by signature, carrying a token whose `sub` is `subject`.
fn verified_caller(subject: &str) -> Result<Conveyance, Box<dyn Error>> {
    let mut claims = Claims::new(UPSTREAM, "urn:example:ferrofed-under-test");
    subject.clone_into(&mut claims.sub);
    let token = CALLERS.mint(&claims)?;
    let mut verified = caller();
    verified.verified_by = Verification::Signature;
    Ok(Conveyance::new(shared(), Principal::Caller(verified))
        .with_subject(SubjectToken::new(SecretString::from(token), "user/aql-*.s")))
}

/// The metadata is discovered from the issuer, the token is issued for an
/// ES256 assertion naming the issuer, and it reaches the node under the
/// `DPoP` scheme with a proof of the grant's key (FAPI 2.0 §5.3.2.1,
/// §5.3.3.1; RFC 9449 §7.1).
// conformance: CP-17
#[tokio::test]
async fn a_dpop_bound_token_reaches_the_node() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(vec![Verdict::Issued], server.endpoint().verdicts());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    let sent = request
        .headers
        .get(http::header::AUTHORIZATION)
        .ok_or("a credential")?
        .to_str()?;
    let token = sent.strip_prefix("DPoP ").ok_or("the DPoP scheme")?;
    let proof = request.headers.get(HEADER).ok_or("a proof")?.to_str()?;
    let verified = dpop::verify(
        proof,
        "POST",
        &format!("{}/openehr/v1/query/aql", node.uri()),
        Some(token),
    )?;
    assert_eq!(prover.thumbprint(), verified.jkt);
    Ok(())
}

/// The token request is the client-credentials grant authenticated by a
/// `private_key_jwt` assertion signed ES256 whose `aud` is the issuer as one
/// string, with the scope and the `authorization_details` as configured, and
/// no client secret (FAPI 2.0 §5.3.3.1, §5.4.1; RFC 9396 §6).
// conformance: CP-17
#[tokio::test]
async fn the_token_request_names_the_issuer_and_carries_the_details() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let prover = prover()?;
    provider(grant(&server, key, &prover)?)?
        .credentials()
        .await?;

    let forms = server.endpoint().forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    assert_eq!(Some(GRANT_TYPE), field(form, "grant_type"));
    assert_eq!(
        Some(CLIENT_ASSERTION_TYPE),
        field(form, "client_assertion_type")
    );
    assert_eq!(Some(SCOPE), field(form, "scope"));
    assert_eq!(Some(DETAILS), field(form, "authorization_details"));
    assert_eq!(None, field(form, "client_secret"));
    let assertion = field(form, "client_assertion").ok_or("an assertion")?;
    let header = jsonwebtoken::decode_header(assertion)?;
    assert_eq!(Algorithm::ES256, header.alg);
    let payload = assertion.split('.').nth(1).ok_or("a JWS")?;
    let claims: AssertionAudience = serde_json::from_slice(&base64::Engine::decode(
        &base64::engine::general_purpose::URL_SAFE_NO_PAD,
        payload,
    )?)?;
    assert_eq!(server.issuer(), claims.aud);
    Ok(())
}

/// A token response that states no `authorization_details` fails the node,
/// and the node is sent nothing (RFC 9396 §7).
// conformance: CP-17
#[tokio::test]
async fn a_token_without_its_granted_details_fails_the_node() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server.endpoint().omit_authorization_details();
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert_eq!(UNAUTHENTICATED, error_text(&reply)?);
    assert!(
        node.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty()
    );
    Ok(())
}

/// Details the server refuses fail the node with the code RFC 9396 §6
/// registers, and the node is sent nothing.
// conformance: CP-17
#[tokio::test]
async fn refused_details_fail_the_node_naming_the_code() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server
        .endpoint()
        .require_authorization_details(["another-type"]);
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert_eq!(
        format!("{UNAUTHENTICATED}: the token endpoint refused with invalid_authorization_details"),
        error_text(&reply)?
    );
    assert!(
        node.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty()
    );
    Ok(())
}

/// A refused grant fails the node with its registered code, and the answer
/// carries no assertion, token or key (§13.1, N25).
// conformance: CP-17
#[tokio::test]
async fn a_refused_grant_fails_the_node_and_shows_no_credential() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server
        .endpoint()
        .refuse(401, "invalid_client", "synthetic refusal");
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    let text = error_text(&reply)?;
    assert_eq!(
        format!("{UNAUTHENTICATED}: the token endpoint refused with invalid_client"),
        text
    );
    for hidden in [CLIENT_ID, "127.0.0.1", "eyJ", prover.thumbprint()] {
        assert!(!text.contains(hidden), "{hidden} is not shown: {text}");
    }
    Ok(())
}

/// A token endpoint that answers after the timeout fails the node, and the
/// node is sent nothing (§11.5, N38).
// conformance: CP-17
#[tokio::test]
async fn a_token_endpoint_past_the_timeout_fails_the_node() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server.endpoint().delay(Duration::from_secs(8));
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_ne!(EndpointStatus::Active, reply.status());
    assert!(
        node.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty()
    );
    Ok(())
}

/// The metadata is read once and kept: a second query asks the token
/// endpoint alone, or nothing when the token is cached.
#[tokio::test]
async fn the_metadata_is_read_once() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    for _ in 0..2 {
        let reply = query(&client, conveyance()).await?;
        assert_eq!(EndpointStatus::Active, reply.status());
    }
    let requests = server
        .endpoint()
        .server()
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let reads = requests
        .iter()
        .filter(|request| request.url.path() == METADATA_PATH)
        .count();
    assert_eq!(1, reads);
    Ok(())
}

/// A discovery that fails is not kept: once the server publishes metadata
/// the grant admits, the next query discovers it and succeeds.
#[tokio::test]
async fn a_failed_discovery_is_tried_again() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let mut wrong = server.metadata();
    wrong.issuer = Some(format!("{}/other", server.issuer()));
    server.publish(&wrong);
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let first = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, first.status());
    assert!(server.endpoint().forms().is_empty(), "no token request");
    server.publish(&server.metadata());
    let second = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::Active, second.status());
    Ok(())
}

/// Token exchange: each verified caller's token is exchanged with an ES256
/// client assertion and an actor token both naming the issuer, carrying the
/// details, and the exchanged token reaches the node `DPoP`-bound (RFC 8693
/// §2.1; FAPI 2.0 §5.3.3.1).
// conformance: CP-17
#[tokio::test]
async fn a_verified_callers_token_is_exchanged() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server.endpoint().accept_exchange(CALLERS.jwks(), UPSTREAM);
    server.endpoint().expect_resource(RESOURCE);
    let node = node(&server).await;
    let prover = prover()?;
    let grant = grant(&server, key, &prover)?
        .with_resource(RESOURCE)?
        .with_token_exchange()?;
    let client = client(&node, &prover)?.with_on_behalf(exchange(grant)?);

    let reply = query(&client, verified_caller("clinician-0042")?).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let answered = server.endpoint().exchanges();
    let [exchanged] = answered.as_slice() else {
        return Err(format!("expected one exchange, got {}", answered.len()).into());
    };
    assert_eq!("clinician-0042", exchanged.subject);
    assert_eq!(Some(RESOURCE), exchanged.resource.as_deref());
    let forms = server.endpoint().forms();
    let form = forms.first().ok_or("a token request")?;
    assert_eq!(Some(DETAILS), field(form, "authorization_details"));
    Ok(())
}

/// No patient identifier reaches the authorization server or the node: a
/// caller whose token carries the withheld identifier is not exchanged, and
/// nothing is sent to either (§5.4.1, N33, CP-26).
// conformance: CP-17 CP-26 track-10
#[tokio::test]
async fn a_callers_token_carrying_the_patient_is_never_sent() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    server.endpoint().accept_exchange(CALLERS.jwks(), UPSTREAM);
    let node = node(&server).await;
    let prover = prover()?;
    let grant = grant(&server, key, &prover)?
        .with_resource(RESOURCE)?
        .with_token_exchange()?;
    let client = client(&node, &prover)?.with_on_behalf(exchange(grant)?);

    let reply = query(&client, verified_caller(PATIENT)?).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert!(server.endpoint().exchanges().is_empty());
    for form in server.endpoint().forms() {
        for (_, value) in form {
            assert!(!value.contains(PATIENT), "the patient reached the server");
        }
    }
    assert!(
        node.received_requests()
            .await
            .ok_or("recording is on")?
            .is_empty()
    );
    Ok(())
}

/// The token request and the node request carry nothing of the patient the
/// query was resolved on, in a form value or a header (§5.4.1, N33).
// conformance: CP-26 track-10
#[tokio::test]
async fn no_carrier_to_the_server_or_the_node_names_the_patient() -> TestResult {
    let key = client_key()?;
    let server = server(&key).await;
    let node = node(&server).await;
    let prover = prover()?;
    let client =
        client(&node, &prover)?.with_credentials_provider(provider(grant(&server, key, &prover)?)?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::Active, reply.status());
    for form in server.endpoint().forms() {
        for (_, value) in form {
            assert!(!value.contains(PATIENT));
        }
    }
    let requests = node.received_requests().await.ok_or("recording is on")?;
    for request in requests {
        for (name, value) in &request.headers {
            let value = value.to_str().unwrap_or_default();
            assert!(!value.contains(PATIENT), "{name} names the patient");
        }
    }
    Ok(())
}
