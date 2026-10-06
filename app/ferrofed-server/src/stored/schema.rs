// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The schema version a writable stored-query store records.
//!
//! The embedded `redb` file and the PostgreSQL database each record the
//! version of the layout they hold. At start, a store at an older version is
//! migrated forward one step at a time, in order, and records the version it
//! reached; a store at a newer version than this binary knows was written by
//! a newer FerroFED, and the start is refused with [`SchemaError::Newer`]
//! rather than read with a layout it does not have. A store with no version
//! recorded was written before versions were recorded, and holds the layout
//! of version 1. No specification governs the storage: our own design.

use std::ops::RangeInclusive;

/// The schema version this binary writes and reads.
pub const CURRENT: u32 = 1;

/// A store whose schema this binary cannot use.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The store records a newer schema than this binary knows.
    #[error(
        "the {store} stored-query store is at schema version {found}, and this FerroFED knows versions up to {supported}: a newer FerroFED wrote it; run that version, or restore the backup taken before the upgrade (see the book's Rollback page)"
    )]
    Newer {
        /// Which store: `redb` or `PostgreSQL`.
        store: &'static str,
        /// The version the store records.
        found: u32,
        /// The newest version this binary knows, [`CURRENT`].
        supported: u32,
    },
    /// This binary names a schema version it has no migration to: a defect
    /// of the binary, never of the store.
    #[error(
        "this FerroFED has no migration of the {store} stored-query store to schema version {version}"
    )]
    NoMigration {
        /// Which store.
        store: &'static str,
        /// The version with no migration.
        version: u32,
    },
}

/// Returns the migrations a `store` at version `found` needs to reach
/// [`CURRENT`], in the order they run; empty when it is current.
///
/// `found` is `0` for a store that holds nothing yet.
///
/// # Errors
///
/// [`SchemaError::Newer`] when `found` is past [`CURRENT`].
pub fn pending(store: &'static str, found: u32) -> Result<RangeInclusive<u32>, SchemaError> {
    if found > CURRENT {
        return Err(SchemaError::Newer {
            store,
            found,
            supported: CURRENT,
        });
    }
    Ok(found.saturating_add(1)..=CURRENT)
}

#[cfg(test)]
mod tests {
    use super::{CURRENT, SchemaError, pending};

    #[test]
    fn an_empty_store_runs_every_migration_and_a_current_one_none() {
        assert_eq!(vec![1], pending("redb", 0).unwrap().collect::<Vec<_>>());
        assert!(pending("redb", CURRENT).unwrap().next().is_none());
    }

    #[test]
    fn a_newer_store_is_refused_naming_both_versions_and_the_way_back() {
        let refused = pending("PostgreSQL", CURRENT + 1).unwrap_err();
        assert_eq!(
            SchemaError::Newer {
                store: "PostgreSQL",
                found: CURRENT + 1,
                supported: CURRENT
            },
            refused
        );
        let message = refused.to_string();
        assert!(
            message.contains(&format!("version {}", CURRENT + 1)),
            "{message}"
        );
        assert!(message.contains("Rollback"), "{message}");
    }
}
