// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every handler shares: the health registry and the federation.

use crate::config::settings::Settings;
use crate::federation::{Federation, FederationError};
use crate::health::Registry;

// TODO(#36): register the indicator that the registry snapshot is loaded.
// TODO(#34): register one reachability indicator per node endpoint.
// TODO(#42): register the indicator that the identity source answers.

/// The state the router is built over.
#[derive(Debug, Default)]
pub struct AppState {
    /// The indicators readiness runs.
    health: Registry,
    /// The federation the ITS-REST façade queries, when a registry is set.
    federation: Option<Federation>,
}

impl AppState {
    /// Returns the state `settings` describe.
    ///
    /// No subsystem with an indicator exists yet, so the health registry is
    /// empty and readiness answers `200` for a process that serves what it
    /// serves.
    ///
    /// # Errors
    /// Returns a [`FederationError`] when the federation `settings` describe
    /// cannot be built.
    pub fn build(settings: &Settings) -> Result<Self, FederationError> {
        settings.log_summary();
        let federation = Federation::load(settings)?;
        Ok(Self {
            health: Registry::default(),
            federation,
        })
    }

    /// Returns a state with `health` as its registry and no federation.
    #[must_use]
    pub const fn with_health(health: Registry) -> Self {
        Self {
            health,
            federation: None,
        }
    }

    /// Returns a state that serves the federated query over `federation`.
    #[must_use]
    pub fn with_federation(federation: Federation) -> Self {
        Self {
            health: Registry::default(),
            federation: Some(federation),
        }
    }

    /// Returns the indicators readiness runs.
    #[must_use]
    pub const fn health(&self) -> &Registry {
        &self.health
    }

    /// Returns the federation, when the gateway federates.
    #[must_use]
    pub const fn federation(&self) -> Option<&Federation> {
        self.federation.as_ref()
    }
}
