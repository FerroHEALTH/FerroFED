// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every handler shares: the health registry.

use crate::config::Settings;
use crate::health::Registry;

// TODO(#36): register the indicator that the registry snapshot is loaded.
// TODO(#34): register one reachability indicator per node endpoint.
// TODO(#42): register the indicator that the identity source answers.

/// The state the router is built over.
#[derive(Debug, Default)]
pub struct AppState {
    /// The indicators readiness runs.
    health: Registry,
}

impl AppState {
    /// Returns the state `settings` describes.
    ///
    /// No subsystem with an indicator exists yet, so the registry is empty and
    /// readiness answers `200` for a process that serves what it serves.
    #[must_use]
    pub fn build(settings: &Settings) -> Self {
        settings.log_summary();
        Self::with_health(Registry::default())
    }

    /// Returns a state with `health` as its registry.
    #[must_use]
    pub const fn with_health(health: Registry) -> Self {
        Self { health }
    }

    /// Returns the indicators readiness runs.
    #[must_use]
    pub const fn health(&self) -> &Registry {
        &self.health
    }
}
