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

pub mod repository;

use std::sync::Arc;

use ferrofed_identity::ihe::audit::atna::RepositoryAudit;
use ihe_atna::atna::forwarder::{Forwarder, Status};
use ihe_atna::atna::repository::{Repository, TlsSettings};
use ihe_atna::atna::spool::Content;

use crate::audit::{AuditTrailError, Forwarding, Stopping, observed, spool, started};
use crate::binding::ihe::audit::repository::AuditRepositorySettings;
use crate::binding::seam::Indicator;
use ferrofed_registry::health::Indication;

impl Forwarding for RepositoryAudit {
    fn start(&self) {
        RepositoryAudit::start(self);
    }

    fn status(&self) -> Status {
        RepositoryAudit::status(self)
    }

    fn stop(&self) -> Stopping<'_> {
        Box::pin(RepositoryAudit::stop(self))
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
    started(key, settings, || {
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
        let spool = spool(
            "xcpd.audit_repository.spool_dir",
            settings.spool_dir.as_deref(),
            settings.bounds,
            Content::SyslogFrames,
        )?;
        Ok(RepositoryAudit::new(
            Forwarder::new(spool, repository, settings.retry_max),
            settings.sender.clone(),
            settings.source.clone(),
        ))
    })
}
