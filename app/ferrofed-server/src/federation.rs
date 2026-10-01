// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the federated query runs over.
//!
//! The registry snapshot, one node client per endpoint with its onward
//! credentials, the cross-reference resolver, the rewrite's context and the
//! fan-out budget (`docs/architecture.md` sections 3, 5, 6 and 9).
//!
//! [`Federation::load`] builds it once at boot from the resolved settings.
//! Without a registry document the gateway federates nothing, so there is no
//! federation and the ITS-REST surface stays unserved.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_engine::dispatch::{NodeClients, SetupError, SharedCredentials};
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::dev::{DevCrossRefError, StaticResolver};
use ferrofed_identity::resolver::Resolver;
use ferrofed_registry::error::{IdError, LoadError};
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aql::{Context, Targeting};
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{Scheme, Settings};

/// The federation a server serves the federated query over.
pub struct Federation {
    snapshot: Arc<RegistrySnapshot>,
    clients: NodeClients<ReqwestTransport>,
    resolver: Option<Arc<dyn Resolver>>,
    context: Context,
    budget: Budget,
}

/// A federation that cannot be built from the settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FederationError {
    /// The registry document could not be read or refused to load.
    #[error("the registry document {} could not be loaded", path.display())]
    Registry {
        /// The document named by `registry.document`.
        path: PathBuf,
        /// What the registry reported.
        #[source]
        source: Box<LoadError>,
    },
    /// The `[dev]` table is set but no registry document is, so its rows name
    /// members that do not exist.
    #[error("the [dev] cross-reference needs registry.document, whose members its rows name")]
    DevWithoutRegistry,
    /// The `[dev]` table does not read as the static cross-reference.
    #[error("the [dev] cross-reference is not valid")]
    DevTable(#[source] crate::config::error::Error),
    /// The static cross-reference refuses its rows or the profile.
    #[error("the [dev] cross-reference cannot be enabled")]
    DevCrossRef(#[source] DevCrossRefError),
    /// A credentials section is keyed by something that is not an endpoint id.
    #[error("credentials.{key:?} is not an endpoint id")]
    CredentialsKey {
        /// The key that was given.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// The node clients could not be built.
    #[error("the node clients could not be built")]
    Clients(#[source] SetupError),
    /// The HTTP client every node client shares could not be built.
    #[error("the HTTP client for the nodes could not be built")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl Federation {
    /// Builds the federation `settings` describe, or `None` when no registry
    /// document is configured.
    ///
    /// The registry document is read and checked, every endpoint gets a node
    /// client over one shared connection pool, an endpoint with a
    /// `[credentials]` section sends them on every request, and the static
    /// cross-reference is enabled when `[dev]` is set under the development
    /// profile. Without a resolver, a query that names a patient fails closed
    /// (decision A17).
    ///
    /// # Errors
    /// Returns a [`FederationError`] for a registry document that does not
    /// load, a `[dev]` table that is refused, a credentials key that is not an
    /// endpoint id or names no endpoint of the registry, and an HTTP client
    /// that cannot be built.
    pub fn load(settings: &Settings) -> Result<Option<Self>, FederationError> {
        let Some(path) = &settings.registry_document else {
            if settings.dev.is_some() {
                return Err(FederationError::DevWithoutRegistry);
            }
            return Ok(None);
        };
        let snapshot =
            RegistrySnapshot::read(path).map_err(|source| FederationError::Registry {
                path: path.clone(),
                source: Box::new(source),
            })?;
        let resolver = match &settings.dev {
            None => None,
            Some(section) => {
                let table = section.table().map_err(FederationError::DevTable)?;
                StaticResolver::from_config(settings.profile, Some(table), &snapshot)
                    .map_err(FederationError::DevCrossRef)?
                    .map(|resolver| -> Arc<dyn Resolver> { Arc::new(resolver) })
            }
        };
        let credentials = onward_credentials(settings)?;
        // NOTE: §11.5 deadlines live on each call; the client's own timeout
        // only backstops a connection the call deadline cannot reach.
        let transport = ReqwestTransport::with_timeout(settings.federation.budget.overall())
            .map_err(|source| FederationError::Transport(Box::new(source)))?;
        let clients = NodeClients::from_snapshot(&snapshot, &transport, &credentials)
            .map_err(FederationError::Clients)?;
        let mut context = Context::new(Targeting::AskAll);
        if let Some(namespace) = &settings.federation.default_namespace {
            context = context.with_default_namespace(namespace.clone());
        }
        Ok(Some(Self {
            snapshot: Arc::new(snapshot),
            clients,
            resolver,
            context,
            budget: settings.federation.budget,
        }))
    }

    /// Assembles a federation from parts, for a test that builds its own.
    #[must_use]
    pub fn new(
        snapshot: RegistrySnapshot,
        clients: NodeClients<ReqwestTransport>,
        resolver: Option<Arc<dyn Resolver>>,
        context: Context,
        budget: Budget,
    ) -> Self {
        Self {
            snapshot: Arc::new(snapshot),
            clients,
            resolver,
            context,
            budget,
        }
    }

    /// The registry snapshot every query of this process runs over.
    #[must_use]
    pub fn snapshot(&self) -> &RegistrySnapshot {
        &self.snapshot
    }

    /// The node clients, one per endpoint.
    #[must_use]
    pub fn clients(&self) -> &NodeClients<ReqwestTransport> {
        &self.clients
    }

    /// The cross-reference resolver, when one is configured.
    #[must_use]
    pub fn resolver(&self) -> Option<&dyn Resolver> {
        self.resolver.as_deref()
    }

    /// What the deployment adds to the query text: the targeting and the
    /// default issuing namespace.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// The per-node timeout and the overall budget of each fan-out.
    #[must_use]
    pub fn budget(&self) -> Budget {
        self.budget
    }
}

impl std::fmt::Debug for Federation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Federation")
            .field("endpoints", &self.clients.len())
            .field("resolver", &self.resolver.is_some())
            .field("budget", &self.budget)
            .finish_non_exhaustive()
    }
}

/// The onward credentials of each endpoint that has a `[credentials]`
/// section, as the node clients send them.
fn onward_credentials(
    settings: &Settings,
) -> Result<BTreeMap<EndpointId, SharedCredentials>, FederationError> {
    let mut credentials = BTreeMap::new();
    for (key, scheme) in &settings.credentials {
        let endpoint =
            EndpointId::new(key.as_str()).map_err(|source| FederationError::CredentialsKey {
                key: key.clone(),
                source,
            })?;
        let onward = match scheme {
            Scheme::Bearer(token) => Credentials::Bearer(token.clone()),
            Scheme::Basic { user, password } => Credentials::Basic {
                user: user.clone(),
                password: password.clone(),
            },
        };
        let shared: SharedCredentials = Arc::new(onward);
        credentials.insert(endpoint, shared);
    }
    Ok(credentials)
}
