// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-20 audit trails: one recorder per spool for the life of the
//! process (ITI TF-2 §3.20.4.1.1).
//!
//! The XCPD localizer sends its ITI-55 audit messages over syslog to
//! `[xcpd.audit_repository]` ([`trail`]); the PIXm resolver, the mCSD
//! directory and the PMIR identity feed send their BALP records over the
//! FHIR Feed to `[audit.repository]` ([`recorder`]). A registry reload
//! builds a new federation, and with it new clients, over the same
//! repositories (a change to either takes a restart). The new clients record
//! through the trails the boot started, so one forwarder drains each spool,
//! in order, whatever the reloads. The trails are kept here, keyed by their
//! spool, and the metrics read their depth. No specification governs the
//! process model: our own design.

pub mod config;
pub mod repository;

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use ferrofed_identity::atna::RepositoryAudit;
use ferrofed_identity::balp::{FeedAudit, FeedConfigError, LogFeedAudit, feed_repository};
use ihe_iti::atna::forwarder::{Forwarder, Status};
use ihe_iti::atna::repository::{Repository, RepositoryError, TlsSettings};
use ihe_iti::atna::spool::{Content, Spool, SpoolError};
use ihe_iti::balp::AuditRecorder;

use crate::binding::ihe::audit::config::{AuditSettings, FeedRepositorySettings};
use crate::binding::ihe::audit::repository::AuditRepositorySettings;
use crate::binding::ihe::xcpd::AuditDestination;
use crate::binding::seam::{Indication, Indicator};
use crate::health::dependencies::Observed;
use crate::service::{self, TlsRefused};

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

/// The ITI-20 syslog trail of the XCPD localizer, indicated as
/// `audit_repository`.
#[derive(Debug)]
pub struct RepositoryTrail(pub Arc<RepositoryAudit>);

impl Indicator for RepositoryTrail {
    fn indicate(&self) -> Vec<(&'static str, Indication)> {
        vec![(
            "audit_repository",
            Indication::State(observed(&self.0.status())),
        )]
    }
}

/// The FHIR Feed trail of the PIXm, PDQm, mCSD and PMIR audit records,
/// indicated as `audit_feed`.
#[derive(Debug)]
pub struct FeedTrail(pub Arc<FeedAudit>);

impl Indicator for FeedTrail {
    fn indicate(&self) -> Vec<(&'static str, Indication)> {
        vec![("audit_feed", Indication::State(observed(&self.0.status())))]
    }
}

/// Every trail the process started, by its spool.
static TRAILS: LazyLock<Mutex<BTreeMap<String, Trail>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// One running trail, with the settings it was started over.
enum Trail {
    /// The ITI-55 syslog trail of the XCPD localizer.
    Syslog {
        settings: AuditRepositorySettings,
        recorder: Arc<RepositoryAudit>,
    },
    /// The BALP FHIR Feed trail of the FHIR profiles.
    Feed {
        settings: FeedRepositorySettings,
        recorder: Arc<FeedAudit>,
    },
}

impl Trail {
    fn status(&self) -> Status {
        match self {
            Self::Syslog { recorder, .. } => recorder.status(),
            Self::Feed { recorder, .. } => recorder.status(),
        }
    }
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
    /// The spool could not be opened.
    #[error("the audit spool cannot be used")]
    Spool(#[source] SpoolError),
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

/// The trail `settings` describe: the one already running over the same
/// spool, or a new one, started when a Tokio runtime is running.
///
/// # Errors
///
/// An [`AuditTrailError`] for a repository or spool that cannot be used,
/// and for a spool another configuration already sends through.
pub fn trail(settings: &AuditRepositorySettings) -> Result<Arc<RepositoryAudit>, AuditTrailError> {
    let key = settings.spool_dir.as_ref().map_or_else(
        || format!("memory:{}", settings.url),
        |directory| directory.display().to_string(),
    );
    let mut trails = TRAILS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(running) = trails.get(&key) {
        return match running {
            Trail::Syslog {
                settings: started,
                recorder,
            } if started == settings => {
                recorder.start();
                Ok(Arc::clone(recorder))
            }
            _ => Err(AuditTrailError::InUse { spool: key }),
        };
    }
    let repository = if settings.unencrypted {
        Repository::unencrypted_for_development(&settings.url, settings.timeouts)
    } else {
        let tls = TlsSettings {
            roots: settings
                .trust_roots
                .as_ref()
                .map(|pem| pem.as_bytes().to_vec()),
            identity: settings
                .client_identity
                .as_ref()
                .map(ferrofed_registry::secret::Secret::to_secret_string),
        };
        Repository::tls(&settings.url, &tls, settings.timeouts)
    }
    .map_err(AuditTrailError::Repository)?;
    let spool = match &settings.spool_dir {
        Some(directory) => {
            Spool::open(directory, settings.bounds).map_err(AuditTrailError::Spool)?
        }
        None => Spool::in_memory(settings.bounds),
    };
    let recorder = Arc::new(RepositoryAudit::new(
        Forwarder::new(spool, repository, settings.retry_max),
        settings.sender.clone(),
        settings.source.clone(),
    ));
    recorder.start();
    trails.insert(
        key,
        Trail::Syslog {
            settings: settings.clone(),
            recorder: Arc::clone(&recorder),
        },
    );
    Ok(recorder)
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
    let mut trails = TRAILS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(running) = trails.get(&key) {
        return match running {
            Trail::Feed {
                settings: started,
                recorder,
            } if started == settings => {
                recorder.start();
                Ok(Arc::clone(recorder))
            }
            _ => Err(AuditTrailError::InUse { spool: key }),
        };
    }
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
    let spool = match &settings.spool_dir {
        Some(directory) => Spool::open_for(directory, settings.bounds, Content::AuditEvents)
            .map_err(AuditTrailError::Spool)?,
        None => Spool::in_memory(settings.bounds),
    };
    let recorder = Arc::new(FeedAudit::new(
        Forwarder::fhir_feed(spool, repository, settings.retry_max),
        settings.observer.clone(),
    ));
    recorder.start();
    trails.insert(
        key,
        Trail::Feed {
            settings: settings.clone(),
            recorder: Arc::clone(&recorder),
        },
    );
    Ok(recorder)
}

/// The recorder of the PIXm, PDQm, mCSD and PMIR audit records `settings`
/// describe: the FHIR Feed trail, the audit log target, or `None` when the
/// records are off.
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

/// The FHIR Feed trail `settings` send to, when the records go to a
/// repository.
///
/// # Errors
///
/// The [`AuditTrailError`] of a trail that cannot start.
pub fn feed(settings: &AuditSettings) -> Result<Option<Arc<FeedAudit>>, AuditTrailError> {
    match (settings.destination, &settings.repository) {
        (AuditDestination::Repository, Some(repository)) => feed_trail(repository).map(Some),
        _ => Ok(None),
    }
}

/// The status of every trail the process started, in spool order.
#[must_use]
pub fn statuses() -> Vec<Status> {
    TRAILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .map(Trail::status)
        .collect()
}
