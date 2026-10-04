// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the bindings hold for the life of the process, beside every
//! federation a reload builds.
//!
//! A binding that records through its own instruments, runs a process of its
//! own, such as an identity feed, or reads the registry from a source of its
//! own, such as a care services directory, adds one field here, under its
//! feature; the server reaches each through the generic methods. No
//! specification governs the process model: our own design.

use std::sync::Arc;

use axum::Router;
use ferrofed_registry::snapshot::RegistrySnapshot;
use opentelemetry::metrics::Meter;

use crate::binding::{Indication, ihe};
use crate::config::settings::Settings;
use crate::federation::error::FederationError;
use crate::reload::Reloader;
use crate::state::{AppState, StateError};

/// The instruments the bindings record through on the metrics surface, one
/// field per binding that has any, created once over the surface's meter.
#[derive(Debug)]
pub struct Instruments {
    /// The IHE binding's identity feed and audit spool instruments.
    pub(crate) ihe: ihe::metrics::Instruments,
}

impl Instruments {
    /// Returns every binding's instruments over `meter`.
    pub(crate) fn new(meter: &Meter) -> Self {
        Self {
            ihe: ihe::metrics::Instruments::new(meter),
        }
    }
}

/// What the bindings run for the life of the process, beside every
/// federation a reload builds: one field per binding that runs anything.
#[derive(Debug, Default)]
pub struct Processes {
    /// The IHE binding's care services directory and identity feed.
    pub(crate) ihe: ihe::process::Processes,
}

impl Processes {
    /// Returns what `settings` describe for the bindings to run.
    ///
    /// # Errors
    ///
    /// The [`StateError`] of a process that cannot be built.
    pub(crate) fn build(settings: &Settings) -> Result<Self, StateError> {
        Ok(Self {
            ihe: ihe::process::Processes::build(settings)?,
        })
    }

    /// Takes over the registry sources the boot opened.
    pub(crate) fn watch(&mut self, sources: Sources) {
        if let Some(directory) = sources.ihe {
            self.ihe.directory = Some(directory);
        }
    }

    /// Returns what each process indicates on `GET /health/dependencies`.
    pub(crate) fn indicate(&self) -> Vec<(&'static str, Indication)> {
        self.ihe.indicate()
    }

    /// Returns `surface` with the routes the processes serve.
    pub(crate) fn routes(&self, surface: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
        self.ihe.routes(surface)
    }

    /// Starts every process, `reloader` replacing the registry a source
    /// changes.
    pub(crate) fn start(&self, reloader: &Arc<Reloader>) -> Running {
        Running {
            ihe: self.ihe.start(reloader),
        }
    }
}

/// The processes a running gateway started.
#[derive(Debug)]
pub struct Running {
    /// The IHE binding's identity feed subscription.
    ihe: ihe::process::Running,
}

impl Running {
    /// Stops every process that holds something at a remote service.
    pub(crate) async fn drain(self) {
        self.ihe.drain().await;
    }
}

/// The registry sources a binding opened at boot, which the gateway keeps
/// its registry in step with.
#[derive(Debug, Default)]
pub struct Sources {
    /// The IHE binding's care services directory.
    ihe: Option<Arc<ihe::mcsd::registry::DirectoryRegistry>>,
}

/// Returns the registry's first read and, when a binding is its source, the
/// source the gateway then keeps it in step with.
///
/// A document is read as [`read_registry`](crate::federation::registry::read_registry)
/// reads it; a source is read once here, so the banner, the build and the
/// refreshes share one read.
#[must_use]
pub fn read_source(
    settings: &Settings,
) -> (Option<Result<RegistrySnapshot, FederationError>>, Sources) {
    let (read, directory) = ihe::mcsd::registry::read_source(settings);
    (read, Sources { ihe: directory })
}
