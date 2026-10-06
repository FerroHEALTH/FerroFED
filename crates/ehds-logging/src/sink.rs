// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where access records go: a store the deployment reviews them in, or a
//! connection to external software that does (Regulation (EU) 2025/327
//! Annex II 3.3).
//!
//! [`AccessSink::store`] returns once the record is stored. A sink that
//! cannot store a record says so, and the caller decides what an access
//! without its record means; the sink never alters the access.

use crate::record::AccessRecord;

/// A store of access records.
#[async_trait::async_trait]
pub trait AccessSink: Send + Sync {
    /// Stores `record`.
    ///
    /// # Errors
    ///
    /// A [`SinkError`] when the record cannot be stored.
    async fn store(&self, record: AccessRecord) -> Result<(), SinkError>;
}

/// Why a sink could not store a record.
#[derive(Debug, thiserror::Error)]
#[error("the access record could not be stored")]
pub struct SinkError(#[source] pub Box<dyn std::error::Error + Send + Sync>);
