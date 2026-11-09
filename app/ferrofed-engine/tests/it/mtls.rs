// SPDX-FileCopyrightText: Cadasto B.V.
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
use std::sync::Arc;
use std::time::{Duration, Instant};

use ferrofed_engine::conveyance::Conveyance;
use ferrofed_engine::conveyance::caller::{Principal, Verification};
use ferrofed_engine::dispatch::reported::UNAUTHENTICATED;
use ferrofed_engine::dispatch::{DispatchOptions, NodeClient, NodeQuery, NodeReply};
use ferrofed_engine::hygiene::Withheld;
use ferrofed_engine::onward::grant::lane::{Lane, Organisations};
use ferrofed_engine::onward::upstream::UpstreamToken;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::issuance::Verdict;
use ferrofed_testkit::leak;
use ferrofed_testkit::oauth::{self, TokenEndpoint};
use ferrofed_testkit::proxy::Capture;
use ferrofed_testkit::tls::MutualTls;
use ferrohealth_smart::client::{
    ClientAuth, ClientCredentials, Endpoint, PresentedTls, TokenError,
};
use ferrohealth_smart::kind::IssuerKind;
use ferrohealth_smart::sender::ClientCertificate;
use jsonwebtoken::Algorithm;
use openehr_federation::outcome::ErrorDetail;
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{CredentialsProvider as _, ReqwestTransport};
use secrecy::SecretString;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use crate::conveyed::{UPSTREAM, caller, shared};
use crate::exchange;

type TestResult = Result<(), Box<dyn Error>>;

/// The client the node's authorization server registered the gateway as.
const CLIENT_ID: &str = "ferrofed-test-gateway";

/// The scope every token is requested with.
const SCOPE: &str = "system/aql-*.s";

/// A synthetic node query, scoped to an `ehr_id` under no real system.
const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

/// An empty ITS-REST `RESULT_SET`.
const EMPTY_RESULT_SET: &str = r##"{"q":"SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c","columns":[{"name":"#0","path":"c/uid/value"}],"rows":[]}"##;

