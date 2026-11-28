// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE binding's own ITI-20 audit trail: the XCPD localizer's ITI-55
//! audit messages, sent over syslog to `[xcpd.audit_repository]` ([`trail`];
//! ITI TF-2 §3.20.4.1.1).
//!
//! The PIXm, PDQm, mCSD and PMIR records go through the `[audit]` trail
//! every build carries ([`crate::audit`]), beside the access records. The
//! syslog trail is started over the same per-spool registry, so one
//! forwarder drains each spool whatever the reloads. No specification
//! governs the process model: our own design.

use std::sync::{Arc, Mutex, PoisonError};

use ferrofed_identity::ihe::audit::atna::RepositoryAudit;
use ferrohealth_server::revocation::{Clearing, RevocationCheck};
use ihe_atna::atna::forwarder::{Forwarder, Status};
use ihe_atna::atna::repository::Repository;
use ihe_atna::atna::spool::Content;

use crate::audit::{AuditTrailError, Forwarding, Stopping, observed, spool, started};
use crate::config::ihe::audit_repository::AuditRepositorySettings;
use crate::health::dependencies::Indicator;
use ferrofed_operator_api::health::Indication;

/// The ITI-20 syslog trail of the XCPD localizer: its recorder, and under
/// `trust_roots_file` what keeps the repository cleared for revocation while
/// the trail delivers.
#[derive(Debug)]
pub struct SyslogTrail {
    audit: Arc<RepositoryAudit>,
    check: Option<(RevocationCheck, url::Url)>,
    clearing: Mutex<Option<Clearing>>,
}

impl SyslogTrail {
    /// The recorder the localizer writes its audit messages to.
    #[must_use]
    pub fn audit(&self) -> &Arc<RepositoryAudit> {
        &self.audit
    }
}

impl Forwarding for SyslogTrail {
    fn start(&self) {
        if let Some((check, endpoint)) = &self.check {
            let mut clearing = self.clearing.lock().unwrap_or_else(PoisonError::into_inner);
            if clearing.is_none() {
                *clearing = crate::revocation::keep(check, vec![endpoint.clone()]);
            }
        }
        self.audit.delivery().start();
    }

    fn status(&self) -> Status {
        self.audit.delivery().status()
    }

    fn stop(&self) -> Stopping<'_> {
        Box::pin(async move {
            self.audit.delivery().stop().await;
            let clearing = self
                .clearing
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .take();
            drop(clearing);
        })
    }
}

/// The ITI-20 syslog trail of the XCPD localizer, indicated as
/// `audit_repository`.
#[derive(Debug)]
pub struct RepositoryTrail(pub Arc<SyslogTrail>);

impl Indicator for RepositoryTrail {
    fn indicate(&self) -> Vec<(&'static str, Indication)> {
        vec![(
            "audit_repository",
            Indication::State(observed(&self.0.status())),
        )]
    }
}

/// The trail `settings` describe: the one already running over the same
/// spool, or a new one, started when a Tokio runtime is running.
///
/// # Errors
///
/// An [`AuditTrailError`] for a repository or spool that cannot be used,
/// and for a spool another configuration already sends through.
pub fn trail(settings: &AuditRepositorySettings) -> Result<Arc<SyslogTrail>, AuditTrailError> {
    let key = settings.spool_dir.as_ref().map_or_else(
        || format!("memory:{}", settings.url),
        |directory| directory.display().to_string(),
    );
    started(key, settings, || {
        let (repository, check) = if settings.unencrypted {
            (
                Repository::unencrypted_for_development(&settings.url, settings.timeouts),
                None,
            )
        } else {
            let tls = settings
                .tls
                .platform_config()
                .map_err(AuditTrailError::Tls)?;
            // NOTE: no specification governs this: our own design; the check reaches the
            // repository's TLS endpoint by an https URL at its host and port.
            let check = tls
                .check()
                .cloned()
                .zip(crate::revocation::handshake(&settings.url));
            (
                Repository::tls(
                    &settings.url,
                    Arc::new(tls.into_config()),
                    settings.timeouts,
                ),
                check,
            )
        };
        let repository = repository.map_err(AuditTrailError::Repository)?;
        let spool = spool(
            "xcpd.audit_repository.spool_dir",
            settings.spool_dir.as_deref(),
            settings.bounds,
            Content::SyslogFrames,
        )?;
        Ok(SyslogTrail {
            audit: Arc::new(RepositoryAudit::new(
                Forwarder::new(spool, repository, settings.retry_max),
                settings.sender.clone(),
                settings.source.clone(),
            )),
            check,
            clearing: Mutex::new(None),
        })
    })
}
