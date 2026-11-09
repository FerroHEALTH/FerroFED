// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Connections to a PostgreSQL database, each on a thread of its own with a
//! runtime of its own, reopened when they close.
//!
//! A store is called where a thread may block (`Connection::call`) and
//! from a request's task (`Connection::call_async`); either way the call
//! is sent to one of the connections' threads, which take the calls from one
//! queue in turn, the shape of the Rust book's thread pool
//! (<https://doc.rust-lang.org/book/ch21-02-multithreaded.html>). Each time a
//! connection opens it runs the store's `prepare` before any statement. The
//! client is `tokio-postgres` over rustls. No specification governs this:
//! our own design.

use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use ferrohealth_secret::secret_url::SecretUrl;
use rustls::ClientConfig;
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use rustls_platform_verifier::BuilderVerifierExt;
use tokio::runtime::Runtime;
use tokio_postgres::{Client, Config};
use tokio_postgres_rustls::MakeRustlsConnect;

use ferrofed_registry::definition::store::StoreError;

/// How long a connection, or one statement, may take, where the store sets
/// no budget of its own.
// NOTE: no specification governs this: our own design; a store that does not
// answer fails the one request loudly rather than holding a blocking thread.
pub const BUDGET: Duration = Duration::from_secs(10);

/// A failure of a connection's own, beside the client's errors.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConnectionError {
    /// The connection string does not parse.
    ///
    /// The parser's message is not kept: it may quote a character of the
    /// string, which holds a password.
    #[error("the PostgreSQL connection string does not parse")]
    Unparsable,
    /// The CA certificates do not read as PEM.
    #[error("the CA certificates of the PostgreSQL store do not read")]
    Ca,
    /// Every thread of the connections has stopped, so nothing reaches the
    /// database.
    #[error("the PostgreSQL store's threads have stopped")]
    Stopped,
    /// The database did not answer within the budget.
    #[error("the PostgreSQL database did not answer within {}s", .0.as_secs())]
    Timeout(Duration),
}

/// One call into a connection's thread, which sends its own answer.
type Job = Box<dyn FnOnce(&mut Worker) + Send>;

/// One statement's future, borrowing the connection it runs on.
pub(crate) type Statement<'a, T> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, StoreError>> + 'a>>;

/// What a caller runs on a connection: one statement's future, borrowing
/// the client.
pub(crate) trait Work<T>:
    for<'c> FnOnce(&'c Client) -> Statement<'c, T> + Send + 'static
{
}

impl<T, W> Work<T> for W where W: for<'c> FnOnce(&'c Client) -> Statement<'c, T> + Send + 'static {}

/// What a connection runs each time it connects, before any statement: the
/// schema created when absent and its layout migrated.
pub(crate) type Prepare = for<'c> fn(&'c mut Client) -> Statement<'c, ()>;

/// Where the connections go: the database, the TLS they reach it over, and
/// the budget of each statement.
#[derive(Clone)]
pub(crate) struct Target {
    config: Config,
    tls: MakeRustlsConnect,
    budget: Duration,
}

impl Target {
    /// The database `url` names, its certificate verified by the PEM CA
    /// certificates `ca` or, when `ca` is `None`, by the platform's roots.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when `url` does not parse, `ca` holds no
    /// certificate that reads, or the TLS setup fails.
    pub(crate) fn new(
        url: &SecretUrl,
        ca: Option<&str>,
        budget: Duration,
    ) -> Result<Self, StoreError> {
        let config = Config::from_str(url.expose())
            .map_err(|_quoted| backend(ConnectionError::Unparsable))?;
        Ok(Self {
            config,
            tls: tls(ca)?,
            budget,
        })
    }
}

/// Connections to a PostgreSQL database, each on a thread of its own.
#[derive(Debug, Clone)]
pub(crate) struct Connection {
    jobs: Sender<Job>,
}

impl Connection {
    /// Connects once to the database `url` names on a thread named `name`,
    /// verifying its certificate against the platform's roots, running
    /// `prepare` first.
    ///
    /// # Errors
    ///
    /// The errors of [`Connection::pool`].
    pub(crate) fn open(
        url: &SecretUrl,
        name: &'static str,
        prepare: Prepare,
    ) -> Result<Self, StoreError> {
        Self::pool(
            &Target::new(url, None, BUDGET)?,
            name,
            prepare,
            NonZeroUsize::MIN,
        )
    }

