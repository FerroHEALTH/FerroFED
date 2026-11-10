// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The localizer an undirected patient query is planned with, and what the
//! gateway does when it does not answer (N4, N10, §14.1).
//!
//! Under `federation.node_selection = "localized"` exactly one localizer is
//! active, built from the configured bindings
//! ([`crate::binding`]): a binding's own localizer when one is configured,
//! such as the XCPD localizer of `[xcpd]` (Annex A.3) or the NVI localizer of
//! `[nl_gf.nvi]` (Annex B §B.1), and otherwise the resolver itself where it
//! localizes, such as the PIXm resolver of `[pixm]` (§14.2) or the static
//! development cross-reference under `profile = "development"`. Two
//! localizers of their own are refused, naming both sections. A configured
//! localizer that does not answer fails closed unless
//! `[federation.localization] on_failure = "ask-all"` declares otherwise, and
//! `OPTIONS {base}/` declares the policy either way (§7a.2, N30). Under
//! `node_selection = "ask-all"` there is no localizer and every member is a
//! candidate (§4.3, N4 last sentence).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::role::localizer::{Localizer, OnFailure};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::binding::seam::{LocalizationError, LocalizerSeam, ResolverSeam};
use crate::binding::{self, Offer, Role};
use crate::config::settings::{LocalizationSettings, Settings};
use crate::config::{self, NodeSelection};
use crate::health::dependencies::Indicator;

/// The localizer of a federation, with its failure policy and budget.
#[derive(Clone)]
pub struct LocalizationPolicy {
    localizer: Option<Arc<dyn Localizer>>,
    mode: Option<&'static str>,
    audit: Option<&'static str>,
    indicators: Vec<Arc<dyn Indicator>>,
    on_failure: OnFailure,
    timeout: Duration,
}

impl LocalizationPolicy {
    /// The health indicators of what the localizer records through, such as
    /// the ATNA Audit Record Repository its audit messages go to.
    #[must_use]
    pub fn indicators(&self) -> &[Arc<dyn Indicator>] {
        &self.indicators
    }

    /// The policy of a deployment with no localizer: every member is a
    /// candidate, and `OPTIONS {base}/` declares the default `closed`.
    #[must_use]
    pub fn none() -> Self {
        Self {
            localizer: None,
            mode: None,
            audit: None,
            indicators: Vec::new(),
            on_failure: OnFailure::Closed,
            timeout: Duration::ZERO,
        }
    }

    /// The policy of `localizer`, the binding `mode` names, which may take
    /// `timeout` and on failure does what `on_failure` says (§14.1).
    #[must_use]
    pub fn new(
        localizer: Arc<dyn Localizer>,
        mode: &'static str,
        on_failure: OnFailure,
        timeout: Duration,
    ) -> Self {
        Self {
            localizer: Some(localizer),
            mode: Some(mode),
            audit: None,
            indicators: Vec::new(),
            on_failure,
            timeout,
        }
    }

    /// The configured localizer, if any.
    #[must_use]
    pub fn localizer(&self) -> Option<&dyn Localizer> {
        self.localizer.as_deref()
    }

    /// The name of the configured binding, which `OPTIONS {base}/` declares
    /// as `localization.mode`.
    #[must_use]
    pub fn mode(&self) -> Option<&'static str> {
        self.mode
    }

    /// What the gateway does when the localizer does not answer.
    #[must_use]
    pub fn on_failure(&self) -> OnFailure {
        self.on_failure
    }

    /// How long the localizer may take; zero when none is configured.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// This policy, its localizer's exchanges audited to `audit`, which
    /// `OPTIONS {base}/` declares as `localization.audit`.
    #[must_use]
    pub fn audited(mut self, audit: &'static str) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Where the localizer's audit messages go, when it records any.
    #[must_use]
    pub fn audit(&self) -> Option<&'static str> {
        self.audit
    }
}

impl fmt::Debug for LocalizationPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalizationPolicy")
            .field("mode", &self.mode)
            .field("audit", &self.audit)
            .field("on_failure", &self.on_failure)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// The localization policy `settings` declare under `selection` over the
/// members of `snapshot`, from the roles the bindings `offers` and the
/// resolver the federation runs, `resolving`.
///
/// The localizer is a binding's own when one offers it (one at most, which
/// the caller holds), and otherwise the resolver itself where it localizes:
/// the PIXm resolver names the members whose domain holds the patient over
/// the ITI-83 call its resolution reuses (§14.2), the development
/// cross-reference the members that hold a row.
///
/// # Errors
///
/// Returns [`LocalizationError::NoLocalizer`] for the localized selection
/// with no localizer, [`LocalizationError::NotLocalized`] and
/// [`LocalizationError::Unused`] for a localization table or a binding's own
/// localizer under the ask-all selection, and the binding's errors for a
/// table its localizer refuses.
pub fn policy(
    settings: &Settings,
    selection: NodeSelection,
    offers: &[Offer],
    resolving: Option<&ResolverSeam>,
    snapshot: &RegistrySnapshot,
) -> Result<LocalizationPolicy, LocalizationError> {
    let federation = &settings.federation;
    let own = offers.iter().find(|offer| offer.role == Role::Localizer);
    match selection {
        NodeSelection::AskAll if federation.localization.is_some() => {
            Err(LocalizationError::NotLocalized)
        }
        NodeSelection::AskAll if let Some(offer) = own => Err(LocalizationError::Unused {
            section: offer.section,
        }),
        NodeSelection::AskAll => Ok(LocalizationPolicy::none()),
        NodeSelection::Localized => {
            let declared = federation.localization.unwrap_or_else(|| {
                let default = config::Localization::default();
                LocalizationSettings {
                    on_failure: default.on_failure,
                    timeout: Duration::from_millis(default.timeout.as_millis()),
                }
            });
            let seam =
                match own {
                    Some(_) => binding::localizer(settings, snapshot)?,
                    None => resolving.and_then(|seam| seam.localizer.clone()).map(
                        |(localizer, mode)| LocalizerSeam {
                            localizer,
                            mode,
                            audit: None,
                            indicators: Vec::new(),
                        },
                    ),
                };
            let Some(seam) = seam else {
                return Err(LocalizationError::NoLocalizer);
            };
            Ok(LocalizationPolicy {
                localizer: Some(seam.localizer),
                mode: Some(seam.mode),
                audit: seam.audit,
                indicators: seam.indicators,
                on_failure: declared.on_failure,
                timeout: declared.timeout,
            })
        }
    }
}
