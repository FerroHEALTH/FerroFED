// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query a node is sent: the generated `query_execute_adhoc_query_body`
//! (`POST {base}/v1/query/aql`), through the outbound gate, inside its
//! `node_request` span ([`crate::trace_context`]), and its answer
//! classified.

use std::time::Instant;

use openehr_its::rest::client::Transport;
use openehr_its::rest::generated::query::QueryExecuteAdhocQueryBodyParams;
use openehr_its::rest::generated::query::client::QueryClient;

use super::{Contact, DispatchError, DispatchOptions, NodeClient, NodeQuery, NodeReply, classify};
use crate::trace_context;

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
            "query_execute_adhoc_query_body",
            self.query_once(query, options),
            |reply| match reply {
                Ok(reply) => (Some(reply.contact()), Some(reply.status())),
                Err(_unsent) => (Some(Contact::Unsent), None),
            },
        )
        .await
    }

    /// Sends `query` to the node once, as [`NodeClient::query`] describes.
    async fn query_once(
        &self,
        query: &NodeQuery,
        options: &DispatchOptions,
    ) -> Result<NodeReply, DispatchError> {
        self.gate(query, options)?;
        let call = options
            .call_options(&self.endpoint)
            .map_err(|error| DispatchError::of_options(&self.endpoint, error))?;
        let params = QueryExecuteAdhocQueryBodyParams {
            accept: None,
            content_type: None,
        };
        let started = Instant::now();
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
