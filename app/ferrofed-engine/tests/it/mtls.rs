// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Mutual TLS on the onward grant against the harness token endpoint and a
//! mock node, both behind a mutual-TLS front: the gateway authenticates by
//! its TLS client certificate with its `client_id` and no assertion (RFC 8705
//! §2), takes a token bound to that certificate and sends it only over
//! connections presenting it (§3), refuses a token bound to another
//! certificate before the node is sent anything (§3.1), and composes with
//! token exchange per caller (RFC 8693 §2.1) (§13.1, §13.4, N25, CP-17).
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::{Conveyance, Principal, Verification};
use ferrofed_engine::dispatch::reported::UNAUTHENTICATED;
use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, NodeQuery, NodeReply};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::client_credentials::ClientCredentials;
use ferrofed_engine::onward::grant::exchange::{Exchange, SubjectToken};
use ferrofed_engine::onward::keys::{KeyRing, SigningKey};
use ferrofed_engine::onward::mtls::{Thumbprint, TlsClientAuth};
use ferrofed_engine::onward::token::{GRANT_TYPE, TokenError};
use ferrofed_engine::onward::{ClientAuthentication, Grant, Scope, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::SecretUrl;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::issuer::{Claims, Issuer};
use ferrofed_testkit::oauth::{self, TokenEndpoint, Verdict};
use ferrofed_testkit::tls::MutualTls;
use jsonwebtoken::Algorithm;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{CredentialsProvider as _, ReqwestTransport};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, shared};

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope every token is requested with.
const SCOPE: &str = "system/aql-*.s";

/// The node a token is exchanged for (RFC 8707 §2).
const RESOURCE: &str = "https://cdr-a.example.org/openehr";

/// The audience the caller's token names: the gateway.
const AUDIENCE: &str = "urn:example:ferrofed-under-test";

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A synthetic subject the gateway resolved on and withholds.
const WITHHELD: &str = "SYNTHETIC-SUBJECT-4f1a";

/// The issuer of the callers' tokens, its key generated once per process.
#[expect(
    clippy::expect_used,
    reason = "a test process that cannot generate a key pair cannot test anything"
)]
static CALLERS: LazyLock<Issuer> =
    LazyLock::new(|| Issuer::new(UPSTREAM).expect("a test key pair should generate"));

/// The harness authorization server and a node on one origin, behind one
/// mutual-TLS front.
struct Harness {
    endpoint: TokenEndpoint,
    front: MutualTls,
}

impl Harness {
    /// An authorization server that authenticates the gateway by its
    /// certificate, with a node beside it that answers a token it issued and
    /// `401` to every other request.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let endpoint = TokenEndpoint::start(CLIENT_ID, Some(300)).await;
        endpoint.accept_tls_client_auth();
        Mock::given(method("POST"))
            .and(path("/openehr/v1/query/aql"))
            .and(endpoint.bearer())
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
            )
            .with_priority(1)
            .mount(endpoint.server())
            .await;
        Mock::given(method("POST"))
            .and(path("/openehr/v1/query/aql"))
            .respond_with(ResponseTemplate::new(401))
            .with_priority(2)
            .mount(endpoint.server())
            .await;
        let front = MutualTls::front(&endpoint.server().uri())?;
        Ok(Self { endpoint, front })
    }

    /// The token endpoint's URL behind the front.
    fn token_url(&self) -> SecretUrl {
        SecretUrl::new(format!("{}{}", self.front.origin(), oauth::TOKEN_PATH))
    }

    /// The thumbprint of the certificate the gateway presents, as the
    /// engine reads it from the client identity.
    fn thumbprint(&self) -> Result<Thumbprint, Box<dyn Error>> {
        Ok(Thumbprint::of_identity(&SecretString::from(
            self.front.client_identity(),
        ))?)
    }

    /// An engine trusting the front, presenting the client certificate when
    /// `present` is set.
    fn transport(&self, present: bool) -> Result<ReqwestTransport, Box<dyn Error>> {
        let mut builder = reqwest::Client::builder().tls_certs_merge(
            reqwest::Certificate::from_pem_bundle(self.front.trust_roots().as_bytes())?,
        );
        if present {
            builder = builder.identity(reqwest::Identity::from_pem(
                self.front.client_identity().as_bytes(),
            )?);
        }
        Ok(ReqwestTransport::with_builder_timeout(
            builder,
            Duration::from_secs(10),
        )?)
    }

    /// The grant authenticating by the certificate (RFC 8705 §2.1).
    fn grant(&self) -> Result<Grant, Box<dyn Error>> {
        Ok(
            Grant::new(&self.token_url(), CLIENT_ID, Scope::parse(SCOPE)?)?
                .with_tls_client_auth(TlsClientAuth::Pki),
        )
    }

    /// The node client over `transport`, sending what `client` configures.
    fn node(
        &self,
        transport: ReqwestTransport,
    ) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
        let document = format!(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
            self.front.origin()
        );
        let snapshot = RegistrySnapshot::from_toml_str(&document)?;
        let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
        Ok(NodeClient::new(endpoint, transport)?)
    }

    /// The requests the node was sent.
    async fn node_requests(&self) -> Result<usize, Box<dyn Error>> {
        Ok(self
            .endpoint
            .server()
            .received_requests()
            .await
            .ok_or("recording is on")?
            .iter()
            .filter(|request| request.url.path() == "/openehr/v1/query/aql")
            .count())
    }
}

