// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The store's migrations: embedded in the binary, applied in order, each
//! once, and never changed once released.
//!
//! `ferrofed.store_migration` records each migration applied, one row per
//! version. [`apply`] runs in one transaction under a transaction-level
//! advisory lock, so of two replicas booting over one empty database the
//! second waits and then finds every migration recorded
//! (<https://www.postgresql.org/docs/18/explicit-locking.html#ADVISORY-LOCKS>).
//! A database that records a version past [`MIGRATIONS`] was migrated by a
//! newer FerroFED and is refused, its transaction rolled back. Migration 1
//! is the stored-query registry's table as the `postgres` backend created
//! it before the store, so a database of that backend is taken over with
//! every definition it holds. Migrations only move forward. No
//! specification governs this: our own design.

use tokio_postgres::Client;

use ferrofed_registry::definition::store::StoreError;

use crate::store::connection::backend;

/// The migrations, in the order they apply: version `n` is the `n`th.
///
/// A released migration never changes; a new one is added at the end.
pub const MIGRATIONS: [&str; 2] = [
    // 1: the stored-query definitions (§12.7, N44), as the `postgres` backend
    // of the stored-query registry created them.
    "CREATE TABLE IF NOT EXISTS ferrofed.stored_query_definition (
  name text NOT NULL,
  version text NOT NULL,
  saved text NOT NULL,
  aql text NOT NULL,
  PRIMARY KEY (name, version)
);",
    // 2: the emergency accesses granted to each relayed health professional, by the
    // digest of the professional's identity (eHDSI Section II Security Services §2.5).
    "CREATE TABLE ferrofed.emergency_access (
  professional text PRIMARY KEY,
  granted timestamptz[] NOT NULL,
  admitted boolean NOT NULL,
  expires_at timestamptz NOT NULL
);",
];

/// The newest migration this binary has, the length of [`MIGRATIONS`].
pub const KNOWN: u32 = 2;

/// The table that records each migration applied.
pub const LEDGER: &str = "ferrofed.store_migration";

/// Takes the lock one replica migrates under, creates the schema when
/// absent, and the ledger.
///
/// The lock is the one the configuration revisions' store and the earlier
/// stored-query store take, so every migration of the schema is serialised.
/// The schema is created only when the catalogue lacks it, so a role without
/// `CREATE` on the database can use a schema made for it.
const LOCK: &str = "SELECT pg_advisory_xact_lock(x'666572726f666564'::bigint);
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'ferrofed') THEN
    CREATE SCHEMA ferrofed;
  END IF;
END $$;
CREATE TABLE IF NOT EXISTS ferrofed.store_migration (
  version integer PRIMARY KEY,
  applied timestamptz NOT NULL DEFAULT now()
);";

/// The newest version the ledger records, `0` for none.
const RECORDED: &str = "SELECT coalesce(max(version), 0) FROM ferrofed.store_migration";

/// Records that migration `$1` was applied.
const RECORD: &str = "INSERT INTO ferrofed.store_migration (version) VALUES ($1)";

/// A database the store cannot migrate.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MigrationError {
    /// The database records a migration past every one this binary has.
    #[error(
        "FerroFED's store is at migration {found}, and this FerroFED knows migrations up to {known}: a newer FerroFED migrated it; run that version, or restore the backup taken before the upgrade (see the book's Rollback page)"
    )]
    Newer {
        /// The newest migration the database records.
        found: u32,
        /// The newest migration this binary has.
        known: u32,
    },
    /// The ledger holds a version that is no migration number.
    #[error("FerroFED's store records migration {0}, which is no migration number")]
    Ledger(i32),
}

/// Applies every migration `client`'s database has not recorded, in one
/// transaction under the advisory lock.
///
/// # Errors
///
/// [`StoreError::Backend`] with [`MigrationError::Newer`] for a database a
/// newer FerroFED migrated, which is left as it was, and for a statement
/// that fails.
pub async fn apply(client: &mut Client) -> Result<(), StoreError> {
    let transaction = client.transaction().await.map_err(backend)?;
    transaction.batch_execute(LOCK).await.map_err(backend)?;
    let found: i32 = transaction
        .query_one(RECORDED, &[])
        .await
        .map_err(backend)?
        .try_get(0)
        .map_err(backend)?;
    let found = u32::try_from(found).map_err(|_negative| backend(MigrationError::Ledger(found)))?;
    if found > KNOWN {
        return Err(backend(MigrationError::Newer {
            found,
            known: KNOWN,
        }));
    }
    let applied = usize::try_from(found).map_err(backend)?;
    for (version, migration) in (1_i32..).zip(MIGRATIONS).skip(applied) {
        transaction
            .batch_execute(migration)
            .await
            .map_err(backend)?;
        transaction
            .execute(RECORD, &[&version])
            .await
            .map_err(backend)?;
    }
    transaction.commit().await.map_err(backend)
}

#[cfg(test)]
mod tests {
    use super::{KNOWN, MIGRATIONS};

    #[test]
    fn the_newest_migration_is_the_last_of_the_list() {
        assert_eq!(Ok(MIGRATIONS.len()), usize::try_from(KNOWN));
    }

    #[test]
    fn a_newer_database_is_refused_naming_both_numbers_and_the_way_back() {
        let refused = super::MigrationError::Newer {
            found: KNOWN + 1,
            known: KNOWN,
        }
        .to_string();
        assert!(
            refused.contains(&format!("migration {}", KNOWN + 1)),
            "{refused}"
        );
        assert!(refused.contains(&format!("up to {KNOWN}")), "{refused}");
        assert!(refused.contains("Rollback"), "{refused}");
    }

    #[test]
    fn the_first_migration_takes_over_the_stored_query_table_it_finds() {
        let [first, ..] = MIGRATIONS;
        assert!(
            first.starts_with("CREATE TABLE IF NOT EXISTS ferrofed.stored_query_definition ("),
            "{first}"
        );
    }
}
