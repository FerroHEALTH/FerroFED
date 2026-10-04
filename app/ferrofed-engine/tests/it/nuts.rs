// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The Nuts grant against the harness Nuts node and a mock node: the token
//! obtained with a Verifiable Presentation of the gateway's credentials
//! reaches the node `DPoP`-bound, proven with the grant's key; a refused or
//! slow grant fails the node with nothing sent to it; and no failure names a
//! credential (§13.1, §13.3, N25, CP-17; Annex B §B.4; Nuts RFC021; RFC 9449
//! §5, §7.1). Every identifier, key and credential is synthetic.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, NodeQuery, NodeReply};
use ferrofed_engine::onward::SystemClock;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::nuts::{NutsCredentials, NutsGrant};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::dpop::{self, HEADER};
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::nuts::{self, NutsNode, Verdict};
use ferrofed_testkit::oauth;
use jsonwebtoken::Algorithm;
use nl_generic_functions::nuts_auth::holder::{Did, Holder, HolderKey};
use nl_generic_functions::nuts_auth::{Grant, NutsClient};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::ReqwestTransport;
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::conveyance;

type TestResult = Result<(), Box<dyn Error>>;

const HOLDER: &str = "did:web:gateway.example.org";
const HOLDER_KID: &str = "did:web:gateway.example.org#key-1";
const ISSUER: &str = "did:web:issuer.example.org";
const ISSUER_KID: &str = "did:web:issuer.example.org#key-1";
const SCOPE: &str = "openehr-query";
const DEFINITION: &str =
    r#"{"id": "pd_synthetic", "input_descriptors": [{"id": "organization_credential"}]}"#;
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A harness Nuts node, the grant at it, and the synthetic credential it
/// trusts.
struct Setup {
    authority: NutsNode,
    grant: NutsGrant,
    prover: Arc<Prover>,
    credential: String,
}

async fn setup() -> Result<Setup, Box<dyn Error>> {
    let authority = NutsNode::start("hospital-a", SCOPE, Some(300)).await;
    authority.define(DEFINITION);
    let holder_pem = oauth::p256_pem()?;
    let issuer_pem = oauth::p256_pem()?;
    authority.trust_holder(
        HOLDER,
        HOLDER_KID,
        nuts::public_jwk(&holder_pem, Algorithm::ES256)?,
    );
    authority.trust_issuer(
        ISSUER,
        ISSUER_KID,
        nuts::public_jwk(&issuer_pem, Algorithm::ES256)?,
    );
    let credential = nuts::credential(
        (&issuer_pem, ISSUER_KID),
        ISSUER,
        HOLDER,
        (
            "SyntheticOrganizationCredential",
            "Synthetic Care Organisation",
        ),
        jiff::Timestamp::now().as_second() + 3600,
    )?;
    let did = Did::new(HOLDER)?;
    let key = HolderKey::from_pem(&SecretString::from(holder_pem), HOLDER_KID, &did)?;
    let holder = Holder::new(
        did,
        key,
        vec![(
            String::from("organization_credential"),
            SecretString::from(credential.clone()),
        )],
    )?;
    let prover = Arc::new(Prover::from_pem(&SecretString::from(oauth::p256_pem()?))?);
    let grant = NutsGrant::new(
        Grant::new(&authority.issuer(), SCOPE)?,
        Arc::new(holder),
        Arc::clone(&prover),
    );
    Ok(Setup {
        authority,
        grant,
        prover,
        credential,
    })
}

/// The provider of `grant`, waiting at most `timeout` for a token.
fn provider(grant: NutsGrant, timeout: Duration) -> Result<Arc<NutsCredentials>, Box<dyn Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    Ok(Arc::new(NutsCredentials::new(
        EndpointId::new("node-a-pub")?,
        grant,
        NutsClient::new(http),
        timeout,
        Arc::new(SystemClock),
    )))
}

/// The client of the endpoint at `{server}/openehr`.
fn client(server: &Server) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
        server.uri()
    );
    let snapshot = RegistrySnapshot::from_toml_str(&document)?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    Ok(NodeClient::new(
        endpoint,
        ReqwestTransport::with_timeout(Duration::from_secs(10))?,
    )?)
}

/// Mounts the query answer on `node` for a request the harness's bound
/// token and proof admit, and `401` for any other.
async fn answer_bound(node: &Server, authority: &NutsNode) {
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .and(authority.dpop_bound())
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
        )
        .with_priority(1)
        .mount(node)
        .await;
    Mock::given(method("POST"))
        .and(path("/openehr/v1/query/aql"))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(2)
        .mount(node)
        .await;
}

