// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node clients whose onward credentials depend on whom each request is
//! made for: a token exchanged per verified caller (§13.1, N25, N26; RFC
//! 8693), obtained when the request is sent.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Client, Transport};

use crate::onward::exchange::SharedOnBehalf;

use super::{DispatchOptions, NodeClient, NodeClients, SetupError};

impl<T: Transport + Clone> NodeClient<T> {
    /// This client asking `source` for the onward credentials of each
    /// request on behalf of the principal that request conveys.
    ///
    /// It takes the place of any provider set with
    /// [`NodeClient::with_credentials_provider`].
    #[must_use]
    pub fn with_on_behalf(mut self, source: SharedOnBehalf) -> Self {
        self.on_behalf = Some(source);
        self
    }

    /// The ITS-REST client a request under `options` is sent through: the
    /// endpoint's own, or one whose credentials are obtained on behalf of
    /// the principal `options` conveys.
    pub(crate) fn client_for(&self, options: &DispatchOptions) -> Cow<'_, Client<T>> {
        match &self.on_behalf {
            None => Cow::Borrowed(&self.client),
            Some(source) => Cow::Owned(self.client.clone().with_credentials_provider(
                source.provider(&options.conveyance, &options.withheld),
            )),
        }
    }
}

impl<T: Transport + Clone> NodeClients<T> {
    /// These clients, each of an endpoint `on_behalf` names asking that
    /// source for the onward credentials of each request
    /// ([`NodeClient::with_on_behalf`]).
    ///
    /// # Errors
    ///
    /// Returns [`SetupError::UnknownEndpoint`] when `on_behalf` names an
    /// endpoint these clients do not hold.
    pub fn with_on_behalf(
        mut self,
        on_behalf: &BTreeMap<EndpointId, SharedOnBehalf>,
    ) -> Result<Self, SetupError> {
        for (endpoint, source) in on_behalf {
            let client =
                self.clients
                    .remove(endpoint)
                    .ok_or_else(|| SetupError::UnknownEndpoint {
                        endpoint: endpoint.clone(),
                    })?;
            self.clients
                .insert(endpoint.clone(), client.with_on_behalf(Arc::clone(source)));
        }
        Ok(self)
    }
}
