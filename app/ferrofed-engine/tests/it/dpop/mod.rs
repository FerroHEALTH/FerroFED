// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `DPoP`-bound onward tokens against the harness token endpoint and a mock
//! node: every request to the token endpoint and to the node carries a proof
//! of the grant's key binding its method and URL, the node's request carries
//! the bound token under the `DPoP` scheme and its proof binds the token's
//! hash, and a nonce either server demands is answered once (§13.1, N25,
//! CP-17; RFC 9449 §4, §5, §7.1, §8, §9).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::{Principal, Verification};
use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, NodeQuery, NodeReply};
use ferrofed_engine::onward::grant::lane::{Lane, Organisations};
use ferrofed_engine::onward::upstream::UpstreamToken;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::dpop::{self, HEADER};
use ferrofed_testkit::issuance::Verdict;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use ferrohealth_smart::client::assertion::AssertionKey;
use ferrohealth_smart::client::dpop::{DpopKey, DpopKeyError};
use ferrohealth_smart::client::keys::KeyRing;
use ferrohealth_smart::client::{ClientCredentials, Endpoint};
use ferrohealth_smart::kind::IssuerKind;
use jsonwebtoken::Algorithm;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Credentials, CredentialsProvider as _, ReqwestTransport};
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, conveyance, shared};
use crate::exchange;

mod nonce;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope every client-credentials token is requested with.
const SCOPE: &str = "system/aql-*.s";

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// The gateway's signing keys.
fn keys() -> Result<Arc<KeyRing>, Box<dyn Error>> {
    let key = AssertionKey::from_pem(
        oauth::Curve::P384.pem()?.as_bytes(),
        Some(Algorithm::ES384),
        None,
    )?;
    Ok(Arc::new(KeyRing::new(key)))
}

/// A fresh `DPoP` key of the P-256 curve.
fn prover() -> Result<Arc<DpopKey>, Box<dyn Error>> {
    Ok(Arc::new(DpopKey::from_pem(
        oauth::Curve::P256.pem()?.as_bytes(),
    )?))
}

/// A token endpoint requiring `DPoP` and trusting `keys`.
async fn endpoint(keys: &KeyRing) -> Result<TokenEndpoint, Box<dyn Error>> {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.trust(serde_json::from_value(keys.published())?);
    endpoint.require_dpop();
    Ok(endpoint)
}

/// The client-credentials grant at `url` over `keys`, binding nothing.
fn plain_at(url: &str, keys: Arc<KeyRing>) -> Result<ClientCredentials, Box<dyn Error>> {
    let mut grant =
        ClientCredentials::with_key_ring(Endpoint::Token(Url::parse(url)?), CLIENT_ID.into(), keys);
    grant.scope = Some(SCOPE.to_owned());
    grant.timeout = Duration::from_secs(5);
    Ok(grant)
}

/// The client-credentials grant at `url` over `keys`, bound to `prover`'s
/// key.
fn grant_at(
    url: &str,
    keys: Arc<KeyRing>,
    prover: &Arc<DpopKey>,
) -> Result<ClientCredentials, Box<dyn Error>> {
    Ok(plain_at(url, keys)?.with_dpop(Arc::clone(prover)))
}

/// The client-credentials grant at `endpoint` over `keys`, bound to
/// `prover`'s key.
fn grant(
    endpoint: &TokenEndpoint,
    keys: Arc<KeyRing>,
    prover: &Arc<DpopKey>,
) -> Result<ClientCredentials, Box<dyn Error>> {
    grant_at(&endpoint.token_url(), keys, prover)
}

/// The engine every node request and token request is sent through.
fn transport() -> Result<ReqwestTransport, Box<dyn Error>> {
    Ok(ReqwestTransport::with_timeout(Duration::from_secs(10))?)
}

/// The client of the endpoint whose base is `{server}/{base}`, over
/// `transport`.
fn client(
    server: &Server,
    base: &str,
    transport: ReqwestTransport,
) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/{base}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
        server.uri()
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(endpoint, transport)?)
}

