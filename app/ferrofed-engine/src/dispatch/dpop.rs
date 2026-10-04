// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node clients whose onward tokens are bound to a key of the gateway's
//! with `DPoP` (RFC 9449): each call asks its endpoint's [`NodeProver`] for
//! the proof of every request it sends under a bound token, and answers a
//! node's demanded nonce once (§9).
//!
//! The `openehr-its` client answers that nonce by sending the request once
//! more, and fails `DeadlineElapsed` or `DpopProof` when it cannot. Either
//! error reads as a request never sent, though here the first request left
//! and the node answered it. Each call's prover records what it saw
//! ([`Sent`]), so such a call is read as a request that left.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{
    ClientError, CredentialsError, DpopProofRequest, DpopProver, Transport,
};

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
        self.dpop = Some(NodeProver::new(Arc::clone(prover), base));
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

/// What one call's prover saw of the call's own sends: whether a request of
/// the call evidently left the gateway.
///
/// A second proof for the call, or a nonce the node sent, proves that a
/// request left and was answered: the client asks for another proof only
/// to answer a nonce challenge, and hears a nonce only in an answer (RFC
/// 9449 §9). No specification governs the reading: our own design.
#[derive(Debug, Clone, Default)]
pub(crate) struct Sent(Arc<AtomicBool>);

impl Sent {
    /// Whether a request of the call evidently left.
    pub(crate) fn evident(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn record(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether `error`, which reads as a request never sent, ended a call
    /// whose request evidently left: a deadline that passed before the
    /// nonce re-send, or a re-send no proof could be made for.
    pub(crate) fn contradicts(&self, error: &ClientError) -> bool {
        self.evident()
            && matches!(
                error,
                ClientError::DeadlineElapsed { .. } | ClientError::DpopProof { .. }
            )
    }
}

/// The [`NodeProver`] of one call, recording in [`Sent`] what it saw.
#[derive(Debug)]
pub(super) struct Witnessed {
    prover: NodeProver,
    proofs: AtomicUsize,
    sent: Sent,
}

impl Witnessed {
    /// The proofs of `prover` for one call, recorded in `sent`.
    pub(super) fn new(prover: NodeProver, sent: Sent) -> Self {
        Self {
            prover,
            proofs: AtomicUsize::new(0),
            sent,
        }
    }
}

#[async_trait::async_trait]
impl DpopProver for Witnessed {
    async fn proof(&self, request: &DpopProofRequest<'_>) -> Result<String, CredentialsError> {
        if self.proofs.fetch_add(1, Ordering::SeqCst) > 0 {
            self.sent.record();
        }
        self.prover.proof(request).await
    }

    fn nonce(&self, nonce: &str) {
        self.sent.record();
        self.prover.nonce(nonce);
    }
}
