// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The FAPI 2.0 grant over mutual TLS, the profile's other choice for client
//! authentication and sender-constraining (FAPI 2.0 Security Profile
//! §5.3.2.1; RFC 8705 §2, §3, §3.3, §5): the server is discovered from its
//! issuer behind a mutual-TLS front, the token endpoint of
//! `mtls_endpoint_aliases` is used, the gateway authenticates by its
//! certificate, and the certificate-bound token reaches the node.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::dispatch::NodeClient;
use ferrofed_engine::onward::grant::fapi2::metadata::DiscoveryError;
use ferrofed_engine::onward::grant::fapi2::{
    Fapi2Credentials, Fapi2Error, Fapi2Grant, Fapi2GrantError, Fapi2Security,
};
use ferrofed_engine::onward::mtls::{Thumbprint, TlsClientAuth};
use ferrofed_engine::onward::{ClientAuthentication, Scope, SenderConstraint, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use ferrofed_testkit::fapi::{AuthorizationServer, Metadata};
use ferrofed_testkit::oauth::{self, Verdict};
use ferrofed_testkit::tls::MutualTls;
use oauth_server_metadata::{AliasHost, EndpointError, Issuer};
use openehr_federation::status::EndpointStatus;
use openehr_its::rest::client::{CredentialsProvider as _, ReqwestTransport};
use secrecy::SecretString;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

use super::{CLIENT_ID, EMPTY_RESULT_SET, SCOPE, TestResult, client_key, field, prover, query};
use crate::conveyed::conveyance;

/// A FAPI 2.0 server that authenticates by certificate, with a node beside
/// it, both behind one mutual-TLS front, and a second front on another
/// origin, `door`, that a mutual-TLS alias may name.
struct Harness {
    server: AuthorizationServer,
    front: MutualTls,
    door: MutualTls,
}

impl Harness {
    /// The server, the node and the front, the metadata naming the front as
    /// the issuer, the alias as the token endpoint a mutual-TLS client uses,
    /// `tls_client_auth`, and certificate-bound tokens.
    async fn start() -> Result<Self, Box<dyn Error>> {
        let server = AuthorizationServer::mutual_tls(CLIENT_ID, Some(300)).await;
        Mock::given(method("POST"))
            .and(path("/openehr/v1/query/aql"))
            .and(server.endpoint().bearer())
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(EMPTY_RESULT_SET.as_bytes().to_vec(), "application/json"),
            )
            .with_priority(1)
            .mount(server.endpoint().server())
            .await;
        Mock::given(method("POST"))
            .and(path("/openehr/v1/query/aql"))
            .respond_with(ResponseTemplate::new(401))
            .with_priority(2)
            .mount(server.endpoint().server())
            .await;
        let (front, door) = MutualTls::pair(&server.issuer())?;
        let harness = Self {
            server,
            front,
            door,
        };
        harness.publish(|_| {});
        harness
            .server
            .endpoint()
            .bind_to_certificate(harness.front.client_thumbprint());
        Ok(harness)
    }

    /// Publishes the mutual-TLS metadata, edited by `edit`.
    fn publish(&self, edit: impl FnOnce(&mut Metadata)) {
        let origin = self.front.origin();
        let mut metadata = self.server.metadata();
        metadata.issuer = Some(origin.clone());
        metadata.token_endpoint = Some(format!("{origin}/conventional/token"));
        metadata.mtls_endpoint_aliases = Some(BTreeMap::from([(
            String::from("token_endpoint"),
            format!("{origin}{}", oauth::TOKEN_PATH),
        )]));
        metadata.token_endpoint_auth_methods_supported =
            Some(vec![String::from("tls_client_auth")]);
        metadata.token_endpoint_auth_signing_alg_values_supported = None;
        metadata.dpop_signing_alg_values_supported = None;
        metadata.tls_client_certificate_bound_access_tokens = Some(true);
        edit(&mut metadata);
        self.server.publish(&metadata);
    }

    /// The thumbprint of the certificate the gateway presents.
    fn thumbprint(&self) -> Result<Thumbprint, Box<dyn Error>> {
        Ok(Thumbprint::of_identity(&SecretString::from(
            self.front.client_identity(),
        ))?)
    }

    /// The grant authenticating by, and binding its tokens to, the
    /// certificate, with no key of its own.
    fn grant(&self) -> Result<Fapi2Grant, Box<dyn Error>> {
        let security = Fapi2Security {
            client_auth: ClientAuthentication::Tls(TlsClientAuth::Pki),
            sender: SenderConstraint::Certificate(self.thumbprint()?),
            client_key: None,
        };
        Ok(Fapi2Grant::secured(
            Issuer::parse(&self.front.origin())?,
            CLIENT_ID,
            security,
            (Some(Scope::parse(SCOPE)?), None),
        )?)
    }

    /// An engine trusting the front and presenting the client certificate.
    fn transport(&self) -> Result<ReqwestTransport, Box<dyn Error>> {
        let builder = reqwest::Client::builder()
            .tls_certs_merge(reqwest::Certificate::from_pem_bundle(
                self.front.trust_roots().as_bytes(),
            )?)
            .identity(reqwest::Identity::from_pem(
                self.front.client_identity().as_bytes(),
            )?);
        Ok(ReqwestTransport::with_builder_timeout(
            builder,
            Duration::from_secs(10),
        )?)
    }

    /// The provider of `grant` over the certificate-presenting engine.
    fn provider(
        &self,
        grant: Fapi2Grant,
    ) -> Result<Arc<Fapi2Credentials<ReqwestTransport>>, Box<dyn Error>> {
        Ok(Arc::new(Fapi2Credentials::new(
            EndpointId::new("node-a-pub")?,
            grant,
            (Duration::from_secs(300), Duration::from_secs(5)),
            self.transport()?,
            Arc::new(SystemClock),
        )))
    }

    /// The node client behind the front, sending what `provider` gives it.
    fn client(
        &self,
        provider: Arc<Fapi2Credentials<ReqwestTransport>>,
    ) -> Result<NodeClient<ReqwestTransport>, Box<dyn Error>> {
        let document = format!(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"{}/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
            self.front.origin()
        );
        let snapshot = RegistrySnapshot::from_toml_str(&document)?;
        let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
        Ok(NodeClient::new(endpoint, self.transport()?)?.with_credentials_provider(provider))
    }
}

