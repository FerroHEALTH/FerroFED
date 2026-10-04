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

use crate::binding::Indication;
#[cfg(feature = "binding-ihe")]
use crate::binding::ihe;
use crate::config::settings::Settings;
use crate::federation::error::FederationError;
use crate::reload::Reloader;
use crate::state::{AppState, StateError};

/// The instruments the bindings record through on the metrics surface, one
/// field per binding that has any, created once over the surface's meter.
#[derive(Debug)]
pub struct Instruments {
    /// The IHE binding's identity feed and audit spool instruments.
    #[cfg(feature = "binding-ihe")]
    pub(crate) ihe: ihe::metrics::Instruments,
}

impl Instruments {
    /// Returns every binding's instruments over `meter`.
    pub(crate) fn new(
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding has instruments")
        )]
        meter: &Meter,
    ) -> Self {
        Self {
            #[cfg(feature = "binding-ihe")]
            ihe: ihe::metrics::Instruments::new(meter),
        }
    }
}

/// What the bindings run for the life of the process, beside every
/// federation a reload builds: one field per binding that runs anything.
#[derive(Debug, Default)]
pub struct Processes {
    /// The IHE binding's care services directory and identity feed.
    #[cfg(feature = "binding-ihe")]
    pub(crate) ihe: ihe::process::Processes,
}

#[cfg_attr(
    not(feature = "binding-ihe"),
    expect(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        clippy::needless_pass_by_value,
        reason = "with no binding that runs a process compiled in, every method is a no-op"
    )
)]
impl Processes {
    /// Returns what `settings` describe for the bindings to run.
    ///
    /// # Errors
    ///
    /// The [`StateError`] of a process that cannot be built.
    pub(crate) fn build(
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding runs a process")
        )]
        settings: &Settings,
    ) -> Result<Self, StateError> {
        Ok(Self {
            #[cfg(feature = "binding-ihe")]
            ihe: ihe::process::Processes::build(settings)?,
        })
    }

    /// Takes over the registry sources the boot opened.
    pub(crate) fn watch(
        &mut self,
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding has a registry source")
        )]
        sources: Sources,
    ) {
        #[cfg(feature = "binding-ihe")]
        if let Some(directory) = sources.ihe {
            self.ihe.directory = Some(directory);
        }
    }

    /// Returns what each process indicates on `GET /health/dependencies`.
    pub(crate) fn indicate(&self) -> Vec<(&'static str, Indication)> {
        #[cfg(feature = "binding-ihe")]
        {
            self.ihe.indicate()
        }
        #[cfg(not(feature = "binding-ihe"))]
        {
            Vec::new()
        }
    }

    /// Returns `surface` with the routes the processes serve.
    pub(crate) fn routes(&self, surface: Router<Arc<AppState>>) -> Router<Arc<AppState>> {
        #[cfg(feature = "binding-ihe")]
        {
            self.ihe.routes(surface)
        }
        #[cfg(not(feature = "binding-ihe"))]
        {
            surface
        }
    }

    /// Starts every process, `reloader` replacing the registry a source
    /// changes.
    pub(crate) fn start(
        &self,
        #[cfg_attr(
            not(feature = "binding-ihe"),
            expect(unused_variables, reason = "no compiled binding runs a process")
        )]
        reloader: &Arc<Reloader>,
    ) -> Running {
        Running {
            #[cfg(feature = "binding-ihe")]
            ihe: self.ihe.start(reloader),
        }
    }
}

/// The processes a running gateway started.
#[derive(Debug)]
pub struct Running {
    /// The IHE binding's identity feed subscription.
    #[cfg(feature = "binding-ihe")]
    ihe: ihe::process::Running,
}

impl Running {
    /// Stops every process that holds something at a remote service.
    #[cfg_attr(
        not(feature = "binding-ihe"),
        expect(clippy::unused_async, reason = "no compiled binding runs a process")
    )]
    pub(crate) async fn drain(self) {
        #[cfg(feature = "binding-ihe")]
        self.ihe.drain().await;
    }
}

/// The registry sources a binding opened at boot, which the gateway keeps
/// its registry in step with.
#[derive(Debug, Default)]
pub struct Sources {
    /// The IHE binding's care services directory.
    #[cfg(feature = "binding-ihe")]
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
    #[cfg(feature = "binding-ihe")]
    {
        let (read, directory) = ihe::mcsd::registry::read_source(settings);
        (read, Sources { ihe: directory })
    }
    #[cfg(not(feature = "binding-ihe"))]
    {
        (
            crate::federation::registry::read_registry(settings),
            Sources::default(),
        )
    }
}