    /// Opens `size` connections to `target`, each on a thread named `name`
    /// and each running `prepare` first, so a database that cannot be reached
    /// or prepared refuses the open.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when a thread cannot be started, the database
    /// cannot be reached in time, or `prepare` fails.
    pub(crate) fn pool(
        target: &Target,
        name: &'static str,
        prepare: Prepare,
        size: NonZeroUsize,
    ) -> Result<Self, StoreError> {
        let (jobs, received) = mpsc::channel::<Job>();
        let received = Arc::new(Mutex::new(received));
        let (ready, opened) = mpsc::sync_channel(size.get());
        for _ in 0..size.get() {
            let (target, received, ready) = (target.clone(), Arc::clone(&received), ready.clone());
            thread::Builder::new()
                .name(String::from(name))
                .spawn(move || match Worker::start(target, prepare) {
                    Ok(mut worker) => {
                        if ready.send(Ok(())).is_ok() {
                            serve(&received, &mut worker);
                        }
                    }
                    Err(error) => {
                        if ready.send(Err(error)).is_err() {
                            tracing::debug!("a PostgreSQL store's opener stopped waiting");
                        }
                    }
                })
                .map_err(backend)?;
        }
        drop(ready);
        for _ in 0..size.get() {
            opened
                .recv()
                .map_err(|_stopped| backend(ConnectionError::Stopped))??;
        }
        Ok(Self { jobs })
    }

    /// Runs `work` on a connection's thread and waits for its answer.
    ///
    /// # Errors
    ///
    /// [`StoreError::Backend`] when every thread has stopped, the database
    /// does not answer in time, and every error `work` returns.
    pub(crate) fn call<T: Send + 'static>(&self, work: impl Work<T>) -> Result<T, StoreError> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.send(job(work, move |answered| reply.send(answered).is_ok()))?;
        answer
            .recv()
            .map_err(|_stopped| backend(ConnectionError::Stopped))?
    }

    /// Runs `work` on a connection's thread and awaits its answer, without
    /// blocking the task's thread.
    ///
    /// # Errors
    ///
    /// The errors of [`Connection::call`].
    pub(crate) async fn call_async<T: Send + 'static>(
        &self,
        work: impl Work<T>,
    ) -> Result<T, StoreError> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.send(job(work, move |answered| reply.send(answered).is_ok()))?;
        answer
            .await
            .map_err(|_stopped| backend(ConnectionError::Stopped))?
    }

    /// Queues `job` for the next free connection.
    fn send(&self, job: Job) -> Result<(), StoreError> {
        self.jobs
            .send(job)
            .map_err(|_stopped| backend(ConnectionError::Stopped))
    }
}

/// The call that runs `work` on a connection and hands its answer to
/// `reply`, which says whether the caller still waits.
fn job<T: Send + 'static>(
    work: impl Work<T>,
    reply: impl FnOnce(Result<T, StoreError>) -> bool + Send + 'static,
) -> Job {
    Box::new(move |worker: &mut Worker| {
        if !reply(worker.run(work)) {
            tracing::debug!("a PostgreSQL store caller stopped waiting");
        }
    })
}

/// Runs the calls `received` hands this thread, one at a time, until every
/// [`Connection`] is dropped.
fn serve(received: &Mutex<Receiver<Job>>, worker: &mut Worker) {
    loop {
        let job = match received.lock() {
            Ok(queue) => queue.recv(),
            Err(_poisoned) => return,
        };
        match job {
            Ok(job) => job(worker),
            Err(_closed) => return,
        }
    }
}

/// A connection's thread: a runtime, and the connection it reopens when it
/// closes.
pub(crate) struct Worker {
    runtime: Runtime,
    target: Target,
    prepare: Prepare,
    client: Option<Client>,
}

impl Worker {
    /// Builds the runtime and connects once, so a store that cannot be
    /// reached refuses the start.
    fn start(target: Target, prepare: Prepare) -> Result<Self, StoreError> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(backend)?;
        let client = runtime.block_on(within(target.budget, connect(&target, prepare)))?;
        Ok(Self {
            runtime,
            target,
            prepare,
            client: Some(client),
        })
    }

    /// Runs `work` on an open connection, reconnecting first when the last
    /// one closed, and drops a connection that did not answer in time.
    fn run<T>(&mut self, work: impl FnOnce(&Client) -> Statement<'_, T>) -> Result<T, StoreError> {
        let budget = self.target.budget;
        let client = match self.client.take() {
            Some(client) if !client.is_closed() => client,
            Some(_) | None => self
                .runtime
                .block_on(within(budget, connect(&self.target, self.prepare)))?,
        };
        let answer = self.runtime.block_on(within(budget, work(&client)));
        let timed_out = matches!(
            &answer,
            Err(StoreError::Backend(error))
                if matches!(error.downcast_ref(), Some(ConnectionError::Timeout(_)))
        );
        if !timed_out {
            self.client = Some(client);
        }
        answer
    }
}