/// The grant discovers the server, takes the mutual-TLS alias of the token
/// endpoint, authenticates by its certificate with `client_id` alone, and
/// the certificate-bound token reaches the node (FAPI 2.0 §5.3.2.1; RFC 8705
/// §2, §3, §5).
// conformance: CP-17
#[tokio::test]
async fn a_fapi2_grant_authenticates_and_binds_its_tokens_by_certificate() -> TestResult {
    let harness = Harness::start().await?;
    let client = harness.client(harness.provider(harness.grant()?)?)?;

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(vec![Verdict::Issued], harness.server.endpoint().verdicts());
    let forms = harness.server.endpoint().forms();
    let [form] = forms.as_slice() else {
        return Err(format!("expected one token request, got {}", forms.len()).into());
    };
    assert_eq!(Some(CLIENT_ID), field(form, "client_id"));
    assert_eq!(None, field(form, "client_assertion"));
    let presented = harness.front.presented();
    assert!(
        presented
            .iter()
            .all(|thumbprint| thumbprint == harness.front.client_thumbprint()),
        "{presented:?}"
    );
    Ok(())
}

/// What the grant meets once the metadata is `edit`ed: the discovery error,
/// with no token request sent.
async fn refused_after(
    edit: impl FnOnce(&mut Metadata),
) -> Result<Option<DiscoveryError>, Box<dyn Error>> {
    let harness = Harness::start().await?;
    harness.publish(edit);
    let error = harness
        .provider(harness.grant()?)?
        .credentials()
        .await
        .err()
        .ok_or("the grant is refused")?;
    assert!(
        harness.server.endpoint().forms().is_empty(),
        "a refused discovery sends no token request"
    );
    Ok(
        match Error::source(&error).and_then(|source| source.downcast_ref::<Fapi2Error>()) {
            Some(Fapi2Error::Discovery(DiscoveryError::CertificateBinding)) => {
                Some(DiscoveryError::CertificateBinding)
            }
            Some(Fapi2Error::Discovery(DiscoveryError::ClientAuthentication)) => {
                Some(DiscoveryError::ClientAuthentication)
            }
            _ => None,
        },
    )
}

/// A server that does not state `tls_client_certificate_bound_access_tokens`
/// issues no bound token, which a grant bound to its certificate takes alone
/// (RFC 8705 §3.3: omitted, it is false).
#[tokio::test]
async fn a_server_without_certificate_bound_tokens_is_refused() -> TestResult {
    for stated in [None, Some(false)] {
        let met = refused_after(|metadata| {
            metadata.tls_client_certificate_bound_access_tokens = stated;
        })
        .await?;
        assert!(
            matches!(met, Some(DiscoveryError::CertificateBinding)),
            "{met:?}"
        );
    }
    Ok(())
}

/// A server that does not list `tls_client_auth` does not take the
/// certificate as client authentication (RFC 8414 §2, RFC 8705 §2.1.1).
#[tokio::test]
async fn a_server_without_tls_client_auth_is_refused() -> TestResult {
    let met = refused_after(|metadata| {
        metadata.token_endpoint_auth_methods_supported =
            Some(vec![String::from("private_key_jwt")]);
    })
    .await?;
    assert!(
        matches!(met, Some(DiscoveryError::ClientAuthentication)),
        "{met:?}"
    );
    Ok(())
}

/// Publishes the token endpoint alias on the second front's origin, as RFC
/// 8705 §5's example puts it on another host, and returns that origin's
/// host and port.
fn alias_on_the_door(harness: &Harness) -> Result<String, Box<dyn Error>> {
    let door = harness.door.origin();
    let alias = format!("{door}{}", oauth::TOKEN_PATH);
    harness.publish(|metadata| {
        metadata.mtls_endpoint_aliases =
            Some(BTreeMap::from([(String::from("token_endpoint"), alias)]));
    });
    Ok(door
        .strip_prefix("https://")
        .ok_or("the door is an https origin")?
        .to_owned())
}

