// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The nonces of `DPoP` (RFC 9449 §8, §9): a token request sent once more
//! for a nonce signs its assertions anew (RFC 7523 §3), the nonces of the
//! authorization server and the node are kept apart even on one origin
//! (§9), and a node request whose deadline passes before the nonce re-send
//! is the time-out of a node that was asked, never a request never sent.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ferrofed_engine::dispatch::{
    Contact, DispatchOptions, NodeClient, NodeQuery, SharedCredentials,
};
use ferrofed_engine::onward::conveyance::{Conveyance, Principal, Verification};
use ferrofed_engine::onward::exchange::{Exchange, SubjectToken};
use ferrofed_engine::onward::keys::KeyRing;
use ferrofed_engine::onward::{Grant, Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::dpop::{self, HEADER};
use ferrofed_testkit::issuer::Claims;
use ferrofed_testkit::mock::Server;
use ferrofed_testkit::oauth::{self, TokenEndpoint, Verdict};
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{Credentials, CredentialsProvider as _, Transport, TransportError};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Match, Mock, ResponseTemplate};

use super::{
    CALLERS, CLIENT_ID, SCOPE, TestResult, answer_when, client, endpoint, grant, keys, prover,
    provider, query, transport,
};
use crate::conveyed::{UPSTREAM, caller, shared};

/// The `jti` of the assertion each token request carried in `field`,
/// verified against `keys` (RFC 7523 §3).
fn jtis(
    endpoint: &TokenEndpoint,
    keys: &KeyRing,
    field: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut jtis = Vec::new();
    for form in endpoint.forms() {
        let assertion = form
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value.as_str())
            .ok_or_else(|| format!("a token request without {field}"))?;
        let claims = oauth::verify(
            assertion,
            &keys.published(),
            CLIENT_ID,
            &endpoint.token_url(),
        )?;
        jtis.push(claims.jti);
    }
    Ok(jtis)
}

/// A token request sent once more for the nonce the token endpoint demanded
/// carries a client assertion signed anew, with a `jti` of its own, and a
/// proof naming that nonce: the endpoint issues only to a proof that names
/// it (RFC 7523 §3, RFC 9449 §8).
// conformance: CP-17
#[tokio::test]
async fn a_nonce_resend_signs_a_new_client_assertion() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    endpoint.require_nonce("synthetic-as-nonce-4");
    let prover = prover()?;
    let provider = provider(grant(&endpoint, &prover)?, Arc::clone(&keys), transport()?)?;

    provider.credentials().await?;
    assert_eq!(
        vec![
            Verdict::Refused(String::from("use_dpop_nonce")),
            Verdict::Issued
        ],
        endpoint.verdicts()
    );
    let sent = jtis(&endpoint, &keys, "client_assertion")?;
    let [first, again] = sent.as_slice() else {
        return Err(format!("expected two token requests, got {}", sent.len()).into());
    };
    assert_ne!(first, again, "the resend repeats no assertion jti");
    Ok(())
}

/// A token exchange sent once more for a demanded nonce carries a client
/// assertion and an actor token both signed anew (RFC 7523 §3, RFC 8693
/// §2.1, RFC 9449 §8).
// conformance: CP-17
#[tokio::test]
async fn a_nonce_resend_of_an_exchange_signs_new_assertions() -> TestResult {
    let keys = keys()?;
    let endpoint = endpoint(&keys).await;
    endpoint.accept_exchange(CALLERS.jwks(), UPSTREAM);
    endpoint.require_nonce("synthetic-as-nonce-5");
    let node = Server::start().await;
    answer_when(&node, endpoint.dpop_bound(None), ResponseTemplate::new(401)).await;
    let prover = prover()?;
    let grant = grant(&endpoint, &prover)?
        .with_resource("https://cdr-a.example.org/openehr")?
        .with_token_exchange();
    let exchange = Arc::new(Exchange::new(
        EndpointId::new("node-a-pub")?,
        grant,
        Arc::clone(&keys),
        (Duration::from_secs(300), Duration::from_secs(5)),
        transport()?,
        Arc::new(SystemClock),
    ));
    let client = client(&node, "openehr", transport()?)?
        .with_on_behalf(exchange)
        .with_dpop(&prover);
    let mut claims = Claims::new(UPSTREAM, "urn:example:ferrofed-under-test");
    crate::conveyed::SUBJECT.clone_into(&mut claims.sub);
    let mut verified = caller();
    verified.verified_by = Verification::Signature;
    let conveyed = Conveyance::new(shared(), Principal::Caller(verified)).with_subject(
        SubjectToken::new(SecretString::from(CALLERS.mint(&claims)?), "user/aql-*.s"),
    );

    let reply = query(&client, conveyed).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    for field in ["client_assertion", "actor_token"] {
        let sent = jtis(&endpoint, &keys, field)?;
        let [first, again] = sent.as_slice() else {
            return Err(format!("expected two exchanges, got {}", sent.len()).into());
        };
        assert_ne!(first, again, "the resend repeats no {field} jti");
    }
    Ok(())
}

/// A request whose `DPoP` proof to `htu` names `nonce`.
struct ProofNaming {
    htu: String,
    nonce: &'static str,
}

impl Match for ProofNaming {
    fn matches(&self, request: &wiremock::Request) -> bool {
        nonce_of(request, &self.htu).is_ok_and(|named| named.as_deref() == Some(self.nonce))
    }
}

