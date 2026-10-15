// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shared revision store: one PostgreSQL database every gateway replica
//! uses, so a revision one replica saves, and the mark one moves, hold for
//! all of them.
//!
//! The revisions live in `ferrofed.config_revision`, keyed by number, the
//! current mark in `ferrofed.config_current`, and the approvals of each
//! revision in `ferrofed.config_approval`. A save is one `INSERT …
//! SELECT … WHERE` the newest number is the base, `ON CONFLICT (id) DO
//! NOTHING`, so two replicas saving on one base store exactly one revision,
//! in one statement; a move of the mark is one `UPDATE … WHERE` the mark is
//! the revision the writer read
//! (<https://www.postgresql.org/docs/18/sql-insert.html#SQL-ON-CONFLICT>).
//! Each time the store connects, one transaction under the stored-query
//! store's advisory lock creates the tables when absent and records the
//! layout's schema version, and a newer one is refused. It runs on a
//! connection of its own (`stored::postgres::Connection`), on a thread with a runtime of its
//! own, because [`RevisionStore`] is called where a thread may block. No
//! specification governs the storage: our own design.

use ferrofed_registry::definition::store;
use ferrofed_registry::secret::SecretUrl;
use tokio_postgres::Client;

use ferrofed_registry::revision::{Current, Revision, RevisionId, Signed};

use crate::revision::{Draft, RevisionStore, Saved, StoreError, Swapped};
use crate::stored::postgres::{Connection, Statement, lock};

/// The table of revisions, qualified by its schema.
pub const TABLE: &str = "ferrofed.config_revision";

/// The key `ferrofed.schema_version` records the revisions' layout under.
pub const LAYOUT: &str = "config_revision";

/// The schema version of the layout this binary writes and reads.
pub const VERSION: i32 = 2;

/// The layout's version as `ferrofed.schema_version` records it.
const SELECT_VERSION: &str = "SELECT version FROM ferrofed.schema_version WHERE layout = $1";

/// Records the layout's version.
const RECORD_VERSION: &str = "INSERT INTO ferrofed.schema_version (layout, version) VALUES ($1, $2)
ON CONFLICT (layout) DO UPDATE SET version = EXCLUDED.version";

/// Version 1: the revisions and the current mark.
const MIGRATION_1: &str = "CREATE TABLE IF NOT EXISTS ferrofed.config_revision (
  id bigint PRIMARY KEY,
  record text NOT NULL
);
CREATE TABLE IF NOT EXISTS ferrofed.config_current (
  mark text PRIMARY KEY,
  revision bigint NOT NULL,
  record text NOT NULL
);";

/// Version 2: the approvals of each revision, one row per approver.
const MIGRATION_2: &str = "CREATE TABLE IF NOT EXISTS ferrofed.config_approval (
  revision bigint NOT NULL,
  approver text NOT NULL,
  record text NOT NULL,
  PRIMARY KEY (revision, approver)
);";

/// Records approval `$3` by `$2` of revision `$1`, once per approver.
const INSERT_APPROVAL: &str = "INSERT INTO ferrofed.config_approval (revision, approver, record)
VALUES ($1, $2, $3) ON CONFLICT (revision, approver) DO NOTHING";

/// The approvals of one revision.
const SELECT_APPROVALS: &str =
    "SELECT record FROM ferrofed.config_approval WHERE revision = $1 ORDER BY approver";

/// Stores revision `$1` when the newest number held is `$3`, `0` for none.
const INSERT: &str = "INSERT INTO ferrofed.config_revision (id, record)
SELECT $1::bigint, $2::text
WHERE (SELECT coalesce(max(id), 0) FROM ferrofed.config_revision) = $3::bigint
ON CONFLICT (id) DO NOTHING";

/// The newest number held.
const HEAD: &str = "SELECT max(id) FROM ferrofed.config_revision";

/// One revision.
const SELECT_ONE: &str = "SELECT record FROM ferrofed.config_revision WHERE id = $1";

/// Every revision, oldest first.
const SELECT_ALL: &str = "SELECT record FROM ferrofed.config_revision ORDER BY id";

/// The current mark.
const SELECT_CURRENT: &str = "SELECT record FROM ferrofed.config_current WHERE mark = 'current'";

/// Sets the first current mark.
const INSERT_CURRENT: &str = "INSERT INTO ferrofed.config_current (mark, revision, record)
VALUES ('current', $1, $2) ON CONFLICT (mark) DO NOTHING";

/// Moves the current mark from revision `$3`.
const UPDATE_CURRENT: &str = "UPDATE ferrofed.config_current SET revision = $1, record = $2
WHERE mark = 'current' AND revision = $3";

/// The configuration revisions in one shared PostgreSQL database.
#[derive(Debug)]
pub struct PostgresRevisions {
    connection: Connection,
}

