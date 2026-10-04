// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Token exchange against the harness token endpoint and a mock node: each
//! verified caller's token is exchanged for a token of the node, the gateway
//! as the actor, the scope the caller's scope that covers the operation, and
//! the node named as the resource; the conveyance header still travels, and
//! a caller with no verified token of its own is refused with nothing sent
//! (§13.1, N24, N25, N26, CP-16, CP-17; RFC 8693 §2, RFC 8707 §2, RFC 7523
//! §2.2).
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
use ferrofed_engine::onward::conveyance::{self, Conveyance, Principal, Verification};
use ferrofed_engine::onward::exchange::{Exchange, SubjectToken};
use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_engine::onward::token::{ACCESS_TOKEN_TYPE, JWT_TOKEN_TYPE, TOKEN_EXCHANGE};
use ferrofed_engine::onward::{Grant, Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, Exchanged, TokenEndpoint, Verdict};
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::ReqwestTransport;
use secrecy::SecretString;
use wiremock::matchers::{header_exists, method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, shared};

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope of the gateway's own client-credentials requests.
const SCOPE: &str = "system/aql-*.s";

/// The node a token is exchanged for (RFC 8707 §2).
const RESOURCE: &str = "https://cdr-a.example.org/openehr";

/// The audience the caller's token names: the gateway.
const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// The caller's scopes that cover a query.
const COVERING: &str = "user/aql-*.s";

/// A synthetic subject the gateway resolved on and withholds.
const WITHHELD: &str = "SYNTHETIC-SUBJECT-4f1a";

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// The issuer of the callers' tokens, its key generated once per process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static CALLERS: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(UPSTREAM).expect("a test key pair should generate"));

/// A token of the caller `subject`, as its issuer signed it.
fn caller_token(subject: &str) -> Result<String, Box<dyn Error>> {
    let mut claims = Claims::new(UPSTREAM, AUDIENCE);
    subject.clone_into(&mut claims.sub);
    Ok(CALLERS.mint(&claims)?)
}

/// The gateway's keys, the ones the token endpoint trusts.
fn keys() -> Result<Arc<KeyRing>, Box<dyn Error>> {
    let key = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    Ok(Arc::new(KeyRing::new(
        key,
        None,
        Duration::ZERO,
        Arc::new(SystemClock),
    )?))
}

/// The token endpoint, exchanging the callers' tokens and trusting `keys`.
async fn endpoint(keys: &KeyRing) -> TokenEndpoint {
    let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
    endpoint.trust(keys.published());
    endpoint.accept_exchange(CALLERS.jwks(), UPSTREAM);
    endpoint.expect_resource(RESOURCE);
    endpoint
}

/// The token-exchange grant at `endpoint`.
fn grant(endpoint: &TokenEndpoint) -> Result<Grant, Box<dyn Error>> {
    Ok(Grant::new(
        &SecretUrl::new(endpoint.token_url()),
        CLIENT_ID,
        Scope::parse(SCOPE)?,
    )?
    .with_resource(RESOURCE)?
    .with_token_exchange())
}

/// The exchange of `grant`, signed with `keys`.
fn exchange(
    grant: Grant,
    keys: Arc<KeyRing>,
) -> Result<Arc<Exchange<ReqwestTransport>>, Box<dyn Error>> {
    Ok(Arc::new(Exchange::new(
        EndpointId::new("node-a-pub")?,
        grant,
        keys,
        (Duration::from_secs(300), Duration::from_secs(5)),
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
        Arc::new(SystemClock),
    )))
}

/// A node answering a request that carries the conveyance header and a token
/// `endpoint` issued, for `subject` when one is named, and `401` to every
/// other.
async fn node(endpoint: &TokenEndpoint, subject: Option<&str>) -> Server {
    let server = Server::start().await;
    let required = match subject {
        Some(subject) => endpoint.bearer_for(subject),
        None => endpoint.bearer(),
    };
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .and(header_exists(conveyance::HEADER))
        .and(required)
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(&server)
        .await;
    server
}

