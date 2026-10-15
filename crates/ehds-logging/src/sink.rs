// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Where access records go: a store the deployment reviews them in, or a
//! connection to external software that does (Regulation (EU) 2025/327
//! Annex II 3.3).
//!
//! [`AccessSink::store`] returns once the record is stored. A sink that
//! cannot store a record says so, and the caller decides what an access
//! without its record means; the sink never alters the access.
//!
//! An access that changes data cannot be taken back once it is made, so its
//! caller asks [`AccessSink::reserve`] for the record's place first and makes
//! the access only once the sink has one: a sink with no room refuses the
//! access before it happens, and the [`Reservation`] is filled with the
//! record of how the access ended.

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

    /// Takes the place of one record before the access it records is made.
    ///
    /// The place is held until the [`Reservation`] is filled or dropped, so
    /// two accesses never both take a sink's last place.
    ///
    /// # Errors
    ///
    /// A [`SinkError`] when the sink has no room for one more record.
    async fn reserve(&self) -> Result<Box<dyn Reservation>, SinkError>;
}

/// The place of one record a sink took before its access was made; dropped
/// unfilled, it gives the place back.
#[async_trait::async_trait]
pub trait Reservation: Send + std::fmt::Debug {
    /// Stores `record` in this place.
    ///
    /// # Errors
    ///
    /// A [`SinkError`] when the record cannot be stored even so: it could
    /// not be written, or not within the sink's bound.
    async fn store(self: Box<Self>, record: AccessRecord) -> Result<(), SinkError>;
}

/// Why a sink could not store a record.
#[derive(Debug, thiserror::Error)]
#[error("the access record could not be stored")]
pub struct SinkError(#[source] pub Box<dyn std::error::Error + Send + Sync>);
