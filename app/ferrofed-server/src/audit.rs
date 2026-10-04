// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITI-20 audit trail of the XCPD localizer: one recorder per spool for
//! the life of the process (ITI TF-2 §3.20.4.1.1).
//!
//! A registry reload builds a new federation, and with it a new localizer,
//! over the same `[xcpd.audit_repository]` (a change to it takes a restart).
//! The new localizer records through the trail the boot started, so one
//! forwarder drains each spool, in order, whatever the reloads. The trails
//! are kept here, keyed by their spool, and the metrics read their depth.
//! No specification governs the process model: our own design.

use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use ferrofed_identity::atna::RepositoryAudit;
use ihe_iti::atna::forwarder::{Forwarder, Status};
use ihe_iti::atna::repository::{Repository, RepositoryError, TlsSettings};
use ihe_iti::atna::spool::{Spool, SpoolError};

use crate::config::audit_repository::AuditRepositorySettings;

/// Every trail the process started, by its spool.
static TRAILS: LazyLock<Mutex<BTreeMap<String, Trail>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// One running trail, with the settings it was started over.
struct Trail {
    settings: AuditRepositorySettings,
    recorder: Arc<RepositoryAudit>,
}

/// Why a trail could not be started.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AuditTrailError {
    /// The repository address or its TLS settings were refused.
    #[error("the audit repository cannot be reached as configured")]
    Repository(#[source] RepositoryError),
    /// The spool could not be opened.
    #[error("the audit spool cannot be used")]
    Spool(#[source] SpoolError),
    /// Another configuration already sends through this spool.
    #[error(
        "the audit spool {spool} is in use by another [xcpd.audit_repository]; a change to it takes a restart"
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
        if running.settings != *settings {
            return Err(AuditTrailError::InUse { spool: key });
        }
        running.recorder.start();
        return Ok(Arc::clone(&running.recorder));
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
        Trail {
            settings: settings.clone(),
            recorder: Arc::clone(&recorder),
        },
    );
    Ok(recorder)
}

/// The status of every trail the process started, in spool order.
#[must_use]
pub fn statuses() -> Vec<Status> {
    TRAILS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .values()
        .map(|trail| trail.recorder.status())
        .collect()
}
