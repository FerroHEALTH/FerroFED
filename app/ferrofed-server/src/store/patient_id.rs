// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifier each stable patient id of the FHIR face stands for,
//! shared by every replica, so a `DocumentReference?patient=` naming an id
//! an earlier answer gave resolves to the patient.
//!
//! `ferrofed.patient_id` holds one row per id, its identifier sealed under
//! a key derived from `fhir.id_key` ([`crate::fhir::ids`]), so the table
//! alone discloses no identifier. Every answer that names the patient
//! writes the row again with `expires_at` `fhir.patient_id_lifetime` after
//! it, by the database's own clock, and the sweep deletes a row past it, so
//! the table holds the patients of recent answers alone. No specification
//! governs the table: our own design.

use std::time::Duration;

use ferrofed_registry::definition::store::StoreError;

use crate::fhir::ids::PatientId;
use crate::store::Store;
use crate::store::connection::backend;

/// Records the seal `$2` of the id `$1` for `$3` seconds, renewing a row
/// the id holds.
const RECORD: &str = "INSERT INTO ferrofed.patient_id (id, sealed, expires_at)
VALUES ($1, $2, now() + make_interval(secs => $3::bigint))
ON CONFLICT (id) DO UPDATE SET sealed = EXCLUDED.sealed, expires_at = EXCLUDED.expires_at";

/// The seal of the id `$1`, while its window lasts.
const READ: &str = "SELECT sealed FROM ferrofed.patient_id WHERE id = $1 AND expires_at > now()";

impl Store {
    /// Records `sealed` as the identifier `id` stands for, for `window`
    /// from now.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails.
    pub async fn record_patient_id(
        &self,
        id: &PatientId,
        sealed: Vec<u8>,
        window: Duration,
    ) -> Result<(), StoreError> {
        let key = id.as_str().to_owned();
        let window = i64::try_from(window.as_secs()).map_err(backend)?;
        self.call_async(move |client| {
            Box::pin(async move {
                client
                    .execute(RECORD, &[&key, &sealed, &window])
                    .await
                    .map(drop)
                    .map_err(backend)
            })
        })
        .await
    }

    /// The seal recorded for `id`, or `None` for an id never recorded or
    /// past its window.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails.
    pub async fn patient_id(&self, id: &PatientId) -> Result<Option<Vec<u8>>, StoreError> {
        let key = id.as_str().to_owned();
        self.call_async(move |client| {
            Box::pin(async move {
                let Some(row) = client.query_opt(READ, &[&key]).await.map_err(backend)? else {
                    return Ok(None);
                };
                row.try_get(0).map(Some).map_err(backend)
            })
        })
        .await
    }
}
