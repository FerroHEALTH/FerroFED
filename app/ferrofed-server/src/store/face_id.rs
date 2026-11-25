// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The patient identifier each stable id of the FHIR face stands for.
//!
//! An id is a patient's or a listed document's, and the record is shared by
//! every replica, so an id an earlier answer gave resolves to its patient:
//! `DocumentReference?patient=`, `?_id=` and `DocumentReference/{id}`.
//!
//! `ferrofed.face_id` holds one row per id, the identifier sealed under a
//! key derived from `fhir.id_key` ([`crate::fhir::ids`]), so the table
//! alone discloses no identifier. Every answer that names an id writes its
//! row again with `expires_at` `fhir.id_lifetime` after it, by the
//! database's own clock, and the sweep deletes a row past it, so the table
//! holds the ids of recent answers alone. No specification governs the
//! table: our own design.

use std::time::Duration;

use ferrofed_registry::definition::store::StoreError;

use crate::fhir::ids::FaceId;
use crate::store::Store;
use crate::store::connection::backend;

/// Records each seal of `$2` for the id at its position in `$1`, for `$3`
/// seconds, renewing a row an id holds.
const RECORD: &str = "INSERT INTO ferrofed.face_id (id, sealed, expires_at)
SELECT id, sealed, now() + make_interval(secs => $3::bigint)
FROM unnest($1::text[], $2::bytea[]) AS recorded (id, sealed)
ON CONFLICT (id) DO UPDATE SET sealed = EXCLUDED.sealed, expires_at = EXCLUDED.expires_at";

/// The seal of the id `$1`, while its window lasts.
const READ: &str = "SELECT sealed FROM ferrofed.face_id WHERE id = $1 AND expires_at > now()";

impl Store {
    /// Records each seal of `recorded` as the identifier its id stands
    /// for, for `window` from now, in one statement.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails.
    pub async fn record_face_ids(
        &self,
        recorded: Vec<(FaceId, Vec<u8>)>,
        window: Duration,
    ) -> Result<(), StoreError> {
        let (ids, sealed): (Vec<String>, Vec<Vec<u8>>) = recorded
            .into_iter()
            .map(|(id, sealed)| (id.as_str().to_owned(), sealed))
            .unzip();
        let window = i64::try_from(window.as_secs()).map_err(backend)?;
        self.call_async(move |client| {
            Box::pin(async move {
                client
                    .execute(RECORD, &[&ids, &sealed, &window])
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
    pub async fn face_id(&self, id: &FaceId) -> Result<Option<Vec<u8>>, StoreError> {
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