/// The client-credentials provider of `grant` with no key of the gateway's.
fn by_certificate(
    grant: Grant,
    transport: ReqwestTransport,
) -> Result<Arc<ClientCredentials<ReqwestTransport>>, Box<dyn Error>> {
    Ok(Arc::new(ClientCredentials::by_certificate(
        EndpointId::new("node-a-pub")?,
        grant,
        Duration::from_secs(5),
        transport,
        Arc::new(SystemClock),
    )))
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

/// The gateway's keys, which sign the actor token of a token exchange.
fn keys() -> Result<Arc<KeyRing>, Box<dyn Error>> {
    let key = SigningKey::from_pem(&SecretString::from(oauth::es384_pem()?))?;
    Ok(Arc::new(KeyRing::new(
        key,
        None,
        Duration::ZERO,
        Arc::new(SystemClock),
    )?))
}

/// The engine reads the certificate's thumbprint as RFC 8705 §3.1 defines
/// it: the unpadded base64url SHA-256 of its DER, the value the front
/// computes from the certificate it admits.
#[tokio::test]
async fn the_thumbprint_is_that_of_the_presented_certificate() -> TestResult {
    let harness = Harness::start().await?;
    assert_eq!(
        harness.front.client_thumbprint(),
        harness.thumbprint()?.as_str()
    );
    Ok(())
}

/// The token request authenticates by the certificate: it names the client
/// with `client_id` and carries no assertion and no secret (RFC 8705 §2,
/// RFC 6749 §4.4.2).
// conformance: CP-17
#[tokio::test]
async fn the_gateway_authenticates_by_its_certificate_and_sends_no_assertion() -> TestResult {
    let harness = Harness::start().await?;
    harness.endpoint.expect_scope(SCOPE);
    let provider = by_certificate(harness.grant()?, harness.transport(true)?)?;
    provider.credentials().await?;

    assert_eq!(vec![Verdict::Issued], harness.endpoint.verdicts());
    let forms = harness.endpoint.forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    let names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(vec!["grant_type", "client_id", "scope"], names);
    assert_eq!(Some(GRANT_TYPE), field(form, "grant_type"));
    assert_eq!(Some(CLIENT_ID), field(form, "client_id"));
    assert_eq!(
        vec![harness.front.client_thumbprint().to_owned()],
        harness.front.presented(),
        "the token endpoint was reached over a connection presenting the certificate"
    );
    assert_eq!(
        ClientAuthentication::Tls(TlsClientAuth::Pki),
        provider.grant().client_authentication()
    );
    Ok(())
}

/// `self_signed_tls_client_auth` sends the same request as the PKI method:
/// the difference is the server's check of the certificate (RFC 8705 §2.2).
#[tokio::test]
async fn the_self_signed_method_sends_the_same_request() -> TestResult {
    let harness = Harness::start().await?;
    let grant = Grant::new(&harness.token_url(), CLIENT_ID, Scope::parse(SCOPE)?)?
        .with_tls_client_auth(TlsClientAuth::SelfSigned);
    by_certificate(grant, harness.transport(true)?)?
        .credentials()
        .await?;
    let forms = harness.endpoint.forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    assert_eq!(Some(CLIENT_ID), field(form, "client_id"));
    assert_eq!(None, field(form, "client_assertion"));
    Ok(())
}

/// A token bound to the certificate reaches the node, and every connection
/// it travelled over presented that certificate (RFC 8705 §3).
// conformance: CP-17
#[tokio::test]
async fn a_bound_token_travels_only_over_connections_presenting_its_certificate() -> TestResult {
    let harness = Harness::start().await?;
    harness
        .endpoint
        .bind_to_certificate(harness.front.client_thumbprint());
    let grant = harness
        .grant()?
        .with_certificate_binding(harness.thumbprint()?);
    let transport = harness.transport(true)?;
    let client = harness
        .node(transport.clone())?
        .with_credentials_provider(by_certificate(grant, transport)?);

    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(1, harness.node_requests().await?);
    let presented = harness.front.presented();
    assert!(!presented.is_empty());
    assert!(
        presented
            .iter()
            .all(|thumbprint| thumbprint == harness.front.client_thumbprint()),
        "{presented:?}"
    );
    Ok(())
}

/// A token whose `cnf` names another certificate is refused before it is
/// cached or sent: the node is sent nothing, and the answer is the fixed
/// sentence alone (RFC 8705 §3, §3.1; §11.1).
// conformance: CP-17
#[tokio::test]
async fn a_token_bound_to_another_certificate_never_reaches_the_node() -> TestResult {
    let harness = Harness::start().await?;
    let other = Thumbprint::of_certificate(b"a certificate the gateway does not hold");
    harness.endpoint.bind_to_certificate(other.as_str());
    let grant = harness
        .grant()?
        .with_certificate_binding(harness.thumbprint()?);
    let transport = harness.transport(true)?;
    let provider = by_certificate(grant, transport.clone())?;
    let client = harness
        .node(transport)?
        .with_credentials_provider(Arc::<ClientCredentials<ReqwestTransport>>::clone(&provider));

    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert!(!reply.contact().sent());
    match reply.outcome().error() {
        Some(ErrorDetail::Text(text)) => assert_eq!(UNAUTHENTICATED, text),
        other => return Err(format!("an unexpected error: {other:?}").into()),
    }
    assert_eq!(0, harness.node_requests().await?);

    let refused = provider
        .credentials()
        .await
        .expect_err("a mismatched token is refused again");
    let mismatch = Error::source(&refused)
        .and_then(|source| source.downcast_ref::<TokenError>())
        .ok_or("the cause is the token error")?;
    assert!(matches!(mismatch, TokenError::CertificateMismatch));
    let shown = format!("{refused} {mismatch} {mismatch:?}");
    assert!(!shown.contains("eyJ"), "no token is shown: {shown}");
    Ok(())
}

/// Without its certificate the gateway completes no handshake, so neither
/// the token endpoint nor the node is reached (RFC 8705 §2).
#[tokio::test]
async fn without_its_certificate_the_gateway_reaches_neither_the_token_endpoint_nor_the_node()
-> TestResult {
    let harness = Harness::start().await?;
    let transport = harness.transport(false)?;
    let client = harness
        .node(transport.clone())?
        .with_credentials_provider(by_certificate(harness.grant()?, transport)?);

    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert!(harness.front.refused() >= 1);
    assert!(harness.endpoint.forms().is_empty());
    assert_eq!(0, harness.node_requests().await?);
    Ok(())
}

/// A grant that authenticates by a client assertion and has no key gets no
/// token, and nothing is sent.
#[tokio::test]
async fn an_assertion_grant_without_a_key_sends_nothing() -> TestResult {
    let harness = Harness::start().await?;
    let grant = Grant::new(&harness.token_url(), CLIENT_ID, Scope::parse(SCOPE)?)?;
    let refused = by_certificate(grant, harness.transport(true)?)?
        .credentials()
        .await
        .expect_err("no key signs the assertion");
    let unsigned = Error::source(&refused)
        .and_then(|source| source.downcast_ref::<TokenError>())
        .ok_or("the cause is the token error")?;
    assert!(matches!(unsigned, TokenError::Unsigned));
    assert!(harness.endpoint.forms().is_empty());
    Ok(())
}

/// Token exchange composes with mutual TLS: the exchange authenticates by
/// the certificate with `client_id`, still names the gateway as the actor
/// with its assertion, and the token it issues, bound to the certificate,
/// reaches the node (RFC 8693 §2.1, RFC 8705 §2, §3; N25, N26).
// conformance: CP-16 CP-17
#[tokio::test]
async fn token_exchange_composes_with_mutual_tls() -> TestResult {
    let harness = Harness::start().await?;
    let keys = keys()?;
    harness.endpoint.trust(keys.published());
    harness.endpoint.accept_exchange(CALLERS.jwks(), UPSTREAM);
    harness.endpoint.expect_resource(RESOURCE);
    harness
        .endpoint
        .expect_assertion(Algorithm::ES384, harness.token_url().expose());
    harness
        .endpoint
        .bind_to_certificate(harness.front.client_thumbprint());
    let grant = harness
        .grant()?
        .with_resource(RESOURCE)?
        .with_token_exchange()
        .with_certificate_binding(harness.thumbprint()?);
    let transport = harness.transport(true)?;
    let exchange = Arc::new(Exchange::new(
        EndpointId::new("node-a-pub")?,
        grant,
        keys,
        (Duration::from_secs(300), Duration::from_secs(5)),
        transport.clone(),
        Arc::new(SystemClock),
    ));
    let client = harness.node(transport)?.with_on_behalf(exchange);
    let mut claims = Claims::new(UPSTREAM, AUDIENCE);
    crate::conveyed::SUBJECT.clone_into(&mut claims.sub);
    let token = CALLERS.mint(&claims)?;
    let mut principal = caller();
    principal.verified_by = Verification::Signature;
    let conveyance = Conveyance::new(shared(), Principal::Caller(principal))
        .with_subject(SubjectToken::new(SecretString::from(token), "user/aql-*.s"));

    let reply = query(&client, conveyance).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let forms = harness.endpoint.forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one exchange, got {}", forms.len()).into());
    };
    assert_eq!(Some(CLIENT_ID), field(form, "client_id"));
    assert_eq!(None, field(form, "client_assertion"));
    assert!(field(form, "actor_token").is_some());
    assert_eq!(1, harness.node_requests().await?);
    Ok(())
}