/// The client of the node at `server`, exchanging with `exchange`.
fn client(
    server: &Server,
    exchange: Arc<Exchange<ReqwestTransport>>,
) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
        server.uri()
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?
    .with_on_behalf(exchange))
}

/// The conveyance of the synthetic caller, verified as `verified_by`,
/// carrying `token` when one is given.
fn conveyance_of(token: Option<&str>, verified_by: Verification) -> Conveyance {
    let mut caller = caller();
    caller.verified_by = verified_by;
    let conveyance = Conveyance::new(shared(), Principal::Caller(caller));
    match token {
        Some(token) => conveyance.with_subject(SubjectToken::new(
            SecretString::from(token.to_owned()),
            COVERING,
        )),
        None => conveyance,
    }
}

/// Sends the node query through `client` conveying `conveyance`, withholding
/// [`WITHHELD`].
async fn query(
    client: &NodeClient<ReqwestTransport>,
    conveyance: Conveyance,
) -> Result<NodeReply, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, conveyance)
        .with_withheld(Arc::new(Withheld::new([SecretString::from(WITHHELD)])));
    Ok(client.query(&NodeQuery::new(NODE_AQL), &options).await?)
}

/// The value of form parameter `name`.
fn field<'f>(form: &'f [(String, String)], name: &str) -> Option<&'f str> {
    form.iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The text of a reply's `error`.
fn error_text(reply: &NodeReply) -> Result<String, Box<dyn Error>> {
    match reply.outcome().error() {
        Some(ErrorDetail::Text(text)) => Ok(text.clone()),
        other => Err(format!("an unexpected error: {other:?}").into()),
    }
}

/// The node receives a token exchanged for the verified caller: the caller's
/// token is the subject, a separate assertion of the gateway the actor, the
/// scope the caller's scope that covers the query, the node the resource,
/// and the conveyance header still travels (RFC 8693 §2.1, RFC 8707 §2, N24,
/// N25, N26).
// conformance: CP-16 CP-17
#[tokio::test]
async fn the_node_receives_a_token_exchanged_for_the_verified_caller() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, Some(crate::conveyed::SUBJECT)).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token(crate::conveyed::SUBJECT)?;

    let reply = query(
        &client,
        conveyance_of(Some(&token), Verification::Signature),
    )
    .await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(
        vec![Exchanged {
            subject: crate::conveyed::SUBJECT.to_owned(),
            scope: Some(COVERING.to_owned()),
            resource: Some(RESOURCE.to_owned()),
        }],
        endpoint.exchanges()
    );
    let forms = endpoint.forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    assert_eq!(Some(TOKEN_EXCHANGE), field(form, "grant_type"));
    assert_eq!(Some(token.as_str()), field(form, "subject_token"));
    assert_eq!(Some(ACCESS_TOKEN_TYPE), field(form, "subject_token_type"));
    assert_eq!(Some(JWT_TOKEN_TYPE), field(form, "actor_token_type"));
    assert_ne!(
        field(form, "actor_token"),
        field(form, "client_assertion"),
        "the actor token is an assertion of its own, with its own jti"
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    let [request] = requests.as_slice() else {
        return Err(format!("expected one node request, got {}", requests.len()).into());
    };
    let sent = request
        .headers
        .get(http::header::AUTHORIZATION)
        .ok_or("the node receives a credential")?
        .to_str()?;
    assert!(
        !sent.contains(&token),
        "the caller's token reaches the token endpoint alone, never the node"
    );
    Ok(())
}

/// A token is exchanged once per caller and scope while it is fresh, and a
/// second caller gets a token of its own.
// conformance: CP-17
#[tokio::test]
async fn an_exchanged_token_is_cached_per_caller() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let exchange = exchange(grant(&endpoint)?, keys)?;
    let client = client(&node, Arc::clone(&exchange))?;
    let first = caller_token("clinician-0042")?;
    let second = caller_token("clinician-0043")?;

    for token in [&first, &first, &second] {
        let reply = query(&client, conveyance_of(Some(token), Verification::Signature)).await?;
        assert_eq!(
            EndpointStatus::Active,
            reply.status(),
            "{:?}",
            reply.outcome()
        );
    }
    assert_eq!(2, endpoint.issued(), "one exchange per caller");
    assert_eq!(2, exchange.cached());
    Ok(())
}