/// The client-credentials provider of `grant`.
fn provider(grant: ClientCredentials) -> Result<Arc<UpstreamToken>, Box<dyn Error>> {
    Ok(UpstreamToken::provider(grant)?)
}

/// Mounts the query answer on `server` for a request `matcher` admits, and
/// `fallback` for every other.
async fn answer_when(
    server: &Server,
    matcher: impl wiremock::Match + 'static,
    fallback: ResponseTemplate,
) {
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .and(matcher)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .respond_with(fallback)
        .with_priority(2)
        .mount(server)
        .await;
}

/// Sends the node query through `client` conveying `conveyance`.
async fn query<T: openehr_its::rest::client::Transport + Clone>(
    client: &NodeClient<T>,
    conveyance: Conveyance,
) -> Result<NodeReply, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, conveyance);
    Ok(client.query(&NodeQuery::new(NODE_AQL), &options).await?)
}

/// The `Authorization` value and the `DPoP` proof of `request`.
fn credential_and_proof(request: &wiremock::Request) -> Result<(&str, &str), Box<dyn Error>> {
    let sent = request
        .headers
        .get(http::header::AUTHORIZATION)
        .ok_or("the node receives a credential")?
        .to_str()?;
    let proof = request
        .headers
        .get(HEADER)
        .ok_or("the node receives a proof")?
        .to_str()?;
    Ok((sent, proof))
}

/// The node receives the bound token under the `DPoP` scheme, never under
/// `Bearer`, with a proof of the grant's key over its method, URL and the
/// token's hash, and the token endpoint received a proof of the same key
/// (RFC 9449 §4.2, §5, §7.1).
// conformance: CP-17
#[tokio::test]
async fn a_dpop_bound_token_reaches_the_node_with_a_proof_of_its_key() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await?;
    let node = Server::start().await;
    answer_when(&node, endpoint.dpop_bound(None), ResponseTemplate::new(401)).await;
    let prover = prover()?;
    let provider = provider(grant(&endpoint, keys, &prover)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(vec![Verdict::Issued], endpoint.issuance().verdicts());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    let (sent, proof) = credential_and_proof(request)?;
    assert!(
        !sent.to_ascii_lowercase().starts_with("bearer"),
        "a DPoP-bound token never goes out under Bearer: {sent}"
    );
    let token = sent.strip_prefix("DPoP ").ok_or("the DPoP scheme")?;
    let verified = dpop::verify(
        proof,
        "POST",
        &format!("{}/openehr/v1/query/aql", node.uri()),
        Some(token),
    )?;
    assert_eq!(prover.thumbprint(), verified.jkt);
    assert_eq!(None, verified.proof.nonce, "no nonce was demanded");
    Ok(())
}

/// A provider of a `DPoP`-bound grant hands its node client a `DPoP`
/// credential, which is written under the `DPoP` scheme and never under
/// `Bearer` (RFC 9449 §7.1).
#[tokio::test]
async fn a_dpop_bound_token_is_never_a_bearer_credential() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await?;
    let prover = prover()?;
    let provider = provider(grant(&endpoint, keys, &prover)?)?;

    let credentials = provider.credentials().await?;
    assert!(
        matches!(credentials, Credentials::Dpop(_)),
        "{credentials:?}"
    );
    let value = credentials.header_value()?;
    let written = value.to_str()?;
    assert!(written.starts_with("DPoP "), "the DPoP scheme");
    Ok(())
}

/// A token endpoint that demands a nonce is answered once more with it, and
/// the token it then issues reaches the node (RFC 9449 §8).
// conformance: CP-17
#[tokio::test]
async fn the_token_endpoints_nonce_is_answered_once() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await?;
    endpoint.issuance().require_nonce("synthetic-as-nonce-1");
    let node = Server::start().await;
    answer_when(&node, endpoint.dpop_bound(None), ResponseTemplate::new(401)).await;
    let prover = prover()?;
    let provider = provider(grant(&endpoint, keys, &prover)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(
        vec![
            Verdict::Refused(String::from("use_dpop_nonce")),
            Verdict::Issued
        ],
        endpoint.issuance().verdicts()
    );
    Ok(())
}