impl PostgresRevisions {
    /// Connects to the database `url` names and creates the tables when
    /// absent.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when `url` does not parse, the TLS setup
    /// fails, the database cannot be reached in time, or the tables cannot
    /// be created, and [`StoreError::Newer`] when the database records a
    /// newer layout than this binary knows.
    pub fn open(url: &SecretUrl) -> Result<Self, StoreError> {
        Connection::open(url, "ferrofed-config-revisions", prepare_revisions)
            .map(|connection| Self { connection })
            .map_err(revision_error)
    }

    /// Runs `work` on the connection.
    fn call<T, W>(&self, work: W) -> Result<T, StoreError>
    where
        T: Send + 'static,
        W: for<'c> FnOnce(&'c Client) -> Statement<'c, T> + Send + 'static,
    {
        self.connection.call(work).map_err(revision_error)
    }

    /// The newest number held.
    fn newest(&self) -> Result<Option<RevisionId>, StoreError> {
        self.call(|client| Box::pin(head(client)))
    }

    /// The revision the mark names.
    fn marked(&self) -> Result<Option<RevisionId>, StoreError> {
        Ok(self.current()?.map(|current| current.revision))
    }
}

impl RevisionStore for PostgresRevisions {
    fn save(&self, base: Option<RevisionId>, draft: Draft) -> Result<Saved, StoreError> {
        let Some(id) = RevisionId::after(base) else {
            return Ok(Saved::Exhausted);
        };
        let revision = draft.numbered(id, base);
        let json = serde_json::to_string(&revision).map_err(backend)?;
        let (number, base_number) = (number(id)?, base.map_or(Ok(0), number)?);
        let rows = self.call(move |client| {
            Box::pin(async move {
                client
                    .execute(INSERT, &[&number, &json, &base_number])
                    .await
                    .map_err(connection_backend)
            })
        })?;
        if rows == 1 {
            Ok(Saved::Stored(revision))
        } else {
            Ok(Saved::Stale {
                head: self.newest()?,
            })
        }
    }

    fn revision(&self, id: RevisionId) -> Result<Option<Revision>, StoreError> {
        let id = number(id)?;
        let record = self.call(move |client| {
            Box::pin(async move {
                let row = client
                    .query_opt(SELECT_ONE, &[&id])
                    .await
                    .map_err(connection_backend)?;
                row.map(|row| row.try_get::<usize, String>(0).map_err(connection_corrupt))
                    .transpose()
            })
        })?;
        record
            .map(|json| serde_json::from_str(&json).map_err(corrupt))
            .transpose()
    }

    fn history(&self) -> Result<Vec<Revision>, StoreError> {
        let records = self.call(|client| {
            Box::pin(async move {
                let rows = client
                    .query(SELECT_ALL, &[])
                    .await
                    .map_err(connection_backend)?;
                rows.iter()
                    .map(|row| row.try_get::<usize, String>(0).map_err(connection_corrupt))
                    .collect::<Result<Vec<_>, _>>()
            })
        })?;
        records
            .iter()
            .map(|json| serde_json::from_str(json).map_err(corrupt))
            .collect()
    }

    fn head(&self) -> Result<Option<RevisionId>, StoreError> {
        self.newest()
    }

    fn current(&self) -> Result<Option<Current>, StoreError> {
        let record = self.call(|client| {
            Box::pin(async move {
                let row = client
                    .query_opt(SELECT_CURRENT, &[])
                    .await
                    .map_err(connection_backend)?;
                row.map(|row| row.try_get::<usize, String>(0).map_err(connection_corrupt))
                    .transpose()
            })
        })?;
        record
            .map(|json| serde_json::from_str(&json).map_err(corrupt))
            .transpose()
    }

    fn swap_current(
        &self,
        expected: Option<RevisionId>,
        next: &Current,
    ) -> Result<Swapped, StoreError> {
        let json = serde_json::to_string(next).map_err(backend)?;
        let revision = number(next.revision)?;
        let expected_number = expected.map(number).transpose()?;
        let rows = self.call(move |client| {
            Box::pin(async move {
                match expected_number {
                    None => client.execute(INSERT_CURRENT, &[&revision, &json]).await,
                    Some(from) => {
                        client
                            .execute(UPDATE_CURRENT, &[&revision, &json, &from])
                            .await
                    }
                }
                .map_err(connection_backend)
            })
        })?;
        if rows == 1 {
            Ok(Swapped::Swapped)
        } else {
            Ok(Swapped::Stale {
                current: self.marked()?,
            })
        }
    }

    fn approvals(&self, id: RevisionId) -> Result<Vec<Signed>, StoreError> {
        let id = number(id)?;
        let records = self.call(move |client| {
            Box::pin(async move {
                let rows = client
                    .query(SELECT_APPROVALS, &[&id])
                    .await
                    .map_err(connection_backend)?;
                rows.iter()
                    .map(|row| row.try_get::<usize, String>(0).map_err(connection_corrupt))
                    .collect::<Result<Vec<_>, _>>()
            })
        })?;
        records
            .iter()
            .map(|json| serde_json::from_str(json).map_err(corrupt))
            .collect()
    }

