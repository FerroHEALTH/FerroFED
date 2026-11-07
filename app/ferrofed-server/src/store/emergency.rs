// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The count of each relayed health professional's emergency accesses,
//! shared by every replica.
//!
//! eHDSI Section II Security Services 10.0.0 §2.5 has the identity of the
//! professional of each emergency access "stored in a table", and the access
//! denied after a number of accesses by that identity in a defined amount
//! of time. `ferrofed.emergency_access` holds one row per professional,
//! keyed by the SHA-256 digest of the identity ([`Professional`]), so the
//! table names no professional: the instants of the accesses granted inside
//! the window, whether the last attempt was granted, and `expires_at`, the
//! end of the window of the last access granted, past which the sweep
//! deletes the row.
//!
//! [`Store::emergency`] counts and grants in one statement: an upsert that
//! locks the professional's row, drops the instants older than the window,
//! and grants the attempt only while fewer than the threshold remain, so
//! two replicas granting at once never both pass the threshold
//! (<https://www.postgresql.org/docs/18/sql-insert.html#SQL-ON-CONFLICT>).
//! The window slides: any span of its length holds at most the threshold of
//! granted accesses. A refused attempt is not counted, so a professional who
//! keeps asking is granted again once the oldest access leaves the window.
//! The instants are the database's own clock, one for every replica. No
//! specification governs the table or the statement: our own design.

use ferrofed_engine::conveyance::relayed::Relayed;
use ferrofed_registry::definition::store::StoreError;

use crate::config::emergency::EmergencyLimit;
use crate::store::Store;
use crate::store::connection::backend;

/// The table of the emergency accesses.
pub const TABLE: &str = "ferrofed.emergency_access";

/// Counts one emergency attempt of the professional `$1` against the
/// threshold `$2` in a window of `$3` seconds, and answers whether it is
/// granted.
const COUNT: &str = "INSERT INTO ferrofed.emergency_access AS held
  (professional, granted, admitted, expires_at)
VALUES ($1, ARRAY[now()], true, now() + make_interval(secs => $3::bigint))
ON CONFLICT (professional) DO UPDATE SET
  granted = ARRAY(
    SELECT stamp FROM unnest(held.granted) AS stamp
    WHERE stamp > now() - make_interval(secs => $3::bigint) ORDER BY stamp
  ) || CASE WHEN (
    SELECT count(*) FROM unnest(held.granted) AS stamp
    WHERE stamp > now() - make_interval(secs => $3::bigint)
  ) < $2::bigint THEN ARRAY[now()] ELSE ARRAY[]::timestamptz[] END,
  admitted = (
    SELECT count(*) FROM unnest(held.granted) AS stamp
    WHERE stamp > now() - make_interval(secs => $3::bigint)
  ) < $2::bigint,
  expires_at = CASE WHEN (
    SELECT count(*) FROM unnest(held.granted) AS stamp
    WHERE stamp > now() - make_interval(secs => $3::bigint)
  ) < $2::bigint THEN now() + make_interval(secs => $3::bigint) ELSE held.expires_at END
RETURNING admitted";

/// The identity of a health professional a national contact point relays,
/// as the count keys it: the SHA-256 digest of the contact point, the
/// country, the issuing authority and the `hp_identifier`.
///
/// `Debug` shows the digest alone, which names no professional.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Professional(String);

impl Professional {
    /// The professional `relayed` names, as a contact point relays them
    /// (Implementing Regulation (EU) 2026/2099 Annex Table 1).
    #[must_use]
    pub fn of(relayed: &Relayed) -> Self {
        let professional = &relayed.professional;
        let parts = [
            relayed.contact_point.as_str(),
            relayed.country_code.as_str(),
            professional.issuing_authority_name.as_str(),
            professional.hp_identifier.as_str(),
        ];
        // NOTE: no specification governs this: our own design; each part is length-prefixed,
        // so no two identities share the text the digest is taken over.
        let text = parts.iter().fold(String::new(), |mut text, part| {
            text.push_str(&part.len().to_string());
            text.push(':');
            text.push_str(part);
            text
        });
        Self(crate::digest::sha256(text.as_bytes()))
    }

