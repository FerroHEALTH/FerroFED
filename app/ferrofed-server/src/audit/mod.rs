// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The audit trail every build carries, `[audit]`.
//!
//! It holds the BALP `AuditEvent` recorder the access log stores each access
//! record through (Regulation (EU) 2025/327 Annex II 3.2), and the spools
//! the recorders deliver from.
//!
//! The access record is due from every deployment whatever its regional
//! binding, so the `[audit]` table, its FHIR Feed trail to an ATNA Audit
//! Record Repository (BALP §1:52.1.1.1) and the log target are built here,
//! with no binding. The IHE binding records its own transactions through the
//! same `[audit]` recorder, and its XCPD localizer starts a syslog trail of
//! its own over [`started`].
//!
//! One recorder runs per spool for the life of the process (ITI TF-2
//! §3.20.4.1.1). A registry reload builds a new federation over the same
//! repositories, and a change to either takes a restart, so one forwarder
//! drains each spool, in order, whatever the reloads. The trails are kept
//! here, keyed by their spool, and the metrics read their depth. No
//! specification governs where the code lives: our own design.

pub mod config;
pub mod metrics;

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use ehds_logging::balp::BalpSink;
use ehds_logging::sink::AccessSink;
use ferrofed_identity::ihe::audit::balp::{
    FeedAudit, FeedConfigError, LogFeedAudit, feed_repository,
};
use ferrofed_registry::health::{Indication, Observed};
use ihe_iti::atna::forwarder::{Forwarder, Status};
use ihe_iti::atna::repository::RepositoryError;
use ihe_iti::atna::spool::{Bounds, Content, Spool, SpoolError};
use ihe_iti::balp::AuditRecorder;
use serde::Deserialize;

use crate::audit::config::{AuditSettings, FeedRepositorySettings};
use crate::binding::seam::Indicator;
use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, ProtectedSite};
use crate::service::{self, TlsRefused};

/// Where audit records go: the `destination` of `[audit]`, and the `audit`
/// of the IHE binding's `[xcpd]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditDestination {
    /// The Audit Record Repository the table's own repository section
    /// names.
    Repository,
    /// A structured event at the `ferrofed::audit` log target, without a
    /// patient identifier, for a deployment that routes its log to its audit
    /// repository.
    Log,
    /// No record: development only.
    Off,
}

impl AuditDestination {
    /// The value as the configuration and `OPTIONS {base}/` spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Log => "log",
            Self::Off => "off",
        }
    }
}

/// Returns what an audit forwarder's `status` says of its repository.
///
/// It is [`Observed::Degraded`] while the forwarder retries a failed
/// delivery, while messages wait in the spool, and while any sits in
/// quarantine; [`Observed::Unknown`] before anything was sent; and
/// [`Observed::Up`] otherwise.
#[must_use]
pub fn observed(status: &Status) -> Observed {
    if !status.reachable || status.depth.messages > 0 {
        Observed::Degraded
    } else if status.delivered == 0 {
        Observed::Unknown
    } else {
        Observed::Up
    }
}

/// A recorder that delivers from a spool: the FHIR Feed trail here, and a
/// binding's own, such as the IHE binding's ITI-55 syslog trail.
pub trait Forwarding: Send + Sync + 'static {
    /// Starts delivering the spooled records on the running Tokio runtime,
    /// unless a delivery runs already.
    fn start(&self);

    /// What the spool holds and how the deliveries went.
    fn status(&self) -> Status;
}

impl Forwarding for FeedAudit {
    fn start(&self) {
        FeedAudit::start(self);
    }

    fn status(&self) -> Status {
        FeedAudit::status(self)
    }
}

/// The FHIR Feed trail of `[audit]`, indicated as `audit_feed`.
#[derive(Debug)]
pub struct FeedTrail(pub Arc<FeedAudit>);

impl Indicator for FeedTrail {
    fn indicate(&self) -> Vec<(&'static str, Indication)> {
        vec![("audit_feed", Indication::State(observed(&self.0.status())))]
    }
}

/// One running trail: the settings it was started over and its recorder.
struct Started<S, R> {
    settings: S,
    recorder: Arc<R>,
}

/// A running trail of any kind, as the process keeps it.
trait Running: Send {
    /// What its spool holds and how its deliveries went.
    fn status(&self) -> Status;

    /// The trail, to read back as the kind it was started as.
    fn as_any(&self) -> &dyn Any;
}

