// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials each node client sends: a configured bearer token
//! or basic credentials, a provider that obtains a token with the OAuth 2.0
//! client-credentials grant, or one that exchanges each verified caller's
//! token (RFC 8693), each authenticated by a signed JWT client assertion
//! (§13.1, N25), one that obtains it under the FAPI 2.0 Security Profile, as
//! the track of Annex B §B.4a does, the provider a binding's grant builds,
//! such as the Nuts grant of Annex B §B.4, and the `DPoP` key of a grant
//! whose tokens are bound to one (RFC 9449).
//!
//! A node whose `[credentials]` section names TLS material is reached over a
//! transport of its own, which presents the gateway's client certificate and
//! trusts the section's roots, and its token endpoint over the same
//! transport: a token bound to that certificate travels only over
//! connections that present it (RFC 8705 §3). Such a node is reached over
//! `https` alone.

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::exchange::{Exchange, SharedOnBehalf};
use ferrofed_engine::onward::fapi2::{Fapi2Credentials, Fapi2Exchange, Fapi2Grant};
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{Grant, GrantKind, SystemClock};
use ferrofed_identity::fhir::{self, Authentication};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{Scheme, Settings};
use crate::config::tls::TlsSettings;
use crate::federation::error::FederationError;

/// The HTTP engine every request to a node and to its token endpoint is
/// sent through: the `reqwest` engine.
pub(crate) type NodeTransport = ReqwestTransport;

/// What the node clients of one federation send each node to authenticate.
#[derive(Debug)]
pub(crate) struct Onward {
    /// The engine every node request and token request is sent through,
    /// unless its endpoint has one of its own.
    pub(crate) transport: NodeTransport,
    /// The engine of each endpoint whose section names TLS material, which
    /// its node requests and its token requests are sent through.
    pub(crate) transports: BTreeMap<EndpointId, NodeTransport>,
    /// The credentials of each endpoint that sends the same to every
    /// caller: a configured secret or a client-credentials grant.
    pub(crate) credentials: BTreeMap<EndpointId, SharedCredentials>,
    /// The credentials of each endpoint that exchanges each verified
    /// caller's token (RFC 8693).
    pub(crate) on_behalf: BTreeMap<EndpointId, SharedOnBehalf>,
    /// The key each endpoint whose grant binds its tokens with `DPoP`
    /// proves its node requests with (RFC 9449).
    pub(crate) dpop: BTreeMap<EndpointId, Arc<Prover>>,
}

/// The onward credentials of each endpoint that has a `[credentials]`
/// section, as the node clients send them over `engine`.
///
/// A bearer token and basic credentials are sent as configured. An OAuth 2.0
/// grant becomes a provider that obtains its token with an assertion the
/// `[signing]` key signs, waiting at most the per-node timeout for the token
/// endpoint (§13.1, N25): one token for every caller under the
/// client-credentials grant, one per verified caller under token exchange. A
/// grant with a `DPoP` key proves every request to its token endpoint, and
/// its node's client proves every request to the node with the same key.
///
/// An endpoint with TLS material of its own is reached, at its node and its
/// token endpoint, over an engine built with that material, its timeout the
/// overall budget as `engine`'s is.
///
/// # Errors
///
/// Returns [`FederationError::Grant`] for a grant with no `[signing]` key to
/// sign its assertion, [`FederationError::ClientCertificateOverHttp`] for a
/// node `registry` declares at an `http` URL that would be presented a client
/// certificate, and [`FederationError::Tls`] and
/// [`FederationError::NodeTransport`] for TLS material no engine can be
/// built with.
pub(crate) fn onward(
    settings: &Settings,
    engine: ReqwestTransport,
    registry: &RegistrySnapshot,
) -> Result<Onward, FederationError> {
    let mut credentials = BTreeMap::new();
    let mut on_behalf = BTreeMap::new();
    let mut dpop = BTreeMap::new();
    let transports = transports(settings, registry)?;
    for (endpoint, scheme) in &settings.credentials {
        let engine = transports.get(endpoint).unwrap_or(&engine);
        let grant = match scheme {
            Scheme::Bearer(token) => {
                let shared: SharedCredentials =
                    Arc::new(Credentials::Bearer(token.to_secret_string()));
                credentials.insert(endpoint.clone(), shared);
                continue;
            }
            Scheme::Basic { user, password } => {
                let shared: SharedCredentials = Arc::new(Credentials::Basic {
                    user: user.clone(),
                    password: password.to_secret_string(),
                });
                credentials.insert(endpoint.clone(), shared);
                continue;
            }
            Scheme::OAuth2(grant) => grant,
            Scheme::Fapi2(grant) => {
                if let Some(prover) = grant.dpop() {
                    dpop.insert(endpoint.clone(), Arc::clone(prover));
                }
                match fapi2(settings, endpoint, grant, engine)? {
                    Provided::Same(provider) => {
                        credentials.insert(endpoint.clone(), provider);
                    }
                    Provided::PerCaller(exchange) => {
                        on_behalf.insert(endpoint.clone(), exchange);
                    }
                }
                continue;
            }
            Scheme::Binding(grant) => {
                let provided = grant.provide(endpoint, settings)?;
                if let Some(prover) = provided.dpop {
                    dpop.insert(endpoint.clone(), prover);
                }
                credentials.insert(endpoint.clone(), provided.credentials);
                continue;
            }
        };
        let Some(signing) = &settings.signing else {
            return Err(FederationError::Grant {
                section: format!("credentials.{endpoint}.oauth2"),
            });
        };
        if let Some(prover) = grant.dpop() {
            dpop.insert(endpoint.clone(), Arc::clone(prover));
        }
        let timing = (
            signing.assertion_lifetime,
            settings.federation.budget.per_node(),
        );
        if grant.kind() == GrantKind::TokenExchange {
            let exchange: SharedOnBehalf = Arc::new(Exchange::new(
                endpoint.clone(),
                Grant::clone(grant),
                Arc::clone(&signing.keys),
                timing,
                engine.clone(),
                Arc::new(SystemClock),
            ));
            on_behalf.insert(endpoint.clone(), exchange);
        } else {
            let provider: SharedCredentials = Arc::new(ClientCredentials::new(
                endpoint.clone(),
                Grant::clone(grant),
                Arc::clone(&signing.keys),
                timing,
                engine.clone(),
                Arc::new(SystemClock),
            ));
            credentials.insert(endpoint.clone(), provider);
        }
    }
    Ok(Onward {
        transport: engine,
        transports,
        credentials,
        on_behalf,
        dpop,
    })
}