    fn approve(&self, id: RevisionId, approval: &Signed) -> Result<(), StoreError> {
        let id = number(id)?;
        let approver = serde_json::to_string(&approval.by).map_err(backend)?;
        let record = serde_json::to_string(approval).map_err(backend)?;
        self.call(move |client| {
            Box::pin(async move {
                client
                    .execute(INSERT_APPROVAL, &[&id, &approver, &record])
                    .await
                    .map(drop)
                    .map_err(connection_backend)
            })
        })
    }
}

/// The newest number `client` reads.
async fn head(client: &Client) -> Result<Option<RevisionId>, store::StoreError> {
    let row = client
        .query_one(HEAD, &[])
        .await
        .map_err(connection_backend)?;
    let newest = row
        .try_get::<usize, Option<i64>>(0)
        .map_err(connection_corrupt)?;
    match newest {
        None => Ok(None),
        Some(newest) => u64::try_from(newest)
            .ok()
            .and_then(RevisionId::new)
            .map(Some)
            .ok_or_else(|| connection_corrupt(NotARevision(newest))),
    }
}

/// A held number that is no revision number.
#[derive(Debug, thiserror::Error)]
#[error("the revision store holds the number {0}, which is no revision number")]
struct NotARevision(i64);

/// `id` as the database stores it.
fn number(id: RevisionId) -> Result<i64, StoreError> {
    i64::try_from(id.get()).map_err(backend)
}

/// [`prepare`], as a connection runs it.
fn prepare_revisions(client: &mut Client) -> Statement<'_, ()> {
    Box::pin(prepare(client))
}

/// Creates the tables when absent and records the layout's version, in one
/// transaction under the advisory lock; a newer version is refused and the
/// transaction rolled back.
async fn prepare(client: &mut Client) -> Result<(), store::StoreError> {
    let transaction = client.transaction().await.map_err(connection_backend)?;
    lock(&transaction).await?;
    let found = transaction
        .query_opt(SELECT_VERSION, &[&LAYOUT])
        .await
        .map_err(connection_backend)?
        .map(|row| row.try_get::<usize, i32>(0))
        .transpose()
        .map_err(connection_corrupt)?
        .unwrap_or(0);
    if found > VERSION {
        return Err(store::StoreError::Backend(Box::new(StoreError::Newer {
            found: u32::try_from(found).unwrap_or(u32::MAX),
            supported: VERSION.unsigned_abs(),
        })));
    }
    if found < 1 {
        transaction
            .batch_execute(MIGRATION_1)
            .await
            .map_err(connection_backend)?;
    }
    if found < 2 {
        transaction
            .batch_execute(MIGRATION_2)
            .await
            .map_err(connection_backend)?;
    }
    if found < VERSION {
        transaction
            .execute(RECORD_VERSION, &[&LAYOUT, &VERSION])
            .await
            .map_err(connection_backend)?;
    }
    transaction.commit().await.map_err(connection_backend)
}

/// The revision store's error for a connection's `error`, the layout refusal
/// of [`prepare`] kept as it is.
fn revision_error(error: store::StoreError) -> StoreError {
    match error {
        store::StoreError::Backend(source) => match source.downcast::<StoreError>() {
            Ok(revision) => *revision,
            Err(source) => StoreError::Backend(source),
        },
        store::StoreError::Corrupt(source) => StoreError::Corrupt(source),
        other => StoreError::Backend(Box::new(other)),
    }
}

/// The backend failure `error`.
fn backend(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Backend(Box::new(error))
}

/// A held row that does not read as a revision, for `error`.
fn corrupt(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Corrupt(Box::new(error))
}

/// The connection's backend failure `error`.
fn connection_backend(error: impl std::error::Error + Send + Sync + 'static) -> store::StoreError {
    store::StoreError::Backend(Box::new(error))
}

/// The connection's damaged-row failure `error`.
fn connection_corrupt(error: impl std::error::Error + Send + Sync + 'static) -> store::StoreError {
    store::StoreError::Corrupt(Box::new(error))
}

#[cfg(test)]
mod tests {
    use ferrofed_registry::definition::store;

    use super::revision_error;
    use crate::revision::StoreError;

    #[test]
    fn a_newer_layout_refusal_crosses_the_connection_as_itself() {
        let refused = revision_error(store::StoreError::Backend(Box::new(StoreError::Newer {
            found: 2,
            supported: 1,
        })));
        assert!(matches!(
            refused,
            StoreError::Newer {
                found: 2,
                supported: 1
            }
        ));
    }
}
