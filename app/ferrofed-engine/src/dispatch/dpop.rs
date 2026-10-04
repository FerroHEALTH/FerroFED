// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node clients whose onward tokens are bound to a key of the gateway's
//! with `DPoP` (RFC 9449): each asks its endpoint's
//! [`NodeProver`] for the proof of every request it sends under a bound
//! token, and answers a node's demanded nonce once (§9).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::Transport;

use crate::onward::dpop::{NodeProver, Prover};

use super::{NodeClient, NodeClients, SetupError};

impl<T: Transport + Clone> NodeClient<T> {
    /// This client proving every request it sends under a `DPoP`-bound token
    /// with `prover`'s key (RFC 9449 §4.2, §7.1).
    ///
    /// The proof binds the request's method, its URL and the token's hash. A
    /// node's `401` with a `DPoP` challenge naming `use_dpop_nonce` is
    /// answered by sending the request once more with the nonce (§9).
    #[must_use]
    pub fn with_dpop(mut self, prover: &Arc<Prover>) -> Self {
        let base = self.client.base().clone();
        self.client = self
            .client
            .with_dpop_prover(NodeProver::new(Arc::clone(prover), base));
        self
    }
}

impl<T: Transport + Clone> NodeClients<T> {
    /// These clients, each of an endpoint `provers` names proving its
    /// requests with that key ([`NodeClient::with_dpop`]).
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::UnknownEndpoint`] when `provers` names an
    /// endpoint these clients do not hold.
    pub fn with_dpop(
        mut self,
        provers: &BTreeMap<EndpointId, Arc<Prover>>,
    ) -> Result<Self, SetupError> {
        for (endpoint, prover) in provers {
            let client =
                self.clients
                    .remove(endpoint)
                    .ok_or_else(|| SetupError::UnknownEndpoint {
                        endpoint: endpoint.clone(),
                    })?;
            self.clients
                .insert(endpoint.clone(), client.with_dpop(prover));
        }
        Ok(self)
    }
}