/// A synthetic subject the gateway resolved on and withholds.
const WITHHELD: &str = "SYNTHETIC-SUBJECT-4f1a";

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
    fn token_url(&self) -> Result<Url, url::ParseError> {
        Url::parse(&format!("{}{}", self.front.origin(), oauth::TOKEN_PATH))
    }

    /// The TLS material the gateway reaches the front with, read once as a
    /// node's `tls` table is, and handed to the grant as it is (RFC 8705
    /// §2, §3).
    fn presented(&self) -> Result<PresentedTls, Box<dyn Error>> {
        let material = self.front.tls_client("credentials.node-a-pub.tls")?;
        Ok(PresentedTls::new(
            Arc::new(material.client_config(rustls::RootCertStore::empty())?),
            material.certificate().cloned(),
        ))
    }

    /// The certificate the gateway presents, as `ferrohealth-smart` reads
    /// it from the presented material.
    fn certificate(&self) -> Result<ClientCertificate, Box<dyn Error>> {
        Ok(self
            .grant()?
            .tls
            .presented_certificate()
            .ok_or("the material holds a certificate")?)
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

    /// The grant authenticating by the certificate, by `tls_client_auth`
    /// (RFC 8705 §2.1) or `self_signed_tls_client_auth` (§2.2), trusting the
    /// front.
    fn grant_by(&self, self_signed: bool) -> Result<ClientCredentials, Box<dyn Error>> {
        let mut grant = ClientCredentials::with_presented_tls(
            Endpoint::Token(self.token_url()?),
            CLIENT_ID.to_owned(),
            self.presented()?,
            self_signed,
        );
        grant.scope = Some(SCOPE.to_owned());
        grant.timeout = Duration::from_secs(5);
        Ok(grant)
    }

    /// The grant authenticating by `tls_client_auth` (RFC 8705 §2.1).
    fn grant(&self) -> Result<ClientCredentials, Box<dyn Error>> {
        self.grant_by(false)
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

/// The client-credentials provider of `grant`.
fn by_certificate(grant: ClientCredentials) -> Result<Arc<UpstreamToken>, Box<dyn Error>> {
    Ok(UpstreamToken::provider(grant)?)
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

/// The certificate's thumbprint is read as RFC 8705 §3.1 defines it: the
/// unpadded base64url SHA-256 of its DER, the value the front computes from
/// the certificate it admits.
#[tokio::test]
async fn the_thumbprint_is_that_of_the_presented_certificate() -> TestResult {
    let harness = Harness::start().await?;
    assert_eq!(
        harness.front.client_thumbprint(),
        harness.certificate()?.thumbprint()
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
    let grant = harness.grant()?;
    assert_eq!(ClientAuth::TlsClientAuth, grant.client_auth);
    by_certificate(grant)?.credentials().await?;

    assert_eq!(
        vec![Verdict::Issued],
        harness.endpoint.issuance().verdicts()
    );
    let forms = harness.endpoint.issuance().forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    // NOTE: RFC 6749 Appendix B, a form's parameters carry no order, so the
    // request is compared as the set of names it sends, each once.
    let mut names: Vec<&str> = form.iter().map(|(name, _)| name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(vec!["client_id", "grant_type", "scope"], names);
    assert_eq!(Some("client_credentials"), field(form, "grant_type"));
    assert_eq!(Some(CLIENT_ID), field(form, "client_id"));
    assert_eq!(
        vec![harness.front.client_thumbprint().to_owned()],
        harness.front.presented(),
        "the token endpoint was reached over a connection presenting the certificate"
    );
    Ok(())
}

/// `self_signed_tls_client_auth` sends the same request as the PKI method:
/// the difference is the server's check of the certificate (RFC 8705 §2.2).
#[tokio::test]
async fn the_self_signed_method_sends_the_same_request() -> TestResult {
    let harness = Harness::start().await?;
    let grant = harness.grant_by(true)?;
    assert_eq!(ClientAuth::SelfSignedTlsClientAuth, grant.client_auth);
    by_certificate(grant)?.credentials().await?;
    let forms = harness.endpoint.issuance().forms();
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
    let grant = harness.grant()?.with_certificate_binding();
    let client = harness
        .node(harness.transport(true)?)?
        .with_credentials_provider(by_certificate(grant)?);

    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(1, harness.node_requests().await?);
    let presented = harness.front.presented();
    assert_ne!(presented, Vec::<String>::new());
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
    let other = ClientCertificate::from_der(b"a certificate the gateway does not hold");
    harness.endpoint.bind_to_certificate(other.thumbprint());
    let grant = harness.grant()?.with_certificate_binding();
    let provider = by_certificate(grant)?;
    let client = harness
        .node(harness.transport(true)?)?
        .with_credentials_provider(Arc::<UpstreamToken>::clone(&provider));

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
        .and_then(<dyn Error>::downcast_ref::<Arc<TokenError>>)
        .ok_or("the cause is the token error")?;
    assert!(matches!(mismatch.as_ref(), TokenError::CertificateMismatch));
    let shown = format!("{refused} {mismatch} {mismatch:?}");
    assert!(!shown.contains("eyJ"), "no token is shown: {shown}");
    Ok(())
}

/// A grant that authenticates by a certificate it does not hold sends no
/// token request, so neither the token endpoint nor the node is reached
/// (RFC 8705 §2).
#[tokio::test]
async fn without_its_certificate_the_gateway_reaches_neither_the_token_endpoint_nor_the_node()
-> TestResult {
    let harness = Harness::start().await?;
    let mut grant = harness.grant()?;
    grant.tls.presented = Some(PresentedTls::new(
        Arc::clone(harness.presented()?.config()),
        None,
    ));
    let client = harness
        .node(harness.transport(false)?)?
        .with_credentials_provider(by_certificate(grant)?);

    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(EndpointStatus::NodeError, reply.status());
    assert_eq!(0, harness.front.handshakes());
    assert_eq!(
        harness.endpoint.issuance().forms(),
        Vec::<Vec<(String, String)>>::new()
    );
    assert_eq!(0, harness.node_requests().await?);
    Ok(())
}

/// A grant that authenticates by a client assertion and has no key gets no
/// token, and nothing is sent.
#[tokio::test]
async fn an_assertion_grant_without_a_key_sends_nothing() -> TestResult {
    let harness = Harness::start().await?;
    let mut grant = harness.grant()?;
    grant.client_auth = ClientAuth::PrivateKeyJwt;
    let refused = by_certificate(grant)?
        .credentials()
        .await
        .expect_err("no key signs the assertion");
    let unsigned = Error::source(&refused)
        .and_then(<dyn Error>::downcast_ref::<Arc<TokenError>>)
        .ok_or("the cause is the token error")?;
    assert!(
        matches!(unsigned.as_ref(), TokenError::Credential { .. }),
        "{unsigned:?}"
    );
    assert_eq!(
        harness.endpoint.issuance().forms(),
        Vec::<Vec<(String, String)>>::new()
    );
    Ok(())
}

/// Token exchange composes with mutual TLS: the exchange authenticates by
/// the certificate with `client_id`, still names the gateway as the actor
/// with an assertion of its own key, and the token it issues, bound to the
/// certificate, reaches the node (RFC 8693 §2.1, RFC 8705 §2, §3; N25,
/// N26).
// conformance: CP-16 CP-17
#[tokio::test]
async fn token_exchange_composes_with_mutual_tls() -> TestResult {
    let harness = Harness::start().await?;
    let keys = exchange::keys()?;
    harness
        .endpoint
        .trust(serde_json::from_value(keys.published())?);
    harness
        .endpoint
        .accept_exchange(exchange::CALLERS.jwks(), UPSTREAM);
    harness
        .endpoint
        .issue_onward_tokens(exchange::ONWARD_ISSUER);
    harness.endpoint.expect_resource(exchange::RESOURCE);
    harness
        .endpoint
        .expect_assertion(Algorithm::ES384, harness.token_url()?.as_str());
    harness
        .endpoint
        .bind_to_certificate(harness.front.client_thumbprint());
    let mut grant = harness.grant()?.with_certificate_binding();
    grant.assertion_keys = Some(keys);
    let gateway = UpstreamToken::provider(grant.clone())?;
    let resolved = exchange::section(
        &harness.endpoint,
        serde_json::json!({"token_endpoint": harness.token_url()?.as_str()}),
    )?
    .resolve(grant)?;
    let lane = Arc::new(Lane::new(
        EndpointId::new("node-a-pub")?,
        (resolved.onward, resolved.target),
        (IssuerKind::Persons, Organisations::Refused),
        gateway,
    ));
    let client = harness.node(harness.transport(true)?)?.with_on_behalf(lane);
    let token = exchange::caller_token(crate::conveyed::SUBJECT)?;
    let mut principal = caller();
    principal.verified_by = Verification::Signature;
    let conveyance = Conveyance::new(shared(), Principal::Caller(principal)).with_subject(
        exchange::subject_of(&token, crate::conveyed::SUBJECT, "user/aql-*.s")?,
    );

    let reply = query(&client, conveyance).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    let forms = harness.endpoint.issuance().forms();
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
    let grant = harness.grant()?.with_certificate_binding();
    let provider = by_certificate(grant.clone())?;
    let client = harness
        .node(harness.transport(true)?)?
        .with_credentials_provider(Arc::<UpstreamToken>::clone(&provider));
    let reply = query(&client, crate::conveyed::conveyance()).await?;
    assert_eq!(EndpointStatus::Active, reply.status());

    let shown = format!("{provider:?} {grant:?}");
    for hidden in ["PRIVATE KEY", "CERTIFICATE", "eyJ"] {
        assert!(!shown.contains(hidden), "{hidden} is not shown: {shown}");
    }
    assert!(shown.contains(CLIENT_ID), "{shown}");
    let requests = harness
        .endpoint
        .server()
        .received_requests()
        .await
        .ok_or("recording is on")?;
    let journal: Vec<Capture> = requests.iter().map(Capture::from).collect();
    leak::assert_withheld(&journal, &[WITHHELD], leak::Scope::WholeRequest, "N33");
    Ok(())
}
