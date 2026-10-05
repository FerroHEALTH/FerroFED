// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node clients of endpoints reached over a transport of their own: one
//! that presents the endpoint's TLS client certificate to the node and to its
//! authorization server, so a token bound to that certificate travels only
//! over connections that present it (RFC 8705 §3).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_its::rest::client::Transport;

use super::{NodeClient, NodeClients, SetupError, SharedCredentials};

impl<T: Transport + Clone> NodeClients<T> {
    /// A client for every endpoint of `snapshot`, as
    /// [`NodeClients::from_snapshot`] builds them, except that an endpoint
    /// `own` names is reached over its own transport.
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::BaseUrl`] when an endpoint's base URL cannot carry
    /// a path, and [`SetupError::UnknownEndpoint`] when `credentials` or `own`
    /// names an endpoint the snapshot does not hold.
    pub fn from_snapshot_over(
        snapshot: &RegistrySnapshot,
        (transport, own): (&T, &BTreeMap<EndpointId, T>),
        credentials: &BTreeMap<EndpointId, SharedCredentials>,
    ) -> Result<Self, SetupError> {
        if let Some(stray) = credentials
            .keys()
            .chain(own.keys())
            .find(|endpoint| snapshot.endpoint(endpoint).is_none())
        {
            return Err(SetupError::UnknownEndpoint {
                endpoint: stray.clone(),
            });
        }
        let mut clients = BTreeMap::new();
        for endpoint in snapshot.endpoints() {
            let engine = own.get(endpoint.id()).unwrap_or(transport);
            let mut client = NodeClient::new(endpoint, engine.clone())?;
            if let Some(provider) = credentials.get(endpoint.id()) {
                client = client.with_credentials_provider(Arc::clone(provider));
            }
            clients.insert(endpoint.id().clone(), client);
        }
        Ok(Self { clients })
    }
}
