// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every handler shares: the health registry, the federation, and the
//! stored-query registry.

use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_registry::definition::store::{Definitions, StoreError};

use crate::config::settings::Settings;
use crate::federation::{Federation, FederationError};
use crate::health::Registry;
use crate::stored::RedbStore;

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
    /// No subsystem with an indicator exists yet, so the health registry is
    /// empty and readiness answers `200` for a process that serves what it
    /// serves. The stored-query store is opened, and every definition it
    /// holds read, before the gateway serves.
    ///
    /// # Errors
    /// Returns a [`StateError`] when the federation `settings` describe
    /// cannot be built, or the stored-query store cannot be opened or read.
    pub fn build(settings: &Settings) -> Result<Self, StateError> {
        settings.log_summary();
        let federation = Federation::load(settings)?;
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
        Ok(Self {
            health: Registry::default(),
            federation,
            definitions: definitions.map(Arc::new),
        })
    }

    /// Returns a state with `health` as its registry and no federation.
    #[must_use]
    pub const fn with_health(health: Registry) -> Self {
        Self {
            health,
            federation: None,
            definitions: None,
        }
    }

    /// Returns a state that serves the federated query over `federation`.
    #[must_use]
    pub fn with_federation(federation: Federation) -> Self {
        Self {
            health: Registry::default(),
            federation: Some(federation),
            definitions: None,
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

    /// Returns the stored-query registry, when it is offered (§12.7).
    #[must_use]
    pub fn definitions(&self) -> Option<&Arc<Definitions>> {
        self.definitions.as_ref()
    }
}
