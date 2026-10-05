// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The IHE binding of Annex A.
//!
//! It holds the PIXm resolver (`[pixm]`, Annex A.1), the PDQm demographics
//! step (`[pdqm]`, §A.2), the XCPD localizer (`[xcpd]`, Annex A.3), the PMIR
//! identity feed (`[pmir]`, Annex A.4), the mCSD care services directory
//! (`[registry.mcsd]`, Annex A.5), and the ATNA and BALP audit trail every
//! one of them records through (`[audit]`, `[xcpd.audit_repository]`).
//!
//! The profiles are one binding: they share one audit spool and forwarder,
//! the PMIR feed is scoped by the domains `[pixm]` maps, and the PDQm step is
//! refused for a namespace `[pixm.namespaces]` maps. No specification governs
//! the grouping: our own design.

pub mod audit;
pub mod mcsd;
pub mod metrics;
pub mod pdqm;
pub mod pixm;
pub mod pmir;
pub mod process;
pub mod xcpd;

use std::sync::Arc;

use ferrofed_identity::directory::error::FhirFormError;
use ferrofed_identity::directory::mcsd::DirectoryReadError;
use ferrofed_identity::localizer::Localizer;
use ferrofed_identity::resolver::Resolver;
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::ihe::mcsd::registry::DirectoryFailure;
use crate::binding::seam::{Indicator, LocalizerSeam, ResolverSeam};
use crate::binding::{Binding, Offer, Reload, Role, Section, StepBudgets};
use crate::config::Config;
use crate::config::error::Error;
use crate::config::settings::Settings;
use crate::config::transport::{self, CleartextError, ProtectedSite};
use crate::federation::DemographicsStep;
use crate::federation::error::FederationError;
use crate::localization::LocalizationError;

/// The `localization.mode` of the XCPD localizer (Annex A.3).
pub const XCPD: &str = "xcpd";

/// The `localization.mode` of the PIXm localizer (§14.2, Annex A.1).
pub const PIXM: &str = "pixm";

/// The IHE binding.
#[derive(Debug, Clone, Copy)]
pub struct Ihe;

/// The sections the IHE binding reads.
const SECTIONS: &[Section] = &[
    Section {
        key: "pixm",
        reload: Reload::Applies,
    },
    Section {
        key: "pdqm",
        reload: Reload::Applies,
    },
    Section {
        key: "xcpd",
        reload: Reload::Applies,
    },
    Section {
        key: "registry.mcsd",
        reload: Reload::Restart,
    },
    Section {
        key: "xcpd.audit",
        reload: Reload::Restart,
    },
    Section {
        key: "audit",
        reload: Reload::Restart,
    },
    Section {
        key: "pmir",
        reload: Reload::Restart,
    },
];