/// No key, certificate or token reaches a `Debug` rendering of the grant or
/// the provider, and the withheld subject reaches neither the token
/// endpoint nor the node (N33).
#[tokio::test]
async fn no_key_token_or_withheld_subject_leaves_the_gateway() -> TestResult {
    let harness = Harness::start().await?;
    harness
        .endpoint
        .bind_to_certificate(harness.front.client_thumbprint());
    let grant = harness
        .grant()?
        .with_certificate_binding(harness.thumbprint()?);
    let transport = harness.transport(true)?;
    let provider = by_certificate(grant, transport.clone())?;
    let client = harness
        .node(transport)?
        .with_credentials_provider(Arc::<ClientCredentials<ReqwestTransport>>::clone(&provider));
    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(EndpointStatus::Active, reply.status());

    let shown = format!("{provider:?} {:?}", provider.grant());
    for hidden in ["PRIVATE KEY", "CERTIFICATE", "eyJ"] {
        assert!(!shown.contains(hidden), "{hidden} is not shown: {shown}");
    }
    assert!(shown.contains(harness.front.client_thumbprint()), "{shown}");
    let requests = harness
        .endpoint
        .server()
        .received_requests()
        .await
        .ok_or("recording is on")?;
    for request in &requests {
        let seen = format!(
            "{} {:?} {}",
            request.url,
            request.headers,
            String::from_utf8_lossy(&request.body)
        );
        assert!(!seen.contains(WITHHELD), "{seen}");
    }
    Ok(())
}
