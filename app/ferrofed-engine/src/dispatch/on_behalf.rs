// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The onward credentials of the node clients: a provider per endpoint, or
//! credentials that depend on whom each request is made for, a token
//! exchanged per verified caller (§13.1, N25, N26; RFC 8693) and obtained
//! when the request is sent.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{Client, ClientError, Transport};
use url::Url;

use crate::onward::grant::exchange::SharedOnBehalf;

use super::{DispatchOptions, NodeClient, NodeClients, SetupError, SharedCredentials};

impl<T: Transport + Clone> NodeClient<T> {
    /// This client asking `provider` for the onward credentials of every
    /// request.
    #[must_use]
    pub fn with_credentials_provider(mut self, provider: SharedCredentials) -> Self {
        self.client = self.client.with_credentials_provider(Arc::clone(&provider));
        self.credentials = Some(provider);
        self
    }

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

    /// The ITS-REST client a request under `options` is sent through.
    ///
    /// The client is the endpoint's own, or one whose credentials are
    /// obtained on behalf of the principal `options` conveys; under `DPoP`
    /// it proves its requests with the endpoint's
    /// [`NodeProver`](crate::onward::dpop::NodeProver).
    pub(crate) fn client_for(&self, options: &DispatchOptions) -> Cow<'_, Client<T>> {
        let client = match &self.on_behalf {
            None => Cow::Borrowed(&self.client),
            Some(source) => Cow::Owned(self.client.clone().with_credentials_provider(
                source.provider(&options.conveyance, &options.withheld),
            )),
        };
        match &self.dpop {
            None => client,
            Some(prover) => Cow::Owned(client.into_owned().with_dpop_prover(prover.clone())),
        }
    }

    /// A client rooted at `base`, another interface of the same node,
    /// sending as [`NodeClient::client_for`] does: over the endpoint's
    /// transport, with its retry budget, its onward credentials, obtained on
    /// behalf of the principal `options` conveys where they are, and its
    /// `DPoP` prover.
    ///
    /// No specification governs reaching a node's second endpoint with the
    /// first one's credentials: our own design.
    pub(crate) fn client_at(
        &self,
        base: Url,
        options: &DispatchOptions,
    ) -> Result<Client<T>, ClientError> {
        let mut client =
            Client::new(self.client.transport().clone(), base)?.with_retry(self.client.retry());
        if let Some(source) = &self.on_behalf {
            client = client
                .with_credentials_provider(source.provider(&options.conveyance, &options.withheld));
        } else if let Some(provider) = &self.credentials {
            client = client.with_credentials_provider(Arc::clone(provider));
        }
        if let Some(prover) = &self.dpop {
            client = client.with_dpop_prover(prover.clone());
        }
        Ok(client)
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

    /// Whether any client obtains its credentials on behalf of each
    /// request's principal, so a caller's verified token is needed (RFC 8693
    /// §2.1).
    #[must_use]
    pub fn exchanging(&self) -> bool {
        self.clients
            .values()
            .any(|client| client.on_behalf.is_some())
    }
}
