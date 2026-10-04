// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials each node client sends: a configured bearer token
//! or basic credentials, a provider that obtains a token with the OAuth 2.0
//! client-credentials grant, or one that exchanges each verified caller's
//! token (RFC 8693), each authenticated by a signed JWT client assertion
//! (§13.1, N25), and the `DPoP` proofs of a grant whose tokens are bound to
//! a key (RFC 9449).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::dpop::DpopTransport;
use ferrofed_engine::onward::exchange::{Exchange, SharedOnBehalf};
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{Grant, GrantKind, SystemClock};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{Scheme, Settings};
use crate::federation::{FederationError, NodeTransport};

/// What the node clients of one federation send each node to authenticate.
#[derive(Debug)]
pub(crate) struct Onward {
    /// The engine every node request and token request is sent through,
    /// with a `DPoP` proof on each request to a grant's node and token
    /// endpoint where the grant binds its tokens (RFC 9449).
    pub(crate) transport: NodeTransport,
    /// The credentials of each endpoint that sends the same to every
    /// caller: a configured secret or a client-credentials grant.
    pub(crate) credentials: BTreeMap<EndpointId, SharedCredentials>,
    /// The credentials of each endpoint that exchanges each verified
    /// caller's token (RFC 8693).
    pub(crate) on_behalf: BTreeMap<EndpointId, SharedOnBehalf>,
}

/// The onward credentials of each endpoint that has a `[credentials]`
/// section, as the node clients send them over `engine`.
///
/// A bearer token and basic credentials are sent as configured. An OAuth 2.0
/// grant becomes a provider that obtains its token with an assertion the
/// `[signing]` key signs, waiting at most the per-node timeout for the token
/// endpoint (§13.1, N25): one token for every caller under the
/// client-credentials grant, one per verified caller under token exchange. A
/// grant with a `DPoP` key proves every request to its node, whose URL
/// `snapshot` holds, and to its token endpoint.
///
/// # Errors
///
/// Returns [`FederationError::Grant`] for a grant with no `[signing]` key to
/// sign its assertion.
pub(crate) fn onward(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    engine: ReqwestTransport,
) -> Result<Onward, FederationError> {
    let mut transport = DpopTransport::new(engine);
    for (endpoint, scheme) in &settings.credentials {
        if let Scheme::OAuth2(grant) = scheme
            && let Some(prover) = grant.dpop()
        {
            transport = transport.with_route(grant.token_endpoint().clone(), Arc::clone(prover));
            // NOTE: no specification governs this: our own design; an endpoint the
            // registry lacks is legitimately absent here, and the client build refuses it.
            if let Some(declared) = snapshot.endpoint(endpoint) {
                transport = transport.with_route(declared.url().clone(), Arc::clone(prover));
            }
        }
    }
    let mut credentials = BTreeMap::new();
    let mut on_behalf = BTreeMap::new();
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
        };
        let Some(signing) = &settings.signing else {
            return Err(FederationError::Grant {
                section: format!("credentials.{endpoint}.oauth2"),
            });
        };
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
                transport.clone(),
                Arc::new(SystemClock),
            ));
            on_behalf.insert(endpoint.clone(), exchange);
        } else {
            let provider: SharedCredentials = Arc::new(ClientCredentials::new(
                endpoint.clone(),
                Grant::clone(grant),
                Arc::clone(&signing.keys),
                timing,
                transport.clone(),
                Arc::new(SystemClock),
            ));
            credentials.insert(endpoint.clone(), provider);
        }
    }
    Ok(Onward {
        transport,
        credentials,
        on_behalf,
    })
}
