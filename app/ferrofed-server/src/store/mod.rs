// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! FerroFED's own PostgreSQL store, which `[store]` configures.
//!
//! One database and login role of FerroFED's own, never a member's CDR, for
//! the state that must both survive a restart and be one on every replica:
//! the definitions of the stored-query registry's `postgres` backend
//! (§12.7, N44), the count of each relayed health professional's
//! emergency accesses ([`emergency`]; eHDSI Section II Security Services
//! 10.0.0 §2.5), and the `DPoP` proofs accepted inside their window
//! ([`replay`]; RFC 9449 §11.1). Caches, audit records, configuration and clinical data are
//! never kept here.
//!
//! Its tables live in the schema `ferrofed`. Each time a connection opens,
//! one transaction under a transaction-level advisory lock creates the
//! schema when absent and applies every migration of [`migrations`] the
//! database has not recorded, in order, so replicas starting together apply
//! each once
//! (<https://www.postgresql.org/docs/18/explicit-locking.html#ADVISORY-LOCKS>);
//! a database a newer FerroFED migrated is refused and left as it was. A
//! table whose rows expire carries the end of each row's window in an
//! `expires_at timestamptz` column, and [`Store::sweep`] deletes every row
//! past it. [`ready::Ready`] is the store's readiness indicator, down while the
//! database does not answer, and a call the database does not answer fails
//! with a typed error, so nothing that needs the store is answered without
//! it. No specification governs the store: our own design.

pub mod connection;
pub mod emergency;
pub mod migrations;
pub mod ready;
pub mod replay;

use std::sync::Arc;
use std::time::Duration;

use ferrofed_registry::definition::store::StoreError;
use tokio_postgres::Client;

use crate::config::store::StoreSettings;

use connection::{Connection, Statement, Target, Work, backend};

/// The schema every table of the store lives in.
pub const SCHEMA: &str = "ferrofed";

/// The name of the store's readiness indicator.
pub const INDICATOR: &str = "store";

/// The name of each connection's thread.
const THREAD: &str = "ferrofed-store";

/// The quoted names of the store's tables that carry `expires_at`.
// NOTE: no specification governs this: our own design; a table joins the sweep by
// its column, so a table with a window never needs a list of its own kept beside it.
const EXPIRING: &str = "SELECT quote_ident(c.relname) FROM pg_catalog.pg_class c
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
JOIN pg_catalog.pg_attribute a ON a.attrelid = c.oid
WHERE n.nspname = 'ferrofed' AND c.relkind = 'r' AND a.attname = 'expires_at'
AND NOT a.attisdropped AND a.atttypid = 'timestamptz'::regtype
ORDER BY 1";

/// FerroFED's own PostgreSQL store: connections to one database, migrated
/// when they open.
#[derive(Debug, Clone)]
pub struct Store {
    connection: Connection,
}

impl Store {
    /// Opens the connections `settings` describe and migrates the database
    /// ([`migrations`]).
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the connection string does not parse,
    /// the TLS setup fails, the database cannot be reached in time, or a
    /// migration fails or finds a database a newer FerroFED migrated.
    pub fn open(settings: &StoreSettings) -> Result<Self, StoreError> {
        let target = Target::new(&settings.url, settings.ca.as_deref(), settings.timeout)?;
        Connection::pool(&target, THREAD, prepare, settings.max_connections)
            .map(|connection| Self { connection })
    }

    /// Runs `work` on a connection where a thread may block, and waits for
    /// its answer.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time or
    /// the connections have stopped, and every error `work` returns.
    pub(crate) fn call<T: Send + 'static>(&self, work: impl Work<T>) -> Result<T, StoreError> {
        self.connection.call(work)
    }

    /// Runs `work` on a connection from a task, and awaits its answer.
    ///
    /// # Errors
    ///
    /// The errors of [`Store::call`].
    pub(crate) async fn call_async<T: Send + 'static>(
        &self,
        work: impl Work<T>,
    ) -> Result<T, StoreError> {
        self.connection.call_async(work).await
    }

    /// Asks the database to answer one statement.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when it does not answer in time, or answers
    /// with an error.
    pub async fn ping(&self) -> Result<(), StoreError> {
        self.call_async(|client| {
            Box::pin(async move {
                client
                    .simple_query("SELECT 1")
                    .await
                    .map(drop)
                    .map_err(backend)
            })
        })
        .await
    }

    /// Deletes every row past its window from every table of the store that
    /// carries `expires_at`, and returns how many rows each lost, by table.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the database does not answer in time, or
    /// a statement fails.
    pub async fn sweep(&self) -> Result<Vec<(String, u64)>, StoreError> {
        self.call_async(|client| Box::pin(sweep(client))).await
    }
}

/// Runs [`Store::sweep`] every `every` until the task is dropped, logging
/// each sweep that deleted a row and each one that failed.
pub async fn sweeping(store: Arc<Store>, every: Duration) {
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        match store.sweep().await {
            Ok(swept) => {
                for (table, rows) in swept.iter().filter(|(_, rows)| *rows > 0) {
                    tracing::debug!(table, rows, "the store's expired rows were deleted");
                }
            }
            Err(error) => {
                tracing::warn!(
                    error = crate::chain(&error),
                    "the store's expired rows could not be deleted"
                );
            }
        }
    }
}

/// The sweep, on one connection.
async fn sweep(client: &Client) -> Result<Vec<(String, u64)>, StoreError> {
    let tables = client.query(EXPIRING, &[]).await.map_err(backend)?;
    let mut swept = Vec::with_capacity(tables.len());
    for row in tables {
        let table: String = row.try_get(0).map_err(backend)?;
        let rows = client
            .execute(
                &format!("DELETE FROM ferrofed.{table} WHERE expires_at <= now()"),
                &[],
            )
            .await
            .map_err(backend)?;
        swept.push((table, rows));
    }
    Ok(swept)
}

/// [`migrations::apply`], as a connection runs it each time it opens.
fn prepare(client: &mut Client) -> Statement<'_, ()> {
    Box::pin(migrations::apply(client))
}
