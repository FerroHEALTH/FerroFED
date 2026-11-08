// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Who dispensed each dispensation a national contact point wrote, shared
//! by every replica, for the 24 hours a discard of it may follow.
//!
//! MyHealth@EU Requirements Catalogue 10.0.0 07.04 admits a discard within
//! 24 hours of the dispensation, from the point of care that dispensed it,
//! and the `NCPeH` Components Specification 10.0.0 §3.1.6.4.2 has it
//! "issued by the same HCPO". The identity assertion names the point of
//! care by a name alone, so the dispenser is kept by identifiers: the
//! assertion's `organization-id` (eHDSI SAML Profile 10.0.0 §2.3) and the
//! identifier of the facility the dispensation names on
//! `performer.actor` (HL7 Europe MPD 1.0.0).
//! `ferrofed.contact_point_dispensation` holds one row per dispensation,
//! keyed by the SHA-256 digest of the face's id of it ([`Dispensation`]),
//! with `expires_at` 24 hours after it was written, by the database's own
//! clock, past which the sweep deletes the row and no discard is admitted.
//! No patient identifier and no clinical content is kept. No specification
//! governs the table: our own design.

use std::time::Duration;

use ferrofed_registry::definition::store::StoreError;

use crate::store::Store;
use crate::store::connection::backend;

/// The table of the dispensers.
pub const TABLE: &str = "ferrofed.contact_point_dispensation";

/// How long after a dispensation its discard is admitted (MyHealth@EU
/// Requirements Catalogue 10.0.0 07.04: "within 24 hours").
pub const DISCARD_WINDOW: Duration = Duration::from_hours(24);

/// Records the dispenser `$2`, `$3`, `$4` of the dispensation `$1` for `$5`
/// seconds; a dispensation recorded before keeps its first dispenser.
const RECORD: &str = "INSERT INTO ferrofed.contact_point_dispensation
  (dispensation, organization, facility_system, facility_value, expires_at)
VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5::bigint))
ON CONFLICT (dispensation) DO NOTHING";

/// The dispenser of the dispensation `$1`, while its window lasts.
const READ: &str = "SELECT organization, facility_system, facility_value
FROM ferrofed.contact_point_dispensation
WHERE dispensation = $1 AND expires_at > now()";

/// A dispensation, as the table keys it: the SHA-256 digest of the face's
/// id of it.
///
/// `Debug` shows the digest alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispensation(String);

impl Dispensation {
    /// The dispensation the face names by `id`.
    #[must_use]
    pub fn of(id: &str) -> Self {
        Self(crate::digest::sha256(id.as_bytes()))
    }

    /// The digest, as the table keys the dispensation.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who dispensed a dispensation: the provider the contact point's identity
/// assertion names by its `organization-id`, and the facility the
/// dispensation names on `performer.actor`, by system and value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispenser {
    /// The identity assertion's `organization-id`.
    pub organization: String,
    /// The facility identifier's system and value.
    pub facility: (String, String),
}

impl Store {
    /// Records `dispenser` as who dispensed `dispensation`, for
    /// [`DISCARD_WINDOW`] from now; a dispensation recorded before keeps
    /// its first dispenser.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails.
    pub async fn record_dispenser(
        &self,
        dispensation: &Dispensation,
        dispenser: &Dispenser,
    ) -> Result<(), StoreError> {
        let key = dispensation.as_str().to_owned();
        let Dispenser {
            organization,
            facility: (system, value),
        } = dispenser.clone();
        let window = i64::try_from(DISCARD_WINDOW.as_secs()).map_err(backend)?;
        self.call_async(move |client| {
            Box::pin(async move {
                client
                    .execute(RECORD, &[&key, &organization, &system, &value, &window])
                    .await
                    .map(drop)
                    .map_err(backend)
            })
        })
        .await
    }

    /// Who dispensed `dispensation`, while a discard of it is admitted, or
    /// `None` once its window has passed or for a dispensation never
    /// recorded.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails.
    pub async fn dispenser(
        &self,
        dispensation: &Dispensation,
    ) -> Result<Option<Dispenser>, StoreError> {
        let key = dispensation.as_str().to_owned();
        self.call_async(move |client| {
            Box::pin(async move {
                let Some(row) = client.query_opt(READ, &[&key]).await.map_err(backend)? else {
                    return Ok(None);
                };
                Ok(Some(Dispenser {
                    organization: row.try_get(0).map_err(backend)?,
                    facility: (
                        row.try_get(1).map_err(backend)?,
                        row.try_get(2).map_err(backend)?,
                    ),
                }))
            })
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::Dispensation;

    #[test]
    fn a_dispensation_is_keyed_by_a_digest_that_names_no_id() {
        let key = Dispensation::of("node-a-mpd~ehr~dispense-1");
        assert_eq!(64, key.as_str().len());
        assert!(!format!("{key:?}").contains("dispense-1"));
        assert_ne!(key, Dispensation::of("node-a-mpd~ehr~dispense-2"));
    }
}
