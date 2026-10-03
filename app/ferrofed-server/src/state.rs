// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every handler shares: the phase of the process, the health registry,
//! the federation, and the stored-query registry.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};

use ferrofed_registry::definition::store::{Definitions, StoreError};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::config::settings::Settings;
use crate::federation::{Federation, FederationError, read_registry};
use crate::health::lifecycle::Lifecycle;
use crate::health::{Built, HealthIndicator, Registry};
use crate::stored::RedbStore;

/// The state the router is built over.
///
/// Every state starts booting, so readiness answers `503` until the run path
/// marks boot complete ([`Lifecycle::booted`]).
#[derive(Debug, Default)]
pub struct AppState {
    /// Where the process is in its life, which gates readiness.
    lifecycle: Lifecycle,
    /// The indicators readiness runs.
    health: Registry,
    /// The federation the ITS-REST façade queries, when a registry is set.
    ///
    /// A registry reload replaces it whole; a request takes the `Arc` once
    /// and keeps it to its end.
    federation: RwLock<Option<Arc<Federation>>>,
    /// The stored-query registry, when it is offered (§12.7). It sits beside
    /// the federation, which holds no store handle.
    definitions: Option<Arc<Definitions>>,
}

/// A state that cannot be built from the settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StateError {
    /// The federation cannot be built.
    #[error(transparent)]
    Federation(#[from] FederationError),
    /// The stored-query registry's store cannot be opened or read.
    #[error("the stored-query store {} could not be opened", path.display())]
    StoredQueries {
        /// The store file `stored_queries.path` names.
        path: PathBuf,
        /// What the store reported.
        #[source]
        source: StoreError,
    },
}

impl AppState {
    /// Returns the state `settings` describe.
    ///
    /// The registry document is loaded, the outbound clients built, and the
    /// stored-query store opened with every definition it holds read, before
    /// the gateway serves. Each subsystem built gets a [`Built`] indicator,
    /// and no member node or identity source gets one. The state is booting
    /// until the run path marks boot complete.
    ///
    /// # Errors
    /// Returns a [`StateError`] when the federation `settings` describe
    /// cannot be built, or the stored-query store cannot be opened or read.
    pub fn build(settings: &Settings) -> Result<Self, StateError> {
        Self::build_read(settings, read_registry(settings))
    }

    /// Returns the state `settings` describe, over `document`, the registry
    /// document [`read_registry`] read from the same settings.
    ///
    /// The boot reads the document once, before the startup banner, and
    /// builds over that read here ([`Federation::load_read`]).
    ///
    /// # Errors
    /// Returns the [`StateError`] [`AppState::build`] returns, the read's own
    /// error included.
    pub fn build_read(
        settings: &Settings,
        document: Option<Result<RegistrySnapshot, FederationError>>,
    ) -> Result<Self, StateError> {
        settings.log_summary();
        let federation = Federation::load_read(settings, document)?;
        let definitions = settings
            .stored_queries
            .as_deref()
            .map(|path| {
                RedbStore::open(path)
                    .and_then(|store| Definitions::open(Box::new(store)))
                    .map_err(|source| StateError::StoredQueries {
                        path: path.to_path_buf(),
                        source,
                    })
            })
            .transpose()?;
        let mut built: Vec<Arc<dyn HealthIndicator>> = vec![Arc::new(Built("configuration"))];
        if federation.is_some() {
            built.push(Arc::new(Built("registry")));
            built.push(Arc::new(Built("outbound_clients")));
        }
        if definitions.is_some() {
            built.push(Arc::new(Built("stored_queries")));
        }
        Ok(Self {
            lifecycle: Lifecycle::default(),
            health: Registry::new(built),
            federation: RwLock::new(federation.map(Arc::new)),
            definitions: definitions.map(Arc::new),
        })
    }

    /// Returns a booting state with `health` as its registry and no
    /// federation.
    #[must_use]
    pub fn with_health(health: Registry) -> Self {
        Self {
            lifecycle: Lifecycle::default(),
            health,
            federation: RwLock::new(None),
            definitions: None,
        }
    }

    /// Returns a booting state that serves the federated query over
    /// `federation`.
    #[must_use]
    pub fn with_federation(federation: Federation) -> Self {
        Self {
            lifecycle: Lifecycle::default(),
            health: Registry::default(),
            federation: RwLock::new(Some(Arc::new(federation))),
            definitions: None,
        }
    }

    /// Returns where the process is in its life, which gates readiness.
    #[must_use]
    pub const fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    /// Returns the indicators readiness runs.
    #[must_use]
    pub const fn health(&self) -> &Registry {
        &self.health
    }

    /// Returns the federation, when the gateway federates.
    ///
    /// A request takes it once and keeps it to its end, so a registry reload
    /// never changes the membership under a running request.
    #[must_use]
    pub fn federation(&self) -> Option<Arc<Federation>> {
        // NOTE: no specification governs this: our own design; the lock guards
        // one `Arc` swap or clone, so a poisoned lock still holds a whole value.
        self.federation
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Puts `federation` in place of the running one, which the requests that
    /// already took it keep, and returns the running one.
    pub(crate) fn replace_federation(
        &self,
        federation: Arc<Federation>,
    ) -> Option<Arc<Federation>> {
        self.federation
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(federation)
    }

    /// Returns the stored-query registry, when it is offered (§12.7).
    #[must_use]
    pub fn definitions(&self) -> Option<&Arc<Definitions>> {
        self.definitions.as_ref()
    }
}
