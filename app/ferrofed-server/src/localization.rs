// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The localizer an undirected patient query is planned with, and what the
//! gateway does when it does not answer (N4, N10, §14.1).
//!
//! Under `federation.node_selection = "localized"` exactly one localizer is
//! active. The static development cross-reference serves as one under
//! `profile = "development"`. A configured localizer that does not answer
//! fails closed unless `[federation.localization] on_failure = "ask-all"`
//! declares otherwise, and `OPTIONS {base}/` declares the policy either way
//! (§7a.2, N30). Under `node_selection = "ask-all"` there is no localizer and
//! every member is a candidate (§4.3, N4 last sentence).

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_identity::dev::StaticResolver;
use ferrofed_identity::localizer::{Localizer, OnFailure};

use crate::config::settings::{FederationSettings, LocalizationSettings};
use crate::config::{self, NodeSelection};

/// The localizer of a federation, with its failure policy and budget.
#[derive(Clone)]
pub struct LocalizationPolicy {
    localizer: Option<Arc<dyn Localizer>>,
    mode: Option<&'static str>,
    on_failure: OnFailure,
    timeout: Duration,
}

impl LocalizationPolicy {
    /// The policy of a deployment with no localizer: every member is a
    /// candidate, and `OPTIONS {base}/` declares the default `closed`.
    #[must_use]
    pub fn none() -> Self {
        Self {
            localizer: None,
            mode: None,
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
}

impl fmt::Debug for LocalizationPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalizationPolicy")
            .field("mode", &self.mode)
            .field("on_failure", &self.on_failure)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// The `localization.mode` of the static development cross-reference.
pub const DEVELOPMENT_STATIC: &str = "development-static";

/// A localization configuration that cannot be set up.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LocalizationError {
    /// `node_selection = "localized"` is declared and no localizer is
    /// configured, so no undirected patient query could find its node set
    /// (N4).
    #[error(
        "federation.node_selection = \"localized\" needs a localizer: the [dev] cross-reference serves as one under profile = \"development\" (N4, §14.1)"
    )]
    NoLocalizer,
    /// `[federation.localization]` is set under a node selection that uses
    /// no localizer.
    #[error(
        "[federation.localization] applies only under federation.node_selection = \"localized\"; remove it, or declare the localized selection"
    )]
    NotLocalized,
}

/// The localization policy `federation` declares under `selection`, over
/// the static development cross-reference `development` when one is
/// configured.
///
/// # Errors
///
/// Returns [`LocalizationError::NoLocalizer`] for the localized selection
/// with no localizer, and [`LocalizationError::NotLocalized`] for a
/// `[federation.localization]` table under the ask-all selection.
pub fn policy(
    federation: &FederationSettings,
    selection: NodeSelection,
    development: Option<Arc<StaticResolver>>,
) -> Result<LocalizationPolicy, LocalizationError> {
    match selection {
        NodeSelection::AskAll if federation.localization.is_some() => {
            Err(LocalizationError::NotLocalized)
        }
        NodeSelection::AskAll => Ok(LocalizationPolicy::none()),
        NodeSelection::Localized => {
            let declared = federation.localization.unwrap_or_else(|| {
                let default = config::Localization::default();
                LocalizationSettings {
                    on_failure: default.on_failure,
                    timeout: Duration::from_millis(default.timeout_ms),
                }
            });
            let localizer: Arc<dyn Localizer> =
                development.ok_or(LocalizationError::NoLocalizer)?;
            Ok(LocalizationPolicy::new(
                localizer,
                DEVELOPMENT_STATIC,
                declared.on_failure,
                declared.timeout,
            ))
        }
    }
}