/// A token endpoint that keeps demanding a nonce is answered once more,
/// with a proof naming the nonce, and no further: its refusal fails the
/// node, with nothing sent to the node (RFC 9449 §8).
// conformance: CP-17
#[tokio::test]
async fn the_token_endpoints_nonce_is_answered_no_more_than_once() -> TestResult {
    let keys = keys()?;
    let authority = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_raw(
                    br#"{"error":"use_dpop_nonce"}"#.to_vec(),
                    "application/json",
                )
                .insert_header(dpop::NONCE_HEADER, "synthetic-as-nonce-2"),
        )
        .mount(&authority)
        .await;
    let token_url = format!("{}/token", authority.uri());
    let node = Server::start().await;
    let prover = prover()?;
    let provider = provider(grant_at(&token_url, keys, &prover)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    let sent = authority
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let [first, second] = sent.as_slice() else {
        return Err(format!("expected one request and one more, got {}", sent.len()).into());
    };
    let mut nonces = Vec::new();
    for request in [first, second] {
        let proof = request
            .headers
            .get(HEADER)
            .ok_or("every token request carries a proof")?
            .to_str()?;
        nonces.push(dpop::verify(proof, "POST", &token_url, None)?.proof.nonce);
    }
    assert_eq!(
        vec![None, Some(String::from("synthetic-as-nonce-2"))],
        nonces
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// A node that demands a nonce is answered once more with it: the second
/// request carries a new proof, with its own `jti`, naming the nonce (RFC
/// 9449 §4.2, §9).
// conformance: CP-17
#[tokio::test]
async fn the_nodes_nonce_is_answered_once() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await?;
    let node = Server::start().await;
    answer_when(
        &node,
        endpoint.dpop_bound(Some("synthetic-rs-nonce-1")),
        dpop::challenge("synthetic-rs-nonce-1"),
    )
    .await;
    let prover = prover()?;
    let provider = provider(grant(&endpoint, keys, &prover)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [first, second] = requests.as_slice() else {
        return Err(format!(
            "expected one request and one more with the nonce, got {}",
            requests.len()
        )
        .into());
    };
    let url = format!("{}/openehr/v1/query/aql", node.uri());
    let mut proofs = Vec::new();
    for request in [first, second] {
        let (sent, proof) = credential_and_proof(request)?;
        let token = sent.strip_prefix("DPoP ").ok_or("the DPoP scheme")?;
        proofs.push(dpop::verify(proof, "POST", &url, Some(token))?.proof);
    }
    let [unprompted, prompted] = proofs.as_slice() else {
        return Err("two proofs".into());
    };
    assert_eq!(None, unprompted.nonce);
    assert_eq!(Some("synthetic-rs-nonce-1"), prompted.nonce.as_deref());
    assert_ne!(unprompted.jti, prompted.jti, "each proof has its own jti");
    assert_eq!(
        1,
        endpoint.issuance().issued(),
        "the challenge drops no token"
    );
    Ok(())
}

/// A token exchanged for a verified caller through the node's onward lane is
/// bound with `DPoP` like any other, and reaches the node under the `DPoP`
/// scheme (RFC 8693, RFC 9449 §7.1).
// conformance: CP-17
#[tokio::test]
async fn an_exchanged_token_is_dpop_bound() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await?;
    endpoint.accept_exchange(exchange::CALLERS.jwks(), UPSTREAM);
    endpoint.issue_onward_tokens(exchange::ONWARD_ISSUER);
    let node = Server::start().await;
    answer_when(&node, endpoint.dpop_bound(None), ResponseTemplate::new(401)).await;
    let prover = prover()?;
    let client = client(&node, "openehr", transport()?)?
        .with_on_behalf(dpop_lane(&endpoint, keys, &prover)?)
        .with_dpop(&prover);

    let reply = query(&client, exchanged_call("clinician-0042")?).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(1, endpoint.exchanges().len());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    let (sent, _proof) = credential_and_proof(request)?;
    assert!(sent.starts_with("DPoP "), "the DPoP scheme: {sent}");
    Ok(())
}

/// The onward lane at `endpoint` whose grant over `keys` binds its tokens to
/// `prover`'s key and names the gateway as the actor.
fn dpop_lane(
    endpoint: &TokenEndpoint,
    keys: Arc<KeyRing>,
    prover: &Arc<DpopKey>,
) -> Result<Arc<Lane>, Box<dyn Error>> {
    let grant = grant(endpoint, keys, prover)?;
    let gateway = UpstreamToken::provider(grant.clone())?;
    let resolved = exchange::section(endpoint, serde_json::json!({}))?.resolve(grant)?;
    Ok(Arc::new(Lane::new(
        EndpointId::new("node-a-pub")?,
        (resolved.onward, resolved.target),
        (IssuerKind::Persons, Organisations::Refused),
        gateway,
    )))
}

/// The call of the verified person `subject`, its token signed by the
/// callers' issuer.
fn exchanged_call(subject: &str) -> Result<Conveyance, Box<dyn Error>> {
    let token = exchange::caller_token(subject)?;
    let mut verified = caller();
    verified.verified_by = Verification::Signature;
    Ok(
        Conveyance::new(shared(), Principal::Caller(verified)).with_subject(exchange::subject_of(
            &token,
            subject,
            "user/aql-*.s",
        )?),
    )
}

/// A token endpoint that answers a bearer token to a grant that asked for a
/// `DPoP`-bound one fails the node, with nothing sent to it (RFC 9449 §5).
// conformance: CP-17
#[tokio::test]
async fn a_bearer_token_where_dpop_was_asked_fails_the_node() -> TestResult {
    let keys = keys()?;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.trust(serde_json::from_value(keys.published())?);
    let node = Server::start().await;
    answer_when(&node, endpoint.bearer(), ResponseTemplate::new(401)).await;
    let prover = prover()?;
    let provider = provider(grant(&endpoint, keys, &prover)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// A request without a `DPoP`-bound token carries no proof, even from a
/// client given a key: a grant that binds nothing sends its token under the
/// `Bearer` scheme.
#[tokio::test]
async fn a_request_without_a_bound_token_carries_no_proof() -> TestResult {
    let keys = keys()?;
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.trust(serde_json::from_value(keys.published())?);
    let node = Server::start().await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .and(endpoint.bearer())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
        )
        .mount(&node)
        .await;
    let provider = provider(plain_at(&endpoint.token_url(), keys)?)?;
    let client = client(&node, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover()?);

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    assert!(request.headers.get(HEADER).is_none(), "no proof");
    Ok(())
}

/// A P-256 key proves with ES256 and a P-384 key with ES384; text that is
/// no EC key in PKCS#8 PEM is refused naming no part of it (RFC 7518 §3.4).
#[test]
fn a_dpop_key_signs_with_the_algorithm_of_its_curve() -> TestResult {
    let p256 = DpopKey::from_pem(oauth::Curve::P256.pem()?.as_bytes())?;
    assert_eq!(Algorithm::ES256, p256.algorithm());
    let p384 = DpopKey::from_pem(oauth::Curve::P384.pem()?.as_bytes())?;
    assert_eq!(Algorithm::ES384, p384.algorithm());
    let refused = DpopKey::from_pem(b"synthetic: no key");
    assert!(
        matches!(refused, Err(DpopKeyError::Key { .. })),
        "{refused:?}"
    );
    assert!(
        !format!("{p256:?}").contains("PRIVATE"),
        "Debug shows the thumbprint alone"
    );
    Ok(())
}
