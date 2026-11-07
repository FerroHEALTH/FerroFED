// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `DPoP` proofs accepted inside their window, shared by every replica.
//!
//! RFC 9449 §11.1 has a server store "the jti value of each `DPoP` proof
//! for the time window in which the respective `DPoP` proof JWT would be
//! accepted", notes that this "may not always be feasible in practice, e.g.,
//! when multiple servers behind a single endpoint have no shared state", and
//! has it "store only a hash thereof". `ferrofed.dpop_proof` holds one row
//! per accepted proof, keyed by the SHA-256 of its `jti` as
//! `ferrohealth-smart` computes it, never the value, with `expires_at`, the
//! end of the window the proof is accepted in, past which the sweep deletes
//! the row. The check and the record are one statement,
//! `INSERT ... ON CONFLICT DO NOTHING`, so of two replicas receiving one
//! proof at once only one records it, and only that one admits the request
//! (<https://www.postgresql.org/docs/18/sql-insert.html#SQL-ON-CONFLICT>).
//! A store that does not answer fails the request closed. Only `DPoP` proofs
//! are recorded, never an access token or an assertion id. No specification
//! governs the table or the statement: our own design.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

use ferrohealth_smart::sender::replay::{ProofId, ReplayStore, ReplayStoreError};

use crate::store::Store;
use crate::store::connection::backend;

/// The table of the accepted proofs.
pub const TABLE: &str = "ferrofed.dpop_proof";

/// Records the proof `$1` until `$2`, answering a row when it was not
/// recorded before.
const RECORD: &str = "INSERT INTO ferrofed.dpop_proof (proof, expires_at) VALUES ($1, $2)
ON CONFLICT (proof) DO NOTHING
RETURNING proof";

/// The `DPoP` replay store over FerroFED's own store.
#[derive(Debug, Clone)]
pub struct Proofs {
    store: Arc<Store>,
}

impl Proofs {
    /// Returns the replay store over `store`.
    #[must_use]
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl ReplayStore for Proofs {
    fn first_use<'a>(
        &'a self,
        proof: &'a ProofId,
        until: SystemTime,
    ) -> Pin<Box<dyn Future<Output = Result<bool, ReplayStoreError>> + Send + 'a>> {
        let key = proof.as_str().to_owned();
        Box::pin(async move {
            let recorded = self
                .store
                .call_async(move |client| {
                    Box::pin(async move {
                        client
                            .query_opt(RECORD, &[&key, &until])
                            .await
                            .map(|row| row.is_some())
                            .map_err(backend)
                    })
                })
                .await
                .map_err(ReplayStoreError::new)?;
            Ok(recorded)
        })
    }
}