async fn query(client: &NodeClient<ReqwestTransport>) -> Result<NodeReply, Box<dyn Error>> {
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(5))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, conveyance());
    Ok(client.query(&NodeQuery::new(NODE_AQL), &options).await?)
}

/// The token the Nuts grant obtained reaches the node under the `DPoP`
/// scheme with a proof of the grant's key over the request and the token's
/// hash (Annex B §B.4; RFC 9449 §7.1; the IG's GFI-005).
// conformance: CP-17
#[tokio::test]
async fn the_nuts_token_reaches_the_node_dpop_bound() -> TestResult {
    let setup = setup().await?;
    let node = Server::start().await;
    answer_bound(&node, &setup.authority).await;
    let client = client(&node)?
        .with_credentials_provider(provider(setup.grant.clone(), Duration::from_secs(5))?)
        .with_dpop(&setup.prover);

    let reply = query(&client).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(vec![Verdict::Issued], setup.authority.verdicts());
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
    assert_eq!(setup.prover.thumbprint(), verified.jkt);
    Ok(())
}

/// A token is obtained once and served from the cache until its lifetime
/// nears its end.
#[tokio::test]
async fn the_nuts_token_is_cached() -> TestResult {
    let setup = setup().await?;
    let node = Server::start().await;
    answer_bound(&node, &setup.authority).await;
    let client = client(&node)?
        .with_credentials_provider(provider(setup.grant.clone(), Duration::from_secs(5))?)
        .with_dpop(&setup.prover);

    for _ in 0..3 {
        assert_eq!(EndpointStatus::Active, query(&client).await?.status());
    }
    assert_eq!(1, setup.authority.issued());
    assert_eq!(1, setup.authority.definitions_served());
    Ok(())
}

/// A refused grant fails the node with nothing sent to it, and the failure
/// names neither the credential nor the presentation (§11; RFC 6749 §5.2).
// conformance: CP-17
#[tokio::test]
async fn a_refused_nuts_grant_fails_the_node() -> TestResult {
    let setup = setup().await?;
    setup
        .authority
        .refuse(400, "invalid_request", "the presentation was refused");
    let node = Server::start().await;
    answer_bound(&node, &setup.authority).await;
    let client = client(&node)?
        .with_credentials_provider(provider(setup.grant.clone(), Duration::from_secs(5))?)
        .with_dpop(&setup.prover);

    let reply = query(&client).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    let rendered = format!("{:?} {:?}", reply.outcome(), reply.status());
    for part in setup.credential.split('.') {
        assert!(!rendered.contains(part), "the failure names the credential");
    }
    for form in setup.authority.forms() {
        for (name, value) in form {
            if name == "assertion" {
                for part in value.split('.').filter(|part| part.len() > 8) {
                    assert!(
                        !rendered.contains(part),
                        "the failure names the presentation"
                    );
                }
            }
        }
    }
    Ok(())
}

/// An authorization server that does not answer within the timeout fails
/// the node, with nothing sent to it (§11).
#[tokio::test]
async fn a_slow_nuts_grant_fails_the_node() -> TestResult {
    let setup = setup().await?;
    setup.authority.delay(Duration::from_secs(3));
    let node = Server::start().await;
    answer_bound(&node, &setup.authority).await;
    let client = client(&node)?
        .with_credentials_provider(provider(setup.grant.clone(), Duration::from_millis(500))?)
        .with_dpop(&setup.prover);

    let reply = query(&client).await?;
    assert_ne!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let requests = node.received_requests().await.ok_or("recording is on")?;
    assert!(requests.is_empty(), "the node was sent nothing");
    Ok(())
}

/// The grant's `Debug` names the holder's DID and key id, never a key or a
/// credential.
#[tokio::test]
async fn the_nuts_grant_renders_no_secret() -> TestResult {
    let setup = setup().await?;
    let rendered = format!(
        "{:?}",
        provider(setup.grant.clone(), Duration::from_secs(5))?
    );
    assert!(rendered.contains(HOLDER_KID));
    for part in setup.credential.split('.') {
        assert!(!rendered.contains(part), "Debug names the credential");
    }
    assert!(!rendered.contains("PRIVATE"));
    Ok(())
}
