// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials each node client sends: a configured bearer token
//! or basic credentials, a provider that obtains a token with the OAuth 2.0
//! client-credentials grant, or one that exchanges each verified caller's
//! token (RFC 8693), each authenticated by a signed JWT client assertion
//! (§13.1, N25), a provider that obtains a token with the Nuts grant of
//! Annex B §B.4, one that obtains it under the FAPI 2.0 Security Profile, as
//! the track of Annex B §B.4a does, and the `DPoP` key of a grant whose
//! tokens are bound to one (RFC 9449).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::dpop::Prover;
use ferrofed_engine::onward::exchange::{Exchange, SharedOnBehalf};
use ferrofed_engine::onward::fapi2::{Fapi2Credentials, Fapi2Exchange, Fapi2Grant};
use ferrofed_engine::onward::nuts::{NutsCredentials, NutsGrant};
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{Grant, GrantKind, SystemClock};
use ferrofed_registry::id::EndpointId;
use nl_generic_functions::nuts_auth::NutsClient;
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{Scheme, Settings};
use crate::federation::error::FederationError;

/// The HTTP engine every request to a node and to its token endpoint is
/// sent through: the `reqwest` engine.
pub(crate) type NodeTransport = ReqwestTransport;

/// What the node clients of one federation send each node to authenticate.
#[derive(Debug)]
pub(crate) struct Onward {
    /// The engine every node request and token request is sent through.
    pub(crate) transport: NodeTransport,
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
/// # Errors
///
/// Returns [`FederationError::Grant`] for a grant with no `[signing]` key to
/// sign its assertion.
pub(crate) fn onward(
    settings: &Settings,
    engine: ReqwestTransport,
) -> Result<Onward, FederationError> {
    let mut credentials = BTreeMap::new();
    let mut on_behalf = BTreeMap::new();
    let mut dpop = BTreeMap::new();
    for (endpoint, scheme) in &settings.credentials {
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
                dpop.insert(endpoint.clone(), Arc::clone(grant.dpop()));
                match fapi2(settings, endpoint, grant, &engine)? {
                    Provided::Same(provider) => {
                        credentials.insert(endpoint.clone(), provider);
                    }
                    Provided::PerCaller(exchange) => {
                        on_behalf.insert(endpoint.clone(), exchange);
                    }
                }
                continue;
            }
            Scheme::Nuts(grant) => {
                dpop.insert(endpoint.clone(), Arc::clone(grant.dpop()));
                let provider: SharedCredentials = Arc::new(NutsCredentials::new(
                    endpoint.clone(),
                    NutsGrant::clone(grant),
                    nuts_client(endpoint)?,
                    settings.federation.budget.per_node(),
                    Arc::new(SystemClock),
                ));
                credentials.insert(endpoint.clone(), provider);
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
        credentials,
        on_behalf,
        dpop,
    })
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

/// The client the Nuts grant of `endpoint` sends its requests through.
///
/// It follows no redirect, since the token request carries the gateway's
/// credentials (no specification governs the client: our own design).
fn nuts_client(endpoint: &EndpointId) -> Result<NutsClient, FederationError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map(NutsClient::new)
        .map_err(|source| FederationError::NutsClient {
            section: format!("credentials.{endpoint}.nuts"),
            source,
        })
}