impl<S: Send + 'static, R: Forwarding> Running for Started<S, R> {
    fn status(&self) -> Status {
        self.recorder.status()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Every trail the process started, by its spool.
static TRAILS: LazyLock<Mutex<BTreeMap<String, Box<dyn Running>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

thread_local! {
    /// The spools a `config check` on this thread built a trail over, while
    /// one runs ([`Checking`]).
    static CHECKED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

/// A `config check` running on the calling thread.
///
/// While it is held, a trail over a spool directory checks the directory in
/// place ([`Spool::inspect`]) and holds its messages in memory, creating and
/// writing nothing, and starts no forwarder. The trails it built are
/// forgotten when it drops, so a later build opens the spool itself. No
/// specification governs this: our own design.
#[derive(Debug)]
pub struct Checking {
    // NOTE: no specification governs this: our own design; the scope is the
    // calling thread's, so the guard stays on it.
    thread: PhantomData<*const ()>,
}

impl Checking {
    /// Enters the check on the calling thread.
    #[must_use]
    pub fn enter() -> Self {
        CHECKED.with_borrow_mut(|checked| {
            checked.get_or_insert_with(Vec::new);
        });
        Self {
            thread: PhantomData,
        }
    }
}

impl Drop for Checking {
    fn drop(&mut self) {
        let built = CHECKED.with_borrow_mut(Option::take).unwrap_or_default();
        let mut trails = TRAILS.lock().unwrap_or_else(PoisonError::into_inner);
        for key in built {
            trails.remove(&key);
        }
    }
}

/// Whether a `config check` runs on the calling thread.
fn checking() -> bool {
    CHECKED.with_borrow(Option::is_some)
}

/// The spool of a trail at `directory`, which `key` names, or in memory
/// without one: opened, or, while a check runs, checked in place and held in
/// memory.
///
/// # Errors
///
/// [`AuditTrailError::Spool`] for a directory that cannot be used.
pub fn spool(
    key: &'static str,
    directory: Option<&Path>,
    bounds: Bounds,
    content: Content,
) -> Result<Spool, AuditTrailError> {
    let refused = |source| AuditTrailError::Spool { key, source };
    match directory {
        Some(directory) if checking() => {
            Spool::inspect(directory).map_err(refused)?;
            Ok(Spool::in_memory(bounds))
        }
        Some(directory) => Spool::open_for(directory, bounds, content).map_err(refused),
        None => Ok(Spool::in_memory(bounds)),
    }
}

/// The trail over the spool `key` names, started over `settings`: the one
/// already running there, or the one `build` makes, started when a Tokio
/// runtime is running.
///
/// # Errors
///
/// The [`AuditTrailError`] of `build`, and [`AuditTrailError::InUse`] for a
/// spool another configuration, or another kind of trail, already sends
/// through.
pub fn started<S, R>(
    key: String,
    settings: &S,
    build: impl FnOnce() -> Result<R, AuditTrailError>,
) -> Result<Arc<R>, AuditTrailError>
where
    S: Clone + PartialEq + Send + 'static,
    R: Forwarding,
{
    let mut trails = TRAILS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(running) = trails.get(&key) {
        return match running.as_any().downcast_ref::<Started<S, R>>() {
            Some(started) if started.settings == *settings => {
                if !checking() {
                    started.recorder.start();
                }
                Ok(Arc::clone(&started.recorder))
            }
            _ => Err(AuditTrailError::InUse { spool: key }),
        };
    }
    let recorder = Arc::new(build()?);
    if checking() {
        CHECKED.with_borrow_mut(|checked| {
            if let Some(built) = checked {
                built.push(key.clone());
            }
        });
    } else {
        recorder.start();
    }
    trails.insert(
        key,
        Box::new(Started {
            settings: settings.clone(),
            recorder: Arc::clone(&recorder),
        }),
    );
    Ok(recorder)
}

/// Why a trail could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuditTrailError {
    /// The repository address or its TLS settings were refused.
    #[error("the audit repository cannot be reached as configured")]
    Repository(#[source] RepositoryError),
    /// The FHIR Feed repository was refused.
    #[error("the audit repository cannot be reached as configured")]
    Feed(#[source] FeedConfigError),
    /// The FHIR Feed repository's TLS material does not read.
    #[error("the audit repository TLS material cannot be used")]
    Tls(#[source] TlsRefused),
    /// The spool could not be opened, or, in a check, would not open.
    #[error("{key} names an audit spool that cannot be used")]
    Spool {
        /// The key that names the spool directory.
        key: &'static str,
        /// Why the spool cannot be used.
        #[source]
        source: SpoolError,
    },
    /// `destination = "repository"` names no `[audit.repository]`.
    #[error("audit.destination = \"repository\" needs [audit.repository]")]
    NoRepository,
    /// Another configuration already sends through this spool.
    #[error(
        "the audit spool {spool} is in use by another audit repository configuration; a change to it takes a restart"
    )]
    InUse {
        /// The spool, as its key names it.
        spool: String,
    },
}

/// The FHIR Feed trail `settings` describe: the one already running over
/// the same spool, or a new one, started when a Tokio runtime is running.
///
/// # Errors
///
/// An [`AuditTrailError`] for a repository or spool that cannot be used,
/// and for a spool another configuration already sends through.
pub fn feed_trail(settings: &FeedRepositorySettings) -> Result<Arc<FeedAudit>, AuditTrailError> {
    let key = settings.spool_dir.as_ref().map_or_else(
        || format!("memory:{}", settings.url),
        |directory| directory.display().to_string(),
    );
    started(key, settings, || {
        let tls = service::tls(
            "audit.repository",
            settings.client_identity.as_ref(),
            settings.trust_roots.as_deref(),
        )
        .map_err(AuditTrailError::Tls)?;
        let repository = feed_repository(
            settings.url.clone(),
            settings.cleartext,
            &tls,
            settings.timeout,
        )
        .map_err(AuditTrailError::Feed)?;
        let spool = spool(
            "audit.repository.spool_dir",
            settings.spool_dir.as_deref(),
            settings.bounds,
            Content::AuditEvents,
        )?;
        Ok(FeedAudit::new(
            Forwarder::fhir_feed(spool, repository, settings.retry_max),
            settings.observer.clone(),
        ))
    })
}

/// The recorder of the BALP records `settings` describe, the access records
/// and the IHE binding's transactions alike: the FHIR Feed trail, the audit
/// log target, or `None` when the records are off.
///
/// # Errors
///
/// The [`AuditTrailError`] of a trail that cannot start.
pub fn recorder(
    settings: &AuditSettings,
) -> Result<Option<Arc<dyn AuditRecorder>>, AuditTrailError> {
    Ok(match (settings.destination, &settings.repository) {
        (AuditDestination::Repository, Some(repository)) => {
            let trail: Arc<dyn AuditRecorder> = feed_trail(repository)?;
            Some(trail)
        }
        (AuditDestination::Log, _) => Some(Arc::new(LogFeedAudit)),
        (AuditDestination::Repository, None) => return Err(AuditTrailError::NoRepository),
        (AuditDestination::Off, _) => None,
    })
}

/// The sink the access log stores its records through, a BALP `AuditEvent`
/// for each, when `settings` send the records anywhere (Regulation (EU)
/// 2025/327 Annex II 3.2).
///
/// # Errors
///
/// The [`AuditTrailError`] of a trail that cannot start.
pub fn access_sink(settings: &Settings) -> Result<Option<Arc<dyn AccessSink>>, AuditTrailError> {
    Ok(
        recorder(&settings.audit)?.map(|recorder| -> Arc<dyn AccessSink> {
            Arc::new(BalpSink::new(recorder, gateway(settings)))
        }),
    )
}

/// The health indicator of the FHIR Feed trail `settings` send to, when the
/// records go to a repository.
///
/// # Errors
///
/// The [`AuditTrailError`] of a trail that cannot start.
pub fn indicators(settings: &AuditSettings) -> Result<Vec<Arc<dyn Indicator>>, AuditTrailError> {
    Ok(match (settings.destination, &settings.repository) {
        (AuditDestination::Repository, Some(repository)) => {
            let trail: Arc<dyn Indicator> = Arc::new(FeedTrail(feed_trail(repository)?));
            vec![trail]
        }
        _ => Vec::new(),
    })
}

/// Holds the repository `settings` send to to the transport policy, and
/// returns it when it is reached in cleartext under the development profile.
///
/// # Errors
///
/// The [`CleartextError`] of a repository the policy refuses.
pub fn sites(settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
    let Some(repository) = &settings.audit.repository else {
        return Ok(Vec::new());
    };
    Ok(transport::protected_payload(
        settings.profile,
        repository.url.as_str(),
        ProtectedSite {
            url_key: String::from("audit.repository.url"),
            payload: String::from(
                "the access records and the IHE transactions' audit records, which name the patient",
            ),
            requires: transport::Encryption::Https,
        },
    )?
    .into_iter()
    .collect())
}

/// Whether the records `settings` send to a repository wait in a spool held
/// in memory, which a restart loses.
#[must_use]
pub fn spools_in_memory(settings: &AuditSettings) -> bool {
    settings
        .repository
        .as_ref()
        .is_some_and(|repository| repository.spool_dir.is_none())
}

/// The status of every trail the process started, in spool order.
#[must_use]
pub fn statuses() -> Vec<Status> {
    TRAILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .map(|trail| trail.status())
        .collect()
}

/// The URL the access records name the gateway by: the audience its callers'
/// tokens name it by, when that is a URL, and its listen address otherwise.
#[expect(
    clippy::expect_used,
    reason = "a socket address is an IP address, bracketed when IPv6, and a port, which always forms an http URL"
)]
fn gateway(settings: &Settings) -> url::Url {
    settings
        .server
        .auth
        .audience
        .as_deref()
        // NOTE: RFC 7519 §4.1.3: an audience is any string, so one that is no URL is
        // legitimately not one, and the listen address names the gateway instead.
        .and_then(|audience| url::Url::parse(audience).ok())
        .unwrap_or_else(|| {
            url::Url::parse(&format!("http://{}/", settings.server.listen))
                .expect("a socket address should form an http URL")
        })
}
