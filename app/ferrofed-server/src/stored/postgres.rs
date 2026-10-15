// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The shared store: one PostgreSQL database every gateway replica uses, so
//! a version one replica stores is held for all of them (§12.7, N44).
//!
//! The definitions live in one table, `ferrofed.stored_query_definition`,
//! whose primary key is the qualified name and the version. An insert is
//! `INSERT … ON CONFLICT (name, version) DO NOTHING`, and the affected-row
//! count says whether it stored the row or found the pair held, so two
//! replicas inserting one pair at once store exactly one of them, in one
//! statement, with no read before the write
//! (<https://www.postgresql.org/docs/18/sql-insert.html#SQL-ON-CONFLICT>).
//! A definition stored with no version takes the trait's own
//! [`DefinitionStore::insert_assigned`]: it reads the versions of its name
//! and inserts the next with that statement, and when another replica stored
//! that version first it reads again, a bounded number of times, so two
//! replicas racing on one name store two versions or one is refused, and
//! neither replaces the other's (§12.7 stored-query-versioning). Each time the store connects, one transaction under a transaction-level
//! advisory lock creates the schema when absent, reads the schema version
//! [`VERSIONS`] records ([`schema`]), runs every migration past it in order
//! and records the version it reached, so replicas starting together do not
//! race on the catalogue
//! (<https://www.postgresql.org/docs/18/explicit-locking.html#ADVISORY-LOCKS>).
//! A database a newer FerroFED migrated is refused and left as it was.
//!
//! The client is `tokio-postgres`, on a thread of the store's own with a
//! runtime of its own, because [`DefinitionStore`] is called where a thread
//! may block. TLS is rustls with the platform's roots, as the node client
//! uses, and the connection string's `sslmode` decides whether it is
//! required. No specification governs the storage: our own design.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use ferrofed_registry::definition::store::{
    DefinitionStore, Field, FieldFault, Insertion, StoreError,
};
use ferrofed_registry::definition::{QueryName, StoredDefinition};
use ferrofed_registry::secret::SecretUrl;
use jiff::Timestamp;
use rustls::ClientConfig;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::runtime::Runtime;
use tokio_postgres::config::{Host, SslMode};
use tokio_postgres::{Client, Config, Row};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::stored::schema::{self, SchemaError};

/// The schema the store's table lives in.
pub const SCHEMA: &str = "ferrofed";

/// The table, qualified by [`SCHEMA`].
pub const TABLE: &str = "ferrofed.stored_query_definition";

/// The table that records the schema version of each layout, qualified by
/// [`SCHEMA`].
pub const VERSIONS: &str = "ferrofed.schema_version";

/// The key [`VERSIONS`] records the definitions' layout under.
pub const LAYOUT: &str = "stored_query_definition";

/// Takes the lock one replica migrates under, creates the schema when
/// absent, and the table of schema versions.
///
/// It runs inside the transaction [`prepare`] opens, so the lock is held
/// until that transaction ends. The schema is created only when the
/// catalogue lacks it, so a role without `CREATE` on the database can use a
/// schema made for it.
const LOCK: &str = "SELECT pg_advisory_xact_lock(x'666572726f666564'::bigint);
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname = 'ferrofed') THEN
    CREATE SCHEMA ferrofed;
  END IF;
END $$;
CREATE TABLE IF NOT EXISTS ferrofed.schema_version (
  layout text PRIMARY KEY,
  version integer NOT NULL
);";

/// The schema version the definitions' layout records.
const SELECT_VERSION: &str = "SELECT version FROM ferrofed.schema_version WHERE layout = $1";

/// Records the schema version a migration reached.
const RECORD_VERSION: &str = "INSERT INTO ferrofed.schema_version (layout, version) VALUES ($1, $2)
ON CONFLICT (layout) DO UPDATE SET version = EXCLUDED.version";

