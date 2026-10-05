// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query a node is sent: the generated `query_execute_adhoc_query_body`
//! (`POST {base}/v1/query/aql`), through the outbound gate, inside its
//! `node_request` span ([`crate::trace_context`]), and its answer
//! classified.

use std::time::Instant;

use openehr_its::rest::client::{CallOptions, Transport};
use openehr_its::rest::generated::query::QueryExecuteAdhocQueryBodyParams;
use openehr_its::rest::generated::query::client::QueryClient;

use super::cap::Slot;
use super::{
    Contact, DispatchError, DispatchOptions, NodeClient, NodeQuery, NodeReply, cap, classify,
};
use crate::trace_context;

/// The ITS-REST operation every query to a node is sent through.
const OPERATION: &str = "query_execute_adhoc_query_body";

impl<T: Transport + Clone> NodeClient<T> {
    /// Sends `query` to the node and classifies the answer.
    ///
    /// # Errors
    ///
    /// Returns [`DispatchError`] when the request could not leave the gateway:
    /// a withheld identifier in the request ([`DispatchError::Withheld`], with
    /// nothing sent), no credential, or a body the client runtime refuses. Every answer, and every failure to reach the node, is a
    /// [`NodeReply`].
    pub async fn query(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<NodeReply, DispatchError> {
        trace_context::node_request(
            &self.endpoint,
            OPERATION,
            self.query_once(query, options),
            |reply| match reply {
                Ok(reply) => (Some(reply.contact()), Some(reply.status())),
                Err(_unsent) => (Some(Contact::Unsent), None),
            },
        )
        .await
    }

    /// Sends `query` as [`NodeClient::query`] does, and abandons it when the
    /// overall budget `until` runs out after it left, returning `None`.
    ///
    /// A query still waiting for a slot of the endpoint's in-flight cap is
    /// never abandoned: the deadline of `options`, which a caller sets no
    /// later than `until`, ends that wait with a capped reply and nothing
    /// sent (§11.1, §11.5, N38).
    ///
    /// # Errors
    ///
    /// As [`NodeClient::query`].
    pub(crate) async fn query_until(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
        until: tokio::time::Instant,
    ) -> Option<Result<NodeReply, DispatchError>> {
        let budgeted = async {
            let call = match self.prepared(query, options) {
                Ok(call) => call,
                Err(unsent) => return Some(Err(unsent)),
            };
            let started = Instant::now();
            let Ok(slot) = self.slot(options.deadline()).await else {
                return Some(Ok(cap::capped_reply(classify::elapsed_ms(started))));
            };
            // NOTE: §11.5, N38: an elapsed budget is the abandonment `None` reports; tokio's
            // timeout_at (docs.rs) polls the send first, so a query that never left is not abandoned.
            tokio::time::timeout_at(
                until,
                self.sent_query(query, options, call, (started, slot)),
            )
            .await
            .ok()
        };
        trace_context::node_request(&self.endpoint, OPERATION, budgeted, |reply| match reply {
            Some(Ok(reply)) => (Some(reply.contact()), Some(reply.status())),
            Some(Err(_unsent)) => (Some(Contact::Unsent), None),
            None => (Some(Contact::Silent), None),
        })
        .await
    }

    /// Sends `query` to the node once, as [`NodeClient::query`] describes.
    async fn query_once(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<NodeReply, DispatchError> {
        let call = self.prepared(query, options)?;
        let started = Instant::now();
        let Ok(slot) = self.slot(options.deadline()).await else {
            return Ok(cap::capped_reply(classify::elapsed_ms(started)));
        };
        self.sent_query(query, options, call, (started, slot)).await
    }

    /// The call options of `query` under `options`, once the outbound gate
    /// admitted it.
    fn prepared(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<CallOptions, DispatchError> {
        self.gate(query, options)?;
        options
            .call_options(&self.endpoint)
            .map_err(|error| DispatchError::of_options(&self.endpoint, error))
    }

    /// Sends `query` once with `call`, holding `slot` of the endpoint's cap
    /// until the node answered or failed, its latency measured from
    /// `started`.
    async fn sent_query(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
        call: CallOptions,
        (started, _slot): (Instant, Slot),
    ) -> Result<NodeReply, DispatchError> {
        let params = QueryExecuteAdhocQueryBodyParams {
            accept: None,
            content_type: None,
        };
        let client = self.client_for(options);
        let answer = QueryClient::new(&client)
            .with_options(call)
            .query_execute_adhoc_query_body(&params, &query.body())
            .await;
        let latency_ms = classify::elapsed_ms(started);
        match answer {
            Ok(outcome) => Ok(classify::narrow(
                classify::answered(outcome, latency_ms, options.withheld()),
                query.width,
            )),
            Err(error) => classify::failed(
                (&self.endpoint, &self.consent_refusal_codes),
                error,
                latency_ms,
                options,
            ),
        }
    }
}