/// The engine of each endpoint whose section names TLS material, built with
/// it, refusing one that would present a client certificate to a node
/// `registry` declares at a URL other than `https`.
fn transports(
    settings: &Settings,
    registry: &RegistrySnapshot,
) -> Result<BTreeMap<EndpointId, NodeTransport>, FederationError> {
    let mut transports = BTreeMap::new();
    for (endpoint, tls) in &settings.onward_tls {
        // NOTE: no specification governs this: our own design; an endpoint the
        // registry lacks is legitimately absent here, and the client build refuses it.
        if tls.client_identity.is_some()
            && let Some(declared) = registry.endpoint(endpoint)
            && !crate::config::transport::client_certificate(declared.url().as_str())
        {
            return Err(FederationError::ClientCertificateOverHttp {
                endpoint: endpoint.clone(),
            });
        }
        transports.insert(endpoint.clone(), transport(settings, endpoint, tls)?);
    }
    Ok(transports)
}

/// The engine of `endpoint`, presenting and trusting `tls`, with the
/// overall budget as its timeout, as the shared engine has.
fn transport(
    settings: &Settings,
    endpoint: &EndpointId,
    tls: &TlsSettings,
) -> Result<NodeTransport, FederationError> {
    let material = crate::service::tls_of(&format!("credentials.{endpoint}"), tls)
        .map_err(FederationError::Tls)?;
    let failed =
        |source: Box<dyn std::error::Error + Send + Sync>| FederationError::NodeTransport {
            endpoint: endpoint.clone(),
            source,
        };
    let builder = fhir::http_client_builder(&Authentication::None, &material)
        .map_err(|source| failed(Box::new(source)))?;
    ReqwestTransport::with_builder_timeout(builder, settings.federation.budget.overall())
        .map_err(|source| failed(Box::new(source)))
}

/// What one endpoint's grant provides its node client.
enum Provided {
    /// The same credentials for every caller.
    Same(SharedCredentials),
    /// Credentials per verified caller (RFC 8693).
    PerCaller(SharedOnBehalf),
}

/// The provider of `endpoint`'s FAPI 2.0 `grant`, discovering its
/// authorization server over `engine` and signing each assertion valid for
/// the `[signing]` lifetime.
///
/// # Errors
///
/// Returns [`FederationError::Grant`] without `[signing]`.
fn fapi2(
    settings: &Settings,
    endpoint: &EndpointId,
    grant: &Fapi2Grant,
    engine: &ReqwestTransport,
) -> Result<Provided, FederationError> {
    let Some(signing) = &settings.signing else {
        return Err(FederationError::Grant {
            section: format!("credentials.{endpoint}.fapi2"),
        });
    };
    let timing = (
        signing.assertion_lifetime,
        settings.federation.budget.per_node(),
    );
    Ok(if grant.kind() == GrantKind::TokenExchange {
        Provided::PerCaller(Arc::new(Fapi2Exchange::new(
            endpoint.clone(),
            grant.clone(),
            timing,
            engine.clone(),
            Arc::new(SystemClock),
        )))
    } else {
        Provided::Same(Arc::new(Fapi2Credentials::new(
            endpoint.clone(),
            grant.clone(),
            timing,
            engine.clone(),
            Arc::new(SystemClock),
        )))
    })
}