/// Version 1: the definitions table. A database written before versions were
/// recorded holds it already.
const MIGRATION_1: &str = "CREATE TABLE IF NOT EXISTS ferrofed.stored_query_definition (
  name text NOT NULL,
  version text NOT NULL,
  saved text NOT NULL,
  aql text NOT NULL,
  PRIMARY KEY (name, version)
);";

/// Stores a definition unless its name and version are held.
const INSERT: &str = "INSERT INTO ferrofed.stored_query_definition (name, version, saved, aql)
VALUES ($1, $2, $3, $4) ON CONFLICT (name, version) DO NOTHING";

/// Every definition.
const SELECT_ALL: &str = "SELECT name, version, saved, aql FROM ferrofed.stored_query_definition";

/// Every version of one name.
const SELECT_NAMED: &str =
    "SELECT name, version, saved, aql FROM ferrofed.stored_query_definition WHERE name = $1";

/// How long a connection, or one statement, may take.
// NOTE: no specification governs this: our own design; a store that does not
// answer fails the one request loudly rather than holding a blocking thread.
const BUDGET: Duration = Duration::from_secs(10);

/// A failure of the store's own, beside the client's errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PostgresError {
    /// The connection string does not parse.
    ///
    /// The parser's message is not kept: it may quote a character of the
    /// string, which holds a password.
    #[error("the PostgreSQL connection string does not parse")]
    Unparsable,
    /// The store's thread has stopped, so nothing reaches the database.
    #[error("the PostgreSQL store's thread has stopped")]
    Stopped,
    /// The database did not answer within the budget.
    #[error("the PostgreSQL database did not answer within {}s", .0.as_secs())]
    Timeout(Duration),
    /// An insert reported a row count other than one or none.
    #[error("an insert reported {0} rows, where one or none is possible")]
    Rows(u64),
}

/// The stored-query definitions in one shared PostgreSQL database.
#[derive(Debug)]
pub struct PostgresStore {
    connection: Connection,
}

/// One call into a connection's thread, which sends its own answer.
type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// What a connection runs each time it connects, before any statement: the
/// schema created when absent and its layout migrated.
pub(crate) type Prepare = for<'c> fn(&'c mut Client) -> Statement<'c, ()>;

/// A connection to a PostgreSQL database on a thread of its own, with a
/// runtime of its own, reopened when it closes.
///
/// A store over it is called where a thread may block, so each call is sent
/// to that thread and waited for.
#[derive(Debug)]
pub(crate) struct Connection {
    jobs: Sender<Job>,
}
/// Whether `url` parses as a PostgreSQL connection string, a URL or libpq
/// key/value pairs.
#[must_use]
pub fn parses(url: &SecretUrl) -> bool {
    // NOTE: no specification governs this: our own design; a parse failure is
    // the answer here, and its message may quote the secret.
    Config::from_str(url.expose()).is_ok()
}

/// Whether the connection `url` names sends a password over a network
/// without requiring TLS.
///
/// The driver reads `sslmode` as `disable`, `prefer` (its default) or
/// `require`, and refuses any other value, so only `require` encrypts for
/// certain; the connector then verifies the server against the platform's
/// roots. A connection whose every host is a Unix socket, with no `hostaddr`,
/// crosses no network.
/// A string that does not parse cannot be shown to require TLS, so it counts
/// as exposing its password.
#[must_use]
pub fn exposes_password(url: &SecretUrl) -> bool {
    let Ok(config) = Config::from_str(url.expose()) else {
        return true;
    };
    let networked = !config.get_hostaddrs().is_empty()
        || config
            .get_hosts()
            .iter()
            .any(|host| !matches!(host, Host::Unix(_)));
    config.get_password().is_some()
        && networked
        && !matches!(config.get_ssl_mode(), SslMode::Require)
}

