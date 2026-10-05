// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node clients whose onward tokens are bound to a key of the gateway's
//! with `DPoP` (RFC 9449): each call asks its endpoint's [`NodeProver`] for
//! the proof of every request it sends under a bound token, and answers a
//! node's demanded nonce once (§9).
//!
//! The `openehr-its` client answers that nonce by sending the request once
//! more, and fails `DeadlineElapsed` or `DpopProof` when it cannot. Either
//! error says whether an earlier send of the call went out (its `sent`), so
//! a call whose first request left and was answered is read as a request
//! that left ([`sent_before`]).

use std::collections::BTreeMap;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::client::{ClientError, Transport};

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

/// Whether a send of the call went out before `error` ended it: a deadline
/// that passed, or a proof that could not be made, after an earlier send,
/// such as before the re-send answering a node's nonce challenge (RFC 9449
/// §9).
///
/// The client says so itself in the error's `sent`, so such a call is read as
/// a request that left, never as one never sent.
pub(crate) fn sent_before(error: &ClientError) -> bool {
    match error {
        // NOTE: openehr-its 0.0.84 ClientError::DeadlineElapsed and ClientError::DpopProof
        // (docs.rs): `sent` says whether an earlier send of the call went out.
        ClientError::DeadlineElapsed { sent, .. } | ClientError::DpopProof { sent, .. } => *sent,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use ferrofed_registry::snapshot::RegistrySnapshot;
    use openehr_federation::outcome::{ErrorDetail, Outcome};
    use openehr_federation::status::EndpointStatus;
    use openehr_its::rest::client::{
        Credentials, CredentialsError, DpopProofRequest, DpopProver, Transport, TransportError,
    };

    use crate::dispatch::reported::{UNPROVEN, UNPROVEN_AGAIN};
    use crate::dispatch::{Contact, DispatchOptions, NodeClient, NodeQuery, NodeReply};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A query scoped to a synthetic `ehr_id`.
    const NODE_AQL: &str = "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '7d44b88c-4199-4bad-97dc-d78268e01398'";

    /// A node that answers every request with a `DPoP` nonce challenge
    /// (RFC 9449 §9).
    #[derive(Debug, Clone, Default)]
    struct Challenging {
        sends: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl Transport for Challenging {
        async fn send(
            &self,
            _request: http::Request<Vec<u8>>,
        ) -> Result<http::Response<Vec<u8>>, TransportError> {
            self.sends.fetch_add(1, Ordering::SeqCst);
            http::Response::builder()
                .status(http::StatusCode::UNAUTHORIZED)
                .header(
                    http::header::WWW_AUTHENTICATE,
                    "DPoP error=\"use_dpop_nonce\", error_description=\"a nonce is required\"",
                )
                .header("DPoP-Nonce", "synthetic-rs-nonce-9")
                .body(Vec::new())
                .map_err(|source| TransportError::Send {
                    source: Box::new(source),
                })
        }
    }

    /// The reason a proof could not be made.
    #[derive(Debug, thiserror::Error)]
    #[error("the synthetic prover makes no proof")]
    struct NoProof;

    /// A prover that makes its first `proofs` proofs and fails every later
    /// one.
    #[derive(Debug)]
    struct Exhausted {
        proofs: usize,
        asked: AtomicUsize,
    }

    #[async_trait::async_trait]
    impl DpopProver for Exhausted {
        async fn proof(&self, _request: &DpopProofRequest<'_>) -> Result<String, CredentialsError> {
            if self.asked.fetch_add(1, Ordering::SeqCst) < self.proofs {
                Ok("synthetic.dpop.proof".to_owned())
            } else {
                Err(CredentialsError::new(NoProof))
            }
        }

        fn nonce(&self, _nonce: &str) {}
    }

    /// The reply of a node that challenges every request, asked through a
    /// client whose prover makes `proofs` proofs, and the sends it saw.
    async fn asked(proofs: usize) -> Result<(NodeReply, usize), Box<dyn std::error::Error>> {
        let snapshot = RegistrySnapshot::from_toml_str(
            "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n\n[[endpoint]]\nid = \"node-a-pub\"\nnode = \"node-a\"\nurl = \"https://cdr-a.example.org/openehr\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n",
        )?;
        let endpoint = snapshot.endpoints().next().ok_or("one endpoint")?;
        let node = Challenging::default();
        let mut client = NodeClient::new(endpoint, node.clone())?
            .with_credentials_provider(Arc::new(Credentials::dpop("synthetic-bound-token-9")));
        client.client = client.client.with_dpop_prover(Exhausted {
            proofs,
            asked: AtomicUsize::new(0),
        });
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(5))
            .ok_or("the deadline is past the platform clock")?;
        let options = DispatchOptions::new(deadline, crate::conveyance::tests::conveyance());
        let reply = client.query(&NodeQuery::new(NODE_AQL), &options).await?;
        Ok((reply, node.sends.load(Ordering::SeqCst)))
    }

    /// The `error` text of a `node-error` `reply`.
    fn node_error(reply: &NodeReply) -> Result<String, Box<dyn std::error::Error>> {
        match reply.outcome() {
            Outcome::NodeError {
                error: ErrorDetail::Text(text),
                ..
            } => Ok(text),
            other => Err(format!("{other:?}").into()),
        }
    }

    /// A node that answered the first request with a nonce challenge, whose
    /// re-send no proof could be made for, is a `node-error` of a node that
    /// was asked: the client's `DpopProof` says an earlier send went out
    /// (§11.1, RFC 9449 §9).
    // conformance: CP-17
    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_proof_that_fails_on_the_nonce_resend_is_a_node_error_of_a_node_that_was_asked()
    -> TestResult {
        let (reply, sends) = asked(1).await?;
        assert_eq!(1, sends, "the first request left, and no re-send");
        assert_eq!(
            EndpointStatus::NodeError,
            reply.status(),
            "{:?}",
            reply.outcome()
        );
        assert_eq!(Contact::Silent, reply.contact(), "a node that was asked");
        assert!(reply.contact().sent());
        assert_eq!(UNPROVEN_AGAIN, node_error(&reply)?);
        Ok(())
    }

    /// A first request no proof could be made for never left: a
    /// `node-error` with nothing sent (RFC 9449 §4).
    // conformance: CP-17
    #[tokio::test]
    #[expect(
        clippy::panic_in_result_fn,
        reason = "a test asserts, and returns its setup errors"
    )]
    async fn a_proof_that_fails_on_the_first_send_is_a_node_error_with_nothing_sent() -> TestResult
    {
        let (reply, sends) = asked(0).await?;
        assert_eq!(0, sends, "no request left");
        assert_eq!(
            EndpointStatus::NodeError,
            reply.status(),
            "{:?}",
            reply.outcome()
        );
        assert_eq!(Contact::Unsent, reply.contact());
        assert!(!reply.contact().sent());
        assert_eq!(UNPROVEN, node_error(&reply)?);
        Ok(())
    }
}