/// A `token_endpoint` alias on a host the grant names is taken: the token
/// request goes there, by the same certificate, and the token reaches the
/// node (RFC 8705 §5).
// conformance: CP-17
#[tokio::test]
async fn an_alias_on_a_host_the_grant_names_is_taken() -> TestResult {
    let harness = Harness::start().await?;
    let host = alias_on_the_door(&harness)?;
    let grant = harness
        .grant()?
        .with_mtls_alias_hosts(vec![AliasHost::parse(&host)?])?;
    let client = harness.client(harness.provider(grant)?)?;

    let reply = query(&client, conveyance()).await?;
    assert_eq!(
        EndpointStatus::Active,
        reply.status(),
        "{:?}",
        reply.outcome()
    );
    assert_eq!(vec![Verdict::Issued], harness.server.endpoint().verdicts());
    assert_eq!(
        vec![harness.door.client_thumbprint().to_owned()],
        harness.door.presented(),
        "the token request went to the alias, by the certificate"
    );
    Ok(())
}

/// A `token_endpoint` alias on a host the grant does not name is refused,
/// and nothing is sent to it, whether the grant names no host or another
/// one.
#[tokio::test]
async fn an_alias_on_a_host_the_grant_does_not_name_is_refused() -> TestResult {
    for named in [vec![], vec![AliasHost::parse("mtls.cdr-a.example.org")?]] {
        let harness = Harness::start().await?;
        alias_on_the_door(&harness)?;
        let grant = if named.is_empty() {
            harness.grant()?
        } else {
            harness.grant()?.with_mtls_alias_hosts(named.clone())?
        };
        let error = harness
            .provider(grant)?
            .credentials()
            .await
            .err()
            .ok_or("the alias is refused")?;
        let met = Error::source(&error).and_then(|source| source.downcast_ref::<Fapi2Error>());
        assert!(
            matches!(
                met,
                Some(Fapi2Error::Discovery(DiscoveryError::Endpoint(
                    EndpointError::OtherOrigin
                )))
            ),
            "{named:?}: {met:?}"
        );
        assert!(harness.server.endpoint().forms().is_empty(), "{named:?}");
        assert_eq!(
            0,
            harness.door.handshakes(),
            "{named:?}: nothing reaches it"
        );
    }
    Ok(())
}

/// A grant that does not use mutual TLS reads no alias, so it takes no
/// alias host.
#[test]
fn a_grant_without_mutual_tls_takes_no_alias_host() -> TestResult {
    let grant = Fapi2Grant::new(
        Issuer::parse("https://as.cdr-a.example.org")?,
        CLIENT_ID,
        (client_key()?, prover()?),
        (Some(Scope::parse(SCOPE)?), None),
    )?;
    assert!(matches!(
        grant.with_mtls_alias_hosts(vec![AliasHost::parse("mtls.cdr-a.example.org")?]),
        Err(Fapi2GrantError::AliasHostsUnused)
    ));
    Ok(())
}

/// `private_key_jwt` needs a client key, and a token exchange needs one to
/// sign its actor token with; a grant without one cannot be built.
#[test]
fn a_grant_without_the_key_it_signs_with_cannot_be_built() -> TestResult {
    let issuer = || Issuer::parse("https://as.cdr-a.example.org");
    let request = || -> Result<_, Box<dyn Error>> { Ok((Some(Scope::parse(SCOPE)?), None)) };
    let thumbprint = Thumbprint::of_certificate(b"synthetic certificate");
    let unsigned = Fapi2Grant::secured(
        issuer()?,
        CLIENT_ID,
        Fapi2Security {
            client_auth: ClientAuthentication::PrivateKeyJwt,
            sender: SenderConstraint::Certificate(thumbprint.clone()),
            client_key: None,
        },
        request()?,
    );
    assert!(matches!(unsigned, Err(Fapi2GrantError::NoClientKey)));

    let by_certificate = Fapi2Grant::secured(
        issuer()?,
        CLIENT_ID,
        Fapi2Security {
            client_auth: ClientAuthentication::Tls(TlsClientAuth::SelfSigned),
            sender: SenderConstraint::Dpop(prover()?),
            client_key: None,
        },
        request()?,
    )?
    .with_resource("https://cdr-a.example.org/openehr")?;
    assert!(by_certificate.dpop().is_some());
    assert!(by_certificate.uses_mutual_tls());
    assert!(matches!(
        by_certificate.with_token_exchange(),
        Err(Fapi2GrantError::NoActorKey)
    ));

    let signed = Fapi2Grant::secured(
        issuer()?,
        CLIENT_ID,
        Fapi2Security {
            client_auth: ClientAuthentication::Tls(TlsClientAuth::Pki),
            sender: SenderConstraint::Certificate(thumbprint),
            client_key: Some(client_key()?),
        },
        request()?,
    )?
    .with_resource("https://cdr-a.example.org/openehr")?
    .with_token_exchange()?;
    assert!(signed.client_key().is_some());
    assert!(signed.certificate().is_some());
    assert!(signed.dpop().is_none());
    Ok(())
}
