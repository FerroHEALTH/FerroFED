// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials each node client sends: a configured bearer token
//! or basic credentials, or a provider that obtains a token with the OAuth
//! 2.0 client-credentials grant and a signed JWT client assertion (§13.1,
//! N25).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_engine::dispatch::SharedCredentials;
use ferrofed_engine::onward::provider::ClientCredentials;
use ferrofed_engine::onward::{Grant, SystemClock};
use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{Scheme, Settings};
use crate::federation::FederationError;

/// The onward credentials of each endpoint that has a `[credentials]`
/// section, as the node clients send them.
///
/// A bearer token and basic credentials are sent as configured. An OAuth 2.0
/// grant becomes a provider that obtains its token over `transport` with an
/// assertion the `[signing]` key signs, waiting at most the per-node timeout
/// for the token endpoint (§13.1, N25).
///
/// # Errors
///
/// Returns [`FederationError::Grant`] for a grant with no
/// `[signing]` key to sign its assertion.
pub(crate) fn onward_credentials(
    settings: &Settings,
    transport: &ReqwestTransport,
) -> Result<BTreeMap<EndpointId, SharedCredentials>, FederationError> {
    let mut credentials = BTreeMap::new();
    for (endpoint, scheme) in &settings.credentials {
        let shared: SharedCredentials = match scheme {
            Scheme::Bearer(token) => Arc::new(Credentials::Bearer(token.to_secret_string())),
            Scheme::Basic { user, password } => Arc::new(Credentials::Basic {
                user: user.clone(),
                password: password.to_secret_string(),
            }),
            Scheme::OAuth2(grant) => {
                let Some(signing) = &settings.signing else {
                    return Err(FederationError::Grant {
                        section: format!("credentials.{endpoint}.oauth2"),
                    });
                };
                Arc::new(ClientCredentials::new(
                    endpoint.clone(),
                    Grant::clone(grant),
                    Arc::clone(&signing.keys),
                    (
                        signing.assertion_lifetime,
                        settings.federation.budget.per_node(),
                    ),
                    transport.clone(),
                    Arc::new(SystemClock),
                ))
            }
        };
        credentials.insert(endpoint.clone(), shared);
    }
    Ok(credentials)
}