/// `future`, failed with [`ConnectionError::Timeout`] past `budget`.
async fn within<T>(
    budget: Duration,
    future: impl Future<Output = Result<T, StoreError>>,
) -> Result<T, StoreError> {
    tokio::time::timeout(budget, future)
        .await
        .map_err(|_elapsed| backend(ConnectionError::Timeout(budget)))?
}

/// Opens a connection to `target`, drives it on the thread's runtime, and
/// runs `prepare` on it.
async fn connect(target: &Target, prepare: Prepare) -> Result<Client, StoreError> {
    let (mut client, connection) = target
        .config
        .connect(target.tls.clone())
        .await
        .map_err(backend)?;
    tokio::spawn(async move {
        if let Err(error) = connection.await {
            tracing::warn!(error = %error, "a PostgreSQL store's connection closed");
        }
    });
    prepare(&mut client).await?;
    Ok(client)
}

/// The TLS connector: rustls over aws-lc-rs, verifying the server against
/// the PEM CA certificates `ca`, or the platform's roots when it is `None`.
fn tls(ca: Option<&str>) -> Result<MakeRustlsConnect, StoreError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(backend)?;
    let config = match ca {
        Some(pem) => {
            let mut roots = rustls::RootCertStore::empty();
            for certificate in CertificateDer::pem_slice_iter(pem.as_bytes()) {
                let certificate = certificate.map_err(|_unread| backend(ConnectionError::Ca))?;
                roots.add(certificate).map_err(backend)?;
            }
            builder.with_root_certificates(roots).with_no_client_auth()
        }
        None => builder
            .with_platform_verifier()
            .map_err(backend)?
            .with_no_client_auth(),
    };
    Ok(MakeRustlsConnect::new(config))
}

/// The backend failure `error`.
pub(crate) fn backend(error: impl std::error::Error + Send + Sync + 'static) -> StoreError {
    StoreError::Backend(Box::new(error))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use ferrofed_registry::definition::store::StoreError;
    use ferrofed_testkit::listener::ListenerCertificates;
    use ferrohealth_secret::secret_url::SecretUrl;

    use super::{Connection, ConnectionError, Statement, Target, tls};

    /// A prepare that is never reached when the open fails first.
    fn prepared(_client: &mut tokio_postgres::Client) -> Statement<'_, ()> {
        Box::pin(async { Ok(()) })
    }

    /// The connection error inside `error`.
    fn connection_error(error: &StoreError) -> Option<&ConnectionError> {
        match error {
            StoreError::Backend(source) => source.downcast_ref::<ConnectionError>(),
            _ => None,
        }
    }

    #[test]
    fn the_ca_is_read_as_pem_and_anything_else_is_refused() {
        let certificates = ListenerCertificates::generate().unwrap();
        assert!(tls(Some(certificates.ca())).is_ok());
        assert!(tls(None).is_ok(), "the platform's roots");
        let refused = tls(Some(
            "-----BEGIN CERTIFICATE-----\nnot base64 at all\n-----END CERTIFICATE-----\n",
        ))
        .err()
        .unwrap();
        assert!(matches!(refused, StoreError::Backend(_)), "{refused:?}");
    }

    #[test]
    fn an_unparsable_url_is_refused_without_quoting_it() {
        let url = SecretUrl::new(String::from("postgres://u@h:x/d?password=example-s3cr3t"));
        let refused = Target::new(&url, None, Duration::from_secs(1))
            .err()
            .unwrap();
        assert!(
            matches!(
                connection_error(&refused),
                Some(ConnectionError::Unparsable)
            ),
            "{refused:?}"
        );
        assert!(!format!("{refused} {refused:?}").contains("s3cr3t"));
    }

    #[test]
    fn a_pool_whose_database_cannot_be_reached_refuses_the_open() {
        let url = SecretUrl::new(String::from(
            "postgres://ferrofed@127.0.0.1:1/ferrofed?sslmode=require&connect_timeout='1ms'",
        ));
        let target = Target::new(&url, None, Duration::from_secs(2)).unwrap();
        let size = NonZeroUsize::new(3).unwrap();
        let refused = Connection::pool(&target, "ferrofed-store-test", prepared, size)
            .err()
            .unwrap();
        assert!(matches!(refused, StoreError::Backend(_)), "{refused:?}");
    }
}
