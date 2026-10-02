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
use ferrofed_identity::binding::{IdentityChange, ResolutionBindings};
use ferrofed_identity::dev::{DevCrossRefError, StaticResolver};
use ferrofed_identity::patient::{IdentifierNamespace, PatientRefError};
use ferrofed_identity::pixm::{ManagerConfig, PixAuth, PixmConfigError, PixmResolver};
use ferrofed_identity::resolver::Resolver;
use ferrofed_registry::error::{IdError, LoadError};
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aql::{Context, OffsetStrategy, Targeting};
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::NodeSelection;
use crate::config::settings::{PixmSettings, Scheme, Settings};

/// The federation a server serves the federated query over.
pub struct Federation {
    snapshot: Arc<RegistrySnapshot>,
    clients: NodeClients<ReqwestTransport>,
    resolver: Option<Arc<dyn Resolver>>,
    bindings: ResolutionBindings,
    context: Context,
    budget: Budget,
    best_effort: bool,
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
    /// `[pixm]` is set but no registry document is, so it names members that
    /// do not exist.
    #[error("the [pixm] resolver needs registry.document, whose members it names")]
    PixmWithoutRegistry,
    /// Both `[dev]` and `[pixm]` are set, and exactly one resolver is active
    /// (`docs/architecture.md` section 6, decision A14).
    #[error("set one resolver: [dev] and [pixm] are both configured")]
    TwoResolvers,
    /// A registry is configured, but `federation.node_selection` is not: how
    /// an undirected patient query finds its nodes is a deployment decision,
    /// declared and never defaulted (§4.3, N4).
    #[error(
        "set federation.node_selection when registry.document is set: \"ask-all\" asks every member's cross-reference (§4.3, N4)"
    )]
    NodeSelectionUndeclared,
    /// A `[pixm]` member key is not a node id.
    #[error("pixm.manager[{manager}].members.{key:?} is not a node id")]
    PixmMember {
        /// The Manager's index.
        manager: usize,
        /// The key that was given.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// A `[pixm.namespaces]` key is not a namespace.
    #[error("pixm.namespaces has an empty namespace")]
    PixmNamespace(#[source] PatientRefError),
    /// The PIXm resolver refuses its Managers or members.
    #[error("the [pixm] resolver cannot be enabled")]
    Pixm(#[source] PixmConfigError),
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
            if settings.pixm.is_some() {
                return Err(FederationError::PixmWithoutRegistry);
            }
            return Ok(None);
        };
        let Some(selection) = settings.federation.node_selection else {
            return Err(FederationError::NodeSelectionUndeclared);
        };
        let snapshot =
            RegistrySnapshot::read(path).map_err(|source| FederationError::Registry {
                path: path.clone(),
                source: Box::new(source),
            })?;
        let resolver = match (&settings.dev, &settings.pixm) {
            (Some(_), Some(_)) => return Err(FederationError::TwoResolvers),
            (None, None) => None,
            (Some(section), None) => {
                let table = section.table().map_err(FederationError::DevTable)?;
                StaticResolver::from_config(settings.profile, Some(table), &snapshot)
                    .map_err(FederationError::DevCrossRef)?
                    .map(|resolver| -> Arc<dyn Resolver> { Arc::new(resolver) })
            }
            (None, Some(pixm)) => Some(pixm_resolver(pixm, &snapshot)?),
        };
        let credentials = onward_credentials(settings)?;
        // NOTE: §11.5 deadlines live on each call; the client's own timeout
        // only backstops a connection the call deadline cannot reach.
        let transport = ReqwestTransport::with_timeout(settings.federation.budget.overall())
            .map_err(|source| FederationError::Transport(Box::new(source)))?;
        let clients = NodeClients::from_snapshot(&snapshot, &transport, &credentials)
            .map_err(FederationError::Clients)?;
        let mut context =
            Context::new(targeting(selection)).with_offset_strategy(settings.federation.offset);
        if let Some(namespace) = &settings.federation.default_namespace {
            context = context.with_default_namespace(namespace.clone());
        }
        Ok(Some(Self {
            snapshot: Arc::new(snapshot),
            clients,
            resolver,
            bindings: ResolutionBindings::new(settings.federation.binding_ttl),
            context,
            budget: settings.federation.budget,
            best_effort: settings.federation.best_effort,
        }))
    }

    /// Assembles a federation from parts, for a test that builds its own.
    ///
    /// It offers best-effort completion, as the configuration does by
    /// default; [`Federation::with_best_effort`] withdraws it.
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
            bindings: ResolutionBindings::new(std::time::Duration::from_millis(
                crate::config::Federation::default().binding_ttl_ms,
            )),
            context,
            budget,
            best_effort: crate::config::Federation::default().best_effort,
        }
    }

    /// This federation, offering best-effort completion when `offered` is
    /// `true` (§11.4).
    #[must_use]
    pub fn with_best_effort(mut self, offered: bool) -> Self {
        self.best_effort = offered;
        self
    }

    /// The resolution bindings of every client session (§12.5.1 step 2).
    #[must_use]
    pub fn bindings(&self) -> &ResolutionBindings {
        &self.bindings
    }

    /// The PMIR hook (track 8, provisional): a merge or split at the identity
    /// source drops every resolution binding it could have made stale, in
    /// every session (`docs/architecture.md` section 6).
    ///
    /// A PMIR subscription (ITI-94) that receives a merge or split (ITI-93)
    /// calls this; until one is configured, the bindings' time-to-live is the
    /// bound. The event names how many bindings went, never an identifier.
    pub fn identity_changed(&self, change: &IdentityChange) -> usize {
        let dropped = self.bindings.identity_changed(change);
        tracing::info!(
            dropped,
            scoped = matches!(change, IdentityChange::Ehrs(_)),
            "an identity change dropped resolution bindings"
        );
        dropped
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

    /// What the deployment adds to the query text: the targeting, the
    /// default issuing namespace and the `OFFSET` strategy.
    #[must_use]
    pub fn context(&self) -> &Context {
        &self.context
    }

    /// The per-node timeout and the overall budget of each fan-out.
    #[must_use]
    pub fn budget(&self) -> Budget {
        self.budget
    }

    /// Whether a request may opt into best-effort completion with
    /// `openEHR-federation-completeness: partial` (§11.4, N37).
    // TODO(#73): declare completeness.best_effort and its opt_in in the OPTIONS {base}/ body (§7a.2, §11.4).
    #[must_use]
    pub fn best_effort(&self) -> bool {
        self.best_effort
    }

    /// How `OFFSET k > 0` is answered across the fan-out, with its bound
    /// (§11.6.2, N39).
    // TODO(#73): declare paging.offset_strategy and paging.max_window in the OPTIONS {base}/ body (§7a.2, §11.6.2).
    #[must_use]
    pub fn offset_strategy(&self) -> OffsetStrategy {
        self.context.offset_strategy()
    }
}