    /// The digest, as the table keys the professional.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Whether an emergency attempt is granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counted {
    /// Fewer than the threshold were granted in the window, so this one is,
    /// and it is counted.
    Granted,
    /// The threshold was reached in the window, so this one is denied and
    /// not counted.
    Exceeded,
}

impl Store {
    /// Counts one emergency attempt of `professional` against `limit`, and
    /// answers whether it is granted, in one statement every replica's
    /// attempts are serialised by.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time,
    /// or the statement fails: the attempt is then neither counted nor
    /// granted.
    pub async fn emergency(
        &self,
        professional: &Professional,
        limit: EmergencyLimit,
    ) -> Result<Counted, StoreError> {
        let key = professional.as_str().to_owned();
        let threshold = i64::from(limit.threshold.get());
        let window = i64::try_from(limit.window.as_secs()).map_err(backend)?;
        let admitted: bool = self
            .call_async(move |client| {
                Box::pin(async move {
                    client
                        .query_one(COUNT, &[&key, &threshold, &window])
                        .await
                        .map_err(backend)?
                        .try_get(0)
                        .map_err(backend)
                })
            })
            .await?;
        Ok(if admitted {
            Counted::Granted
        } else {
            Counted::Exceeded
        })
    }
}

#[cfg(test)]
mod tests {
    use ferrofed_engine::conveyance::relayed::{HealthProfessional, HealthcareProvider, Relayed};

    use super::{COUNT, Professional, TABLE};

    /// A synthetic relayed professional, `hp_identifier` `id` issued by
    /// `authority`.
    fn relayed(authority: &str, id: &str) -> Relayed {
        Relayed {
            contact_point: String::from("https://ncp.example.org"),
            country_code: String::from("LU"),
            professional: HealthProfessional {
                hp_identifier: id.to_owned(),
                issuing_authority_name: authority.to_owned(),
                hp_professional_role: Vec::new(),
                on_behalf_of: None,
            },
            provider: HealthcareProvider {
                identifier: String::from("urn:oid:2.999.9.66"),
                issuing_authority_name: String::from("Qz7-hcp-authority"),
            },
        }
    }

    #[test]
    fn a_professional_is_keyed_by_a_digest_that_names_no_attribute() {
        let key = Professional::of(&relayed("Qz7-authority", "Qz7-hp-63"));
        assert_eq!(64, key.as_str().len(), "{key:?}");
        assert!(key.as_str().bytes().all(|byte| byte.is_ascii_hexdigit()));
        for attribute in ["Qz7-authority", "Qz7-hp-63", "LU", "ncp.example.org"] {
            assert!(!format!("{key:?}").contains(attribute), "{attribute}");
        }
        assert_eq!(
            key,
            Professional::of(&relayed("Qz7-authority", "Qz7-hp-63"))
        );
    }

    #[test]
    fn two_identities_sharing_their_text_have_two_keys() {
        assert_ne!(
            Professional::of(&relayed("ab", "c")),
            Professional::of(&relayed("a", "bc"))
        );
        let mut other_provider = relayed("Qz7-authority", "Qz7-hp-63");
        other_provider.provider.identifier = String::from("urn:oid:2.999.9.67");
        assert_eq!(
            Professional::of(&relayed("Qz7-authority", "Qz7-hp-63")),
            Professional::of(&other_provider),
            "the provider is no part of the identity"
        );
    }

    #[test]
    fn the_count_runs_in_one_statement_over_the_table() {
        assert!(COUNT.starts_with(&format!("INSERT INTO {TABLE} AS held")));
        assert!(COUNT.contains("ON CONFLICT (professional) DO UPDATE"));
        assert!(!COUNT.contains(';'), "one statement");
    }
}
