// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The most rows the gateway reads of one node for a query that asks for
//! every row it matches.
//!
//! ITS-REST leaves the rows a node answers without `fetch` to the node
//! ("the default depends on the implementation", Query API `Fetch`), so a
//! node's default page would look like its whole answer, and §11.5 forbids
//! reading a share that may be cut as complete. Every query a node is sent
//! therefore carries an explicit `fetch`. One whose AQL carries a `LIMIT`
//! is sent that `LIMIT` as its `fetch` by its caller
//! ([`NodeQuery::with_fetch`]); one without is sent the ceiling plus one,
//! and a node that answers more rows than the ceiling filled it, so its
//! answer may have been cut and the endpoint is `node-error` (§11.1): it
//! fails the query under all-or-nothing and clears
//! `meta.federation.complete` (§11.4). No specification governs the ceiling
//! itself: our own design.

use std::borrow::Cow;
use std::num::NonZeroU32;

use http::StatusCode;
use openehr_federation::outcome::{ErrorDetail, Outcome};
use openehr_its::rest::client::Transport;

use super::{Contact, NodeClient, NodeClients, NodeQuery, NodeReply};

impl<T: Transport + Clone> NodeClients<T> {
    /// These clients, each sending a query whose AQL carries no `LIMIT`
    /// with `fetch` set to `ceiling` plus one, and reporting a node that
    /// answers it with more than `ceiling` rows `node-error`.
    ///
    /// A set of clients built again, as a registry reload builds it, starts
    /// with no ceiling until this is called on it.
    #[must_use]
    pub fn with_row_ceiling(mut self, ceiling: NonZeroU32) -> Self {
        for client in self.clients.values_mut() {
            client.row_ceiling = Some(ceiling);
        }
        self
    }
}

impl<T> NodeClient<T> {
    /// `query` as it is sent: with `fetch` set to the ceiling plus one where
    /// it carries none, and the ceiling its answer is then held to.
    pub(super) fn paged<'q>(
        &self,
        query: &'q NodeQuery,
    ) -> (Cow<'q, NodeQuery>, Option<NonZeroU32>) {
        match (query.fetch, self.row_ceiling) {
            (None, Some(ceiling)) => {
                let fetch = u64::from(ceiling.get()).saturating_add(1);
                (Cow::Owned(query.clone().with_fetch(fetch)), Some(ceiling))
            }
            _ => (Cow::Borrowed(query), None),
        }
    }
}

/// `reply`, or a `node-error` when the node answered more rows than
/// `ceiling` (§11.1, §11.5).
pub(super) fn held(reply: NodeReply, ceiling: Option<NonZeroU32>) -> NodeReply {
    let (
        NodeReply::Answered {
            result_set,
            latency_ms,
        },
        Some(ceiling),
    ) = (&reply, ceiling)
    else {
        return reply;
    };
    // NOTE: no specification governs this: our own design; a ceiling past
    // usize::MAX rows holds every answer in memory.
    let most = usize::try_from(ceiling.get()).unwrap_or(usize::MAX);
    if result_set.rows.len() <= most {
        return reply;
    }
    NodeReply::Failed {
        outcome: Outcome::NodeError {
            latency_ms: *latency_ms,
            error: ErrorDetail::Text(format!(
                "the node answered more than the {ceiling} rows the gateway reads of one node, so its answer may have been cut"
            )),
        },
        contact: Contact::Answered(StatusCode::OK),
    }
}