/// A node that answers `401` drops the exchanged token, and the caller's
/// next request exchanges anew.
// conformance: CP-17
#[tokio::test]
async fn a_node_401_drops_the_exchanged_token() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token("clinician-0042")?;
    let conveyed = || conveyance_of(Some(&token), Verification::Signature);

    assert_eq!(
        EndpointStatus::Active,
        query(&client, conveyed()).await?.status()
    );
    endpoint.revoke_all();
    assert_eq!(
        EndpointStatus::NodeError,
        query(&client, conveyed()).await?.status()
    );
    assert_eq!(
        EndpointStatus::Active,
        query(&client, conveyed()).await?.status()
    );
    assert_eq!(
        2,
        endpoint.issued(),
        "the refused token was exchanged again"
    );
    Ok(())
}

/// A caller the edge asserted has no verified token of its own, so nothing
/// is exchanged and nothing reaches the node.
// conformance: CP-17
#[tokio::test]
async fn an_edge_asserted_caller_is_refused_with_nothing_sent() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token("clinician-0042")?;

    let reply = query(&client, conveyance_of(Some(&token), Verification::Edge)).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert_eq!(UNAUTHENTICATED, error_text(&reply)?);
    assert!(endpoint.forms().is_empty(), "no token request was sent");
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// A verified caller whose request carries no token of it is refused with
/// nothing sent, never sent on the gateway's own grant.
// conformance: CP-17
#[tokio::test]
async fn a_caller_without_its_token_is_refused_with_nothing_sent() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;

    let reply = query(&client, conveyance_of(None, Verification::Signature)).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert!(endpoint.forms().is_empty(), "no token request was sent");
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// A caller's token whose claims carry the identifier the request withholds
/// never reaches the token endpoint, and the node is sent nothing (§5.4.1,
/// N33).
// conformance: CP-26
#[tokio::test]
async fn a_caller_token_carrying_the_withheld_identifier_is_never_exchanged() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token(WITHHELD)?;

    let reply = query(
        &client,
        conveyance_of(Some(&token), Verification::Signature),
    )
    .await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert!(
        endpoint.forms().is_empty(),
        "the token endpoint was sent nothing"
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// The gateway's own request, which has no caller, uses the
/// client-credentials grant at the same token endpoint.
// conformance: CP-17
#[tokio::test]
async fn the_gateways_own_request_uses_client_credentials() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;

    let reply = query(&client, Conveyance::new(shared(), Principal::Gateway)).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let forms = endpoint.forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    assert_eq!(Some("client_credentials"), field(form, "grant_type"));
    assert_eq!(Some(SCOPE), field(form, "scope"));
    assert_eq!(None, field(form, "subject_token"));
    Ok(())
}

/// A refused exchange fails the node with the token endpoint's registered
/// code, and nothing is sent to the node (RFC 6749 §5.2).
// conformance: CP-17
#[tokio::test]
async fn a_refused_exchange_names_the_token_endpoints_code() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    endpoint.refuse(400, "invalid_grant", "synthetic: the subject is unknown");
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token("clinician-0042")?;

    let reply = query(
        &client,
        conveyance_of(Some(&token), Verification::Signature),
    )
    .await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert_eq!(
        format!("{UNAUTHENTICATED}: the token endpoint refused with invalid_grant"),
        error_text(&reply)?
    );
    assert_eq!(
        vec![Verdict::Refused(String::from("invalid_grant"))],
        endpoint.verdicts()
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// An exchange answer that names no `issued_token_type` is no token the
/// gateway sends (RFC 8693 §2.2.1).
// conformance: CP-17
#[tokio::test]
async fn an_exchange_answer_without_its_issued_token_type_fails_the_node() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    endpoint.omit_issued_token_type();
    let node = node(&endpoint, None).await;
    let client = client(&node, exchange(grant(&endpoint)?, keys)?)?;
    let token = caller_token("clinician-0042")?;

    let reply = query(
        &client,
        conveyance_of(Some(&token), Verification::Signature),
    )
    .await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}