impl Connection {
    /// Connects to the database `url` names on a thread named `name`,
    /// running `prepare` first, so a database that cannot be reached or
    /// prepared refuses the open.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when `url` does not parse, the TLS setup
    /// fails, the database cannot be reached in time, or `prepare` fails.
    pub(crate) fn open(
        url: &SecretUrl,
        name: &'static str,
        prepare: Prepare,
    ) -> Result<Self, StoreError> {
        let config =
            Config::from_str(url.expose()).map_err(|_quoted| backend(PostgresError::Unparsable))?;
        let tls = tls().map_err(backend)?;
        let (jobs, received) = mpsc::channel::<Job>();
        let (ready, opened) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name(String::from(name))
            .spawn(move || {
                let mut worker = match Worker::start(config, tls, prepare) {
                    Ok(worker) => worker,
                    Err(error) => {
                        if ready.send(Err(error)).is_err() {
                            tracing::debug!("a PostgreSQL store's opener stopped waiting");
                        }
                        return;
                    }
                };
                if ready.send(Ok(())).is_ok() {
                    for job in &received {
                        job(&mut worker);
                    }
                }
            })
            .map_err(backend)?;
        opened
            .recv()
            .map_err(|_stopped| backend(PostgresError::Stopped))??;
        Ok(Self { jobs })
    }

    /// Runs `work` on the connection's thread and waits for its answer.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when the thread has stopped, the database does
    /// not answer in time, and every error `work` returns.
    pub(crate) fn call<T, W>(&self, work: W) -> Result<T, StoreError>
    where
        T: Send + 'static,
        W: for<'c> FnOnce(&'c Client) -> Statement<'c, T> + Send + 'static,
    {
        let (reply, answer) = mpsc::sync_channel(1);
        let job: Job = Box::new(move |worker: &mut Worker| {
            if reply.send(worker.run(work)).is_err() {
                tracing::debug!("a PostgreSQL store caller stopped waiting");
            }
        });
        self.jobs
            .send(job)
            .map_err(|_stopped| backend(PostgresError::Stopped))?;
        answer
            .recv()
            .map_err(|_stopped| backend(PostgresError::Stopped))?
    }
}

impl PostgresStore {
    /// Connects to the database `url` names and creates the schema and the
    /// table when absent.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when `url` does not parse, the TLS setup
    /// fails, the database cannot be reached in time, or the schema cannot
    /// be created.
    pub fn open(url: &SecretUrl) -> Result<Self, StoreError> {
        Connection::open(url, "ferrofed-stored-queries", prepare_definitions)
            .map(|connection| Self { connection })
    }
}

impl DefinitionStore for PostgresStore {
    fn insert_if_absent(&self, definition: &StoredDefinition) -> Result<Insertion, StoreError> {
        let definition = definition.clone();
        self.connection
            .call(move |client| Box::pin(insert(client, definition)))
    }

    fn load(&self) -> Result<Vec<StoredDefinition>, StoreError> {
        self.connection
            .call(move |client| Box::pin(load(client, None)))
    }

    fn load_named(&self, name: &QueryName) -> Result<Vec<StoredDefinition>, StoreError> {
        let name = Some(name.clone());
        self.connection
            .call(move |client| Box::pin(load(client, name)))
    }

    fn is_shared(&self) -> bool {
        true
    }
}

/// A connection's thread: a runtime, and the connection it reopens when it
/// closes.
pub(crate) struct Worker {
    runtime: Runtime,
    config: Config,
    tls: MakeRustlsConnect,
    prepare: Prepare,
    client: Option<Client>,
}

impl Worker {
    /// Builds the runtime and connects once, so a store that cannot be
    /// reached refuses the start.
    fn start(config: Config, tls: MakeRustlsConnect, prepare: Prepare) -> Result<Self, StoreError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(backend)?;
        let client = runtime.block_on(within(connect(&config, &tls, prepare)))?;
        Ok(Self {
            runtime,
            config,
            tls,
            prepare,
            client: Some(client),
        })
    }

    /// Runs `work` on an open connection, reconnecting first when the last
    /// one closed, and drops a connection that did not answer in time.
    fn run<T>(&mut self, work: impl FnOnce(&Client) -> Statement<'_, T>) -> Result<T, StoreError> {
        let client = match self.client.take() {
            Some(client) if !client.is_closed() => client,
            Some(_) | None => {
                self.runtime
                    .block_on(within(connect(&self.config, &self.tls, self.prepare)))?
            }
        };
        let answer = self.runtime.block_on(within(work(&client)));
        let timed_out = matches!(
            &answer,
            Err(StoreError::Backend(error))
                if matches!(error.downcast_ref(), Some(PostgresError::Timeout(_)))
        );
        if !timed_out {
            self.client = Some(client);
        }
        answer
    }
}
/// One statement's future, borrowing the connection it runs on.
pub(crate) type Statement<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, StoreError>> + 'a>>;