impl Binding for Ihe {
    fn name(&self) -> &'static str {
        "ihe"
    }

    fn sections(&self) -> &'static [Section] {
        SECTIONS
    }

    fn resolve(&self, config: &Config, settings: &mut Settings) -> Result<(), Error> {
        settings.pixm = config
            .pixm
            .as_ref()
            .map(|section| pixm::resolve(section, config.profile))
            .transpose()?;
        settings.xcpd = xcpd::resolve(config)?;
        settings.pmir = pmir::config::resolve(config)?;
        let audit = audit::config::resolve(config)?;
        if config.registry.document.is_some() && config.registry.mcsd.is_some() {
            return Err(Error::TwoRegistrySources);
        }
        settings.registry_directory = config
            .registry
            .mcsd
            .as_ref()
            .map(|directory| mcsd::resolve(directory, config.profile, audit.clone()))
            .transpose()?;
        settings.audit = audit;
        settings.pdqm = pdqm::resolve(config)?;
        Ok(())
    }

    fn budgets(&self, config: &Config) -> StepBudgets {
        StepBudgets {
            demographics_ms: config.pdqm.as_ref().map_or(0, |pdqm| pdqm.timeout_ms),
            consent_ms: 0,
        }
    }

    fn offers(&self, settings: &Settings) -> Vec<Offer> {
        let offered = [
            (settings.pixm.is_some(), Role::Resolver, "[pixm]"),
            (settings.pdqm.is_some(), Role::Demographics, "[pdqm]"),
            (settings.xcpd.is_some(), Role::Localizer, "[xcpd]"),
            (
                settings.registry_directory.is_some(),
                Role::RegistrySource,
                "[registry.mcsd]",
            ),
            (settings.pmir.is_some(), Role::IdentityFeed, "[pmir]"),
        ];
        offered
            .into_iter()
            .filter_map(|(set, role, section)| set.then_some(Offer { role, section }))
            .collect()
    }

    fn localizers(&self) -> (&'static [&'static str], &'static [&'static str]) {
        (&["[xcpd]"], &["[pixm]"])
    }

    fn resolvers(&self) -> &'static [&'static str] {
        &["[pixm]"]
    }

    fn unregistered(&self, settings: &Settings) -> Result<(), FederationError> {
        if settings.pixm.is_some() {
            return Err(FederationError::PixmWithoutRegistry);
        }
        if settings.pdqm.is_some() {
            return Err(FederationError::PdqmWithoutResolver);
        }
        Ok(())
    }

    fn resolver(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<ResolverSeam>, FederationError> {
        let Some(section) = &settings.pixm else {
            return Ok(None);
        };
        let resolver = pixm::resolver(section, &settings.audit, snapshot)?;
        let localizer: Arc<dyn Localizer> = resolver.clone();
        let resolver: Arc<dyn Resolver> = resolver;
        Ok(Some(ResolverSeam {
            resolver,
            localizer: Some((localizer, PIXM)),
        }))
    }

    fn demographics(
        &self,
        settings: &Settings,
        resolving: bool,
    ) -> Result<Option<DemographicsStep>, FederationError> {
        let Some(pdqm) = &settings.pdqm else {
            return Ok(None);
        };
        let step = pdqm::step(pdqm, &settings.audit)?;
        if !resolving {
            return Err(FederationError::PdqmWithoutResolver);
        }
        Ok(Some(step))
    }

    fn localizer(
        &self,
        settings: &Settings,
        snapshot: &RegistrySnapshot,
    ) -> Result<Option<LocalizerSeam>, LocalizationError> {
        settings
            .xcpd
            .as_ref()
            .map(|section| xcpd::localizer(section, settings.profile, snapshot))
            .transpose()
    }

    fn indicators(&self, settings: &Settings) -> Result<Vec<Arc<dyn Indicator>>, FederationError> {
        let feed = audit::feed(&settings.audit).map_err(FederationError::Audit)?;
        Ok(feed
            .into_iter()
            .map(|trail| -> Arc<dyn Indicator> { Arc::new(audit::FeedTrail(trail)) })
            .collect())
    }

    fn read_registry(
        &self,
        settings: &Settings,
    ) -> Option<Result<RegistrySnapshot, FederationError>> {
        settings
            .registry_directory
            .as_ref()
            .map(mcsd::registry::read)
    }

    fn sites(&self, settings: &Settings) -> Result<Vec<ProtectedSite>, CleartextError> {
        let profile = settings.profile;
        let mut cleartext = Vec::new();
        let mut hold = |url: &str, site: ProtectedSite| {
            transport::protected_payload(profile, url, site)
                .map(|exposed| cleartext.extend(exposed))
        };
        for (index, manager) in settings
            .pixm
            .iter()
            .flat_map(|pixm| pixm.managers.iter().enumerate())
        {
            let key = format!("pixm.manager[{index}]");
            let credentials = manager
                .credentials
                .is_some()
                .then(|| format!("{key}.credentials"));
            hold(
                manager.url.expose(),
                transport::identity_site(&key, credentials.as_deref()),
            )?;
        }
        if let Some(pdqm) = &settings.pdqm {
            let key = pdqm::PDQM_KEY;
            let credentials = pdqm
                .credentials
                .is_some()
                .then(|| format!("{key}.credentials"));
            hold(
                pdqm.url.expose(),
                transport::identity_site(key, credentials.as_deref()),
            )?;
        }
        let mut audit = None;
        if let Some(xcpd) = &settings.xcpd {
            let assertion = xcpd.assertion.as_ref().map(|_| xcpd.assertion_key);
            for (index, gateway) in xcpd.gateways.iter().enumerate() {
                let key = format!("xcpd.gateway[{index}]");
                hold(
                    gateway.url.expose(),
                    transport::identity_site(&key, assertion),
                )?;
            }
            if let Some(repository) = &xcpd.audit_repository {
                let site = ProtectedSite {
                    url_key: String::from("xcpd.audit_repository.url"),
                    payload: String::from("the ITI-55 audit messages, which name the patient"),
                    requires: transport::Encryption::SyslogTls,
                };
                audit = transport::encrypted_syslog(profile, &repository.url, site)?;
            }
        }
        if let Some(directory) = settings
            .registry_directory
            .as_ref()
            .filter(|directory| directory.credentials.is_some())
        {
            hold(directory.url.expose(), mcsd::site())?;
        }
        if let Some(repository) = &settings.audit.repository {
            hold(
                repository.url.as_str(),
                ProtectedSite {
                    url_key: String::from("audit.repository.url"),
                    payload: String::from(
                        "the PIXm, PDQm, mCSD and PMIR audit records, which name the patient",
                    ),
                    requires: transport::Encryption::Https,
                },
            )?;
        }
        cleartext.extend(pmir::config::sites(profile, settings.pmir.as_ref())?);
        cleartext.extend(audit);
        Ok(cleartext)
    }

    fn effective(&self, boot: &Settings, fresh: &mut Settings) {
        // NOTE: no specification governs this: our own design; one forwarder
        // drains each audit spool, so where the audit messages go takes a restart.
        if let (Some(xcpd), Some(started)) = (fresh.xcpd.as_mut(), &boot.xcpd) {
            xcpd.audit = started.audit;
            xcpd.audit_repository.clone_from(&started.audit_repository);
        }
        // NOTE: no specification governs this: our own design; the identity feed
        // outlives every federation a reload builds, so a change to it takes a restart.
        fresh.pmir = None;
        fresh.audit.clone_from(&boot.audit);
    }

    fn needs_restart(&self, boot: &Settings, fresh: &Settings) -> Vec<&'static str> {
        let directory = match (&boot.registry_directory, &fresh.registry_directory) {
            (Some(was), Some(now)) => !was.same_as(now),
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        };
        let xcpd_audit = match (&boot.xcpd, &fresh.xcpd) {
            (Some(was), Some(now)) => {
                was.audit != now.audit || was.audit_repository != now.audit_repository
            }
            _ => false,
        };
        let pmir = match (&boot.pmir, &fresh.pmir) {
            (Some(was), Some(now)) => !was.same_as(now),
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
        };
        [
            ("registry.mcsd", directory),
            ("xcpd.audit", xcpd_audit),
            ("audit", boot.audit != fresh.audit),
            ("pmir", pmir),
        ]
        .into_iter()
        .filter_map(|(key, changed)| changed.then_some(key))
        .collect()
    }

    fn class(&self, error: &FederationError) -> Option<&'static str> {
        match error {
            FederationError::PixmWithoutRegistry
            | FederationError::PixmMember { .. }
            | FederationError::PixmNamespace(_)
            | FederationError::Pixm(_) => Some("pixm"),
            FederationError::PdqmWithoutResolver
            | FederationError::PdqmNamespace(_)
            | FederationError::Pdqm(_) => Some("pdqm"),
            FederationError::Audit(_) => Some("audit"),
            FederationError::FhirRegistry { source, .. } => Some(match **source {
                FhirFormError::Read { .. } => "registry-unreadable",
                _ => "registry-invalid",
            }),
            FederationError::Directory(failure) => Some(match &**failure {
                DirectoryFailure::Read(DirectoryReadError::Exchange(error)) if error.exceeded() => {
                    "registry-budget"
                }
                DirectoryFailure::Read(DirectoryReadError::Exchange(_)) => "registry-unreadable",
                DirectoryFailure::Read(_) => "registry-invalid",
                _ => "registry-directory",
            }),
            _ => None,
        }
    }

    fn spools_in_memory(&self, settings: &Settings) -> bool {
        settings.xcpd.as_ref().is_some_and(|xcpd| {
            xcpd.audit_repository
                .as_ref()
                .is_some_and(|repository| repository.spool_dir.is_none())
        }) || settings
            .audit
            .repository
            .as_ref()
            .is_some_and(|repository| repository.spool_dir.is_none())
    }

    fn log_summary(&self, settings: &Settings) {
        tracing::info!(
            binding = self.name(),
            registry_directory = settings.registry_directory.is_some(),
            registry_refresh_s = settings
                .registry_directory
                .as_ref()
                .map(|directory| directory.refresh_interval.as_secs()),
            pix_managers = settings.pixm.as_ref().map_or(0, |pixm| pixm.managers.len()),
            pmir_feed = settings.pmir.is_some(),
            pdqm_transaction = settings.pdqm.as_ref().map(|pdqm| pdqm.transaction.as_str()),
            xcpd_localizer = settings.xcpd.is_some(),
            "binding configured"
        );
    }
}