/// The nonce the `DPoP` proof of `request` to `htu` names, or `None` for a
/// proof that names none.
///
/// # Errors
///
/// Returns why `request` carries no proof that verifies.
fn nonce_of(request: &wiremock::Request, htu: &str) -> Result<Option<String>, String> {
    let token = request
        .headers
        .get(http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("DPoP "));
    let proof = request
        .headers
        .get(HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or("the request carries no proof")?;
    Ok(dpop::verify(proof, request.method.as_str(), htu, token)?
        .proof
        .nonce)
}

/// The authorization server and the node share an origin and demand
/// different nonces: every token request names the authorization server's
/// nonce or none, and every node request the node's or none (RFC 9449 §9).
// conformance: CP-17
#[tokio::test]
async fn the_token_endpoint_and_the_node_keep_their_own_nonces() -> TestResult {
    const AS_NONCE: &str = "synthetic-as-nonce-6";
    const RS_NONCE: &str = "synthetic-rs-nonce-6";
    let keys = keys()?;
    let server = Server::start().await;
    let token_url = format!("{}/token", server.uri());
    let node_url = format!("{}/openehr/v1/query/aql", server.uri());
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(ProofNaming {
            htu: token_url.clone(),
            nonce: AS_NONCE,
        })
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            br#"{"access_token":"synthetic-bound-token-6","token_type":"DPoP"}"#.to_vec(),
            "application/json",
        ))
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_raw(
                    br#"{"error":"use_dpop_nonce"}"#.to_vec(),
                    "application/json",
                )
                .insert_header(dpop::NONCE_HEADER, AS_NONCE),
        )
        .with_priority(2)
        .mount(&server)
        .await;
    answer_when(
        &server,
        ProofNaming {
            htu: node_url.clone(),
            nonce: RS_NONCE,
        },
        dpop::challenge(RS_NONCE),
    )
    .await;
    let prover = prover()?;
    let grant = Grant::new(
        &SecretUrl::new(token_url.clone()),
        CLIENT_ID,
        Scope::parse(SCOPE)?,
    )?
    .with_dpop(Arc::clone(&prover));
    let provider = provider(grant, keys, transport()?)?;
    let client = client(&server, "openehr", transport()?)?
        .with_credentials_provider(provider)
        .with_dpop(&prover);

    for _ in 0..2 {
        let reply = query(&client, crate::conveyed::conveyance()).await?;
        assert_eq!(
            EndpointStatus::Active,
            reply.status(),
            "{:?}",
            reply.outcome()
        );
    }
    let mut token_nonces = Vec::new();
    let mut node_nonces = Vec::new();
    for request in server.received_requests().await.ok_or("recording is on")? {
        if request.url.path() == "/token" {
            token_nonces.push(nonce_of(&request, &token_url)?);
        } else {
            node_nonces.push(nonce_of(&request, &node_url)?);
        }
    }
    let named = |nonce: &str| Some(nonce.to_owned());
    assert_eq!(
        vec![None, named(AS_NONCE), named(AS_NONCE)],
        token_nonces,
        "the token endpoint is sent its own nonce, never the node's"
    );
    assert_eq!(
        vec![None, named(RS_NONCE), named(RS_NONCE)],
        node_nonces,
        "the node is sent its own nonce, never the token endpoint's"
    );
    Ok(())
}

/// An engine that answers every request with a node's `DPoP` nonce
/// challenge (RFC 9449 §9), only after `delay`, past any deadline the
/// request carries.
#[derive(Debug, Clone)]
struct LateChallenge {
    delay: Duration,
    sends: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl Transport for LateChallenge {
    async fn send(
        &self,
        _request: http::Request<Vec<u8>>,
    ) -> Result<http::Response<Vec<u8>>, TransportError> {
        self.sends.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.delay).await;
        http::Response::builder()
            .status(http::StatusCode::UNAUTHORIZED)
            .header(
                http::header::WWW_AUTHENTICATE,
                "DPoP error=\"use_dpop_nonce\", error_description=\"a nonce is required\"",
            )
            .header(dpop::NONCE_HEADER, "synthetic-rs-nonce-7")
            .body(Vec::new())
            .map_err(|source| TransportError::Send {
                source: Box::new(source),
            })
    }
}

/// A node that answers with a nonce challenge after the request's deadline
/// was asked: the deadline that passes before the nonce re-send is the
/// node's `time-out`, and the request is read as one that left (§11.1,
/// RFC 9449 §9).
// conformance: CP-17
#[tokio::test]
async fn a_deadline_before_the_nonce_resend_is_a_time_out_of_a_node_that_was_asked() -> TestResult {
    let snapshot = RegistrySnapshot::from_toml_str(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"https://cdr-a.example.org/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
    )?;
    let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
    let engine = LateChallenge {
        delay: Duration::from_millis(400),
        sends: Arc::new(AtomicUsize::new(0)),
    };
    let token: SharedCredentials = Arc::new(Credentials::dpop("synthetic-bound-token-7"));
    let client = NodeClient::new(endpoint, engine.clone())?
        .with_credentials_provider(token)
        .with_dpop(&prover()?);
    let deadline = Instant::now()
        .checked_add(Duration::from_millis(150))
        .ok_or("the deadline is past the platform clock")?;
    let options = DispatchOptions::new(deadline, crate::conveyed::conveyance());

    let reply = client
        .query(&NodeQuery::new(super::NODE_AQL), &options)
        .await?;
    assert_eq!(1, engine.sends.load(Ordering::SeqCst), "one request left");
    assert_eq!(
        EndpointStatus::TimeOut,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(Contact::Silent, reply.contact(), "a node that was asked");
    assert!(reply.contact().sent());
    let Outcome::TimeOut {
        error: ErrorDetail::Text(text),
        ..
    } = reply.outcome()
    else {
        return Err(format!("{:?}", reply.outcome()).into());
    };
    assert!(text.contains("nonce"), "{text}");
    assert!(!text.contains("before the request was sent"), "{text}");
    Ok(())
}