impl std::fmt::Debug for Federation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Federation")
            .field("endpoints", &self.clients.len())
            .field("resolver", &self.resolver.is_some())
            .field("budget", &self.budget)
            .field("best_effort", &self.best_effort)
            .field("offset_strategy", &self.context.offset_strategy())
            .finish_non_exhaustive()
    }
}

/// The PIXm resolver `[pixm]` describes over the members of `snapshot`.
fn pixm_resolver(
    pixm: &PixmSettings,
    snapshot: &RegistrySnapshot,
) -> Result<Arc<dyn Resolver>, FederationError> {
    let mut managers = Vec::with_capacity(pixm.managers.len());
    for (index, manager) in pixm.managers.iter().enumerate() {
        let mut members = BTreeMap::new();
        for (key, domain) in &manager.members {
            let member =
                NodeId::new(key.as_str()).map_err(|source| FederationError::PixmMember {
                    manager: index,
                    key: key.clone(),
                    source,
                })?;
            members.insert(member, domain.clone());
        }
        let auth = match &manager.credentials {
            None => PixAuth::None,
            Some(Scheme::Bearer(token)) => PixAuth::Bearer(token.clone()),
            Some(Scheme::Basic { user, password }) => PixAuth::Basic {
                user: user.clone(),
                password: password.clone(),
            },
        };
        managers.push(ManagerConfig {
            base: manager.url.clone(),
            auth,
            members,
        });
    }
    let mut namespaces = BTreeMap::new();
    for (namespace, system) in &pixm.namespaces {
        let namespace =
            IdentifierNamespace::new(namespace.as_str()).map_err(FederationError::PixmNamespace)?;
        namespaces.insert(namespace, system.clone());
    }
    let resolver =
        PixmResolver::from_config(managers, namespaces, snapshot).map_err(FederationError::Pixm)?;
    Ok(Arc::new(resolver))
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

/// The rewrite's targeting for an undirected query under the declared node
/// selection (§4.3, N4).
// TODO(#73): declare the node selection in the OPTIONS {base}/ body (§7a.2, N30).
fn targeting(selection: NodeSelection) -> Targeting {
    match selection {
        NodeSelection::AskAll => Targeting::AskAll,
    }
}