/// `future`, failed with [`PostgresError::Timeout`] past [`BUDGET`].
async fn within<T>(future: impl Future<Output = Result<T, StoreError>>) -> Result<T, StoreError> {
    tokio::time::timeout(BUDGET, future)
        .await
        .map_err(|_elapsed| backend(PostgresError::Timeout(BUDGET)))?
}

/// Opens a connection, drives it on the thread's runtime, and runs
/// `prepare` on it.
async fn connect(
    config: &Config,
    tls: &MakeRustlsConnect,
    prepare: Prepare,
) -> Result<Client, StoreError> {
    let (mut client, connection) = config.connect(tls.clone()).await.map_err(backend)?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::warn!(error = %error, "a PostgreSQL store's connection closed");
        }
    });
    prepare(&mut client).await?;
    Ok(client)
}

/// [`prepare`], as a connection runs it.
fn prepare_definitions(client: &mut Client) -> Statement<'_, ()> {
    Box::pin(prepare(client))
}

/// Takes the lock one replica migrates under in `transaction`, and creates
/// the schema and the table of schema versions when absent.
///
/// # Errors
///
/// [`StoreError::Backend`] when a statement fails.
pub(crate) async fn lock(transaction: &tokio_postgres::Transaction<'_>) -> Result<(), StoreError> {
    transaction.batch_execute(LOCK).await.map_err(backend)
}

/// Creates the schema when absent and migrates the definitions' layout to
/// [`schema::CURRENT`], in one transaction under the advisory lock; a
/// refusal rolls it back, so a database a newer FerroFED migrated is left as
/// it was.
async fn prepare(client: &mut Client) -> Result<(), StoreError> {
    let transaction = client.transaction().await.map_err(backend)?;
    transaction.batch_execute(LOCK).await.map_err(backend)?;
    let found = transaction
        .query_opt(SELECT_VERSION, &[&LAYOUT])
        .await
        .map_err(backend)?
        .map(|row| row.try_get::<usize, i32>(0))
        .transpose()
        .map_err(fault(Field::Layout, FieldFault::Type))?
        .unwrap_or(0);
    let found = u32::try_from(found).map_err(fault(Field::Layout, FieldFault::Range))?;
    let pending = schema::pending("PostgreSQL", found).map_err(schema_error)?;
    for step in pending {
        transaction
            .batch_execute(migration(step)?)
            .await
            .map_err(backend)?;
        let reached = i32::try_from(step).map_err(fault(Field::Layout, FieldFault::Range))?;
        transaction
            .execute(RECORD_VERSION, &[&LAYOUT, &reached])
            .await
            .map_err(backend)?;
    }
    transaction.commit().await.map_err(backend)
}

/// The statements that bring the layout to version `step`.
fn migration(step: u32) -> Result<&'static str, StoreError> {
    match step {
        1 => Ok(MIGRATION_1),
        version => Err(schema_error(SchemaError::NoMigration {
            store: "PostgreSQL",
            version,
        })),
    }
}

/// The backend failure a schema refusal is.
fn schema_error(error: SchemaError) -> StoreError {
    StoreError::Backend(Box::new(error))
}

/// Inserts `definition` unless its name and version are held.
async fn insert(client: &Client, definition: StoredDefinition) -> Result<Insertion, StoreError> {
    let version = definition.version().to_string();
    let saved = definition.saved().to_string();
    let rows = client
        .execute(
            INSERT,
            &[
                &definition.name().as_str(),
                &version.as_str(),
                &saved.as_str(),
                &definition.aql(),
            ],
        )
        .await
        .map_err(backend)?;
    match rows {
        1 => Ok(Insertion::Stored),
        0 => Ok(Insertion::Held),
        other => Err(backend(PostgresError::Rows(other))),
    }
}

