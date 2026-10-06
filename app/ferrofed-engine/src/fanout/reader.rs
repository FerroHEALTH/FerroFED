// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A reader of each endpoint's own rows, called before the merge combines
//! them (§11.6).
//!
//! The merged rows do not say which endpoint each came from, and an endpoint
//! column added to tell them apart would take part in which rows `DISTINCT`
//! keeps (N13). A caller that needs a fact of each endpoint's answer, such
//! as the access log's categories per origin, reads it here instead, and the
//! merge and its answer stay as they are. No specification governs the
//! reader: our own design.

use std::fmt;
use std::sync::Arc;

use ferrofed_registry::id::EndpointId;
use openehr_its::rest::generated::query::ResultSetRow;

/// The signature of what a [`RowReader`] calls.
type Read = dyn Fn(&EndpointId, &[ResultSetRow]) + Send + Sync;

/// Reads the rows each endpoint answered with, once per endpoint that
/// answered, before the merge ([`super::Plan::reading`]).
///
/// It is called for every endpoint whose node answered rows, an answer the
/// merge later refuses or a failing query discards included, so a reader
/// that counts only what was delivered checks the endpoint's record in the
/// answer.
#[derive(Clone)]
pub struct RowReader(Arc<Read>);

impl RowReader {
    /// The reader that calls `read` with each answering endpoint and its
    /// rows.
    #[must_use]
    pub fn new(read: impl Fn(&EndpointId, &[ResultSetRow]) + Send + Sync + 'static) -> Self {
        Self(Arc::new(read))
    }

    /// Reads the `rows` `endpoint` answered with.
    pub(super) fn read(&self, endpoint: &EndpointId, rows: &[ResultSetRow]) {
        (self.0)(endpoint, rows);
    }
}

impl fmt::Debug for RowReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RowReader").finish_non_exhaustive()
    }
}