/// Every definition, or every version of `name`.
async fn load(
    client: &Client,
    name: Option<QueryName>,
) -> Result<Vec<StoredDefinition>, StoreError> {
    let rows = match &name {
        Some(name) => client.query(SELECT_NAMED, &[&name.as_str()]).await,
        None => client.query(SELECT_ALL, &[]).await,
    }
    .map_err(backend)?;
    rows.iter().map(definition).collect()
}

/// The definition one row holds.
fn definition(row: &Row) -> Result<StoredDefinition, StoreError> {
    let text = |(index, field): (usize, Field)| {
        row.try_get::<usize, &str>(index)
            .map_err(fault(field, FieldFault::Type))
    };
    Ok(StoredDefinition::new(
        text((0, Field::Name))?
            .parse()
            .map_err(fault(Field::Name, FieldFault::Form))?,
        text((1, Field::Version))?
            .parse()
            .map_err(fault(Field::Version, FieldFault::Form))?,
        text((3, Field::Aql))?.to_owned(),
        text((2, Field::Saved))?
            .parse::<Timestamp>()
            .map_err(fault(Field::Saved, FieldFault::Form))?,
    ))
}

/// The TLS connector: rustls over aws-lc-rs, verifying the server against
/// the platform's roots.
fn tls() -> Result<MakeRustlsConnect, rustls::Error> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_platform_verifier()?
        .with_no_client_auth();
    Ok(MakeRustlsConnect::new(config))
}

/// The backend failure `error`.
fn backend(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Backend(Box::new(error))
}

/// The damaged-store error of a held `field` with the fault `kind`, for any
/// reader error.
fn fault<E>(field: Field, kind: FieldFault) -> impl FnOnce(E) -> StoreError {
    // NOTE: no specification governs this: our own design; the reader's error
    // is dropped because it can quote the held text, and the fault names it.
    move |_| StoreError::unreadable(field, kind)
}

#[cfg(test)]
mod tests {
    use super::{MIGRATION_1, PostgresError, PostgresStore, migration, parses};
    use crate::stored::schema::{CURRENT, SchemaError};
    use ferrofed_registry::definition::store::StoreError;
    use ferrofed_registry::secret::SecretUrl;

    #[test]
    fn a_url_and_libpq_pairs_parse_and_garbage_does_not() {
        for parsed in [
            "postgres://ferrofed:secret@db.example.org:5432/ferrofed?sslmode=require",
            "host=db.example.org user=ferrofed password=secret dbname=ferrofed",
        ] {
            assert!(parses(&SecretUrl::new(parsed)), "{parsed}");
        }
        for refused in ["postgres://a:b@host:notaport/db", "host='unterminated"] {
            assert!(!parses(&SecretUrl::new(refused)), "{refused}");
        }
    }

    #[test]
    fn every_version_up_to_the_current_one_has_its_migration() {
        assert_eq!(MIGRATION_1, migration(1).unwrap());
        for version in 1..=CURRENT {
            assert!(migration(version).is_ok(), "version {version}");
        }
        let Err(StoreError::Backend(missing)) = migration(CURRENT + 1) else {
            panic!("no migration past the current version");
        };
        assert!(matches!(
            missing.downcast_ref::<SchemaError>(),
            Some(SchemaError::NoMigration { .. })
        ));
    }

    #[test]
    fn an_unparsable_url_is_refused_without_quoting_it() {
        let refused = PostgresStore::open(&SecretUrl::new("postgres://u:example-s3cr3t@h:x/d"));
        let Err(StoreError::Backend(error)) = refused else {
            panic!("refused: {refused:?}");
        };
        assert!(error.is::<PostgresError>(), "{error}");
        assert!(!error.to_string().contains("s3cr3t"), "{error}");
    }
}
