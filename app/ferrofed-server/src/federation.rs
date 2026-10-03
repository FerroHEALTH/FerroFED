// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the federated query runs over.
//!
//! The registry snapshot, one node client per endpoint with its onward
//! credentials, the cross-reference resolver, the rewrite's context and the
//! fan-out budget (§5.2, §7.1, §11.5).
//!
//! [`Federation::load`] builds it once at boot from the resolved settings.
//! Without a registry document the gateway federates nothing, so there is no
//! federation and the ITS-REST surface stays unserved.

use std::collections::{BTreeMap, BTreeSet};
use std::num::{NonZeroU32, NonZeroUsize};
use std::path::PathBuf;
use std::sync::Arc;

use ferrofed_engine::dispatch::{NodeClients, SetupError, SharedCredentials};
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::binding::{IdentityChange, ResolutionBindings};
use ferrofed_identity::dev::{DevCrossRefError, StaticResolver};
use ferrofed_identity::directory;
use ferrofed_identity::directory::error::FhirFormError;
use ferrofed_identity::patient::{IdentifierNamespace, PatientRefError};
use ferrofed_identity::pixm::{ManagerConfig, PixAuth, PixmConfigError, PixmResolver};
use ferrofed_identity::resolver::Resolver;
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::error::{IdError, LoadError};
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::{Context, OffsetStrategy, Targeting};
use openehr_federation::dedup::DedupMode;
use openehr_its::rest::client::{Credentials, ReqwestTransport};

use crate::config::settings::{PixmSettings, Scheme, Settings};
use crate::config::{NodeSelection, RegistryFormat};

/// The federation a server serves the federated query over.
pub struct Federation {
    snapshot: Arc<RegistrySnapshot>,
    clients: NodeClients<ReqwestTransport>,
    resolver: Option<Arc<dyn Resolver>>,
    bindings: ResolutionBindings,
    index: EhrIndex,
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
    /// The registry document in FHIR form could not be read or refused to
    /// load (N19, N20, §15.2).
    #[error("the registry document {} could not be loaded", path.display())]
    FhirRegistry {
        /// The document named by `registry.document`.
        path: PathBuf,
        /// What the FHIR form reported.
        #[source]
        source: Box<FhirFormError>,
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
    /// (no specification governs this: our own design).
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
    /// (§11.3 covers only an answered lookup; no specification governs this:
    /// our own design).
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
        let snapshot = match settings.registry_format {
            RegistryFormat::Toml => {
                RegistrySnapshot::read(path).map_err(|source| FederationError::Registry {
                    path: path.clone(),
                    source: Box::new(source),
                })?
            }
            RegistryFormat::Fhir => {
                directory::read(path).map_err(|source| FederationError::FhirRegistry {
                    path: path.clone(),
                    source: Box::new(source),
                })?
            }
        };
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
        let mut context = Context::new(targeting(selection))
            .with_offset_strategy(settings.federation.offset)
            .with_decomposable_aggregates(settings.federation.decomposable.iter().copied());
        if let Some(namespace) = &settings.federation.default_namespace {
            context = context.with_default_namespace(namespace.clone());
        }
        Ok(Some(Self {
            snapshot: Arc::new(snapshot),
            clients,
            resolver,
            bindings: ResolutionBindings::new(settings.federation.binding_ttl),
            index: ehr_index(settings.federation.ehr_index_capacity),
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
            index: ehr_index(default_index_capacity()),
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

    /// The `ehr_id` to node index every request of this process shares
    /// (§12.5.1 step 3).
    #[must_use]
    pub fn index(&self) -> &EhrIndex {
        &self.index
    }

    /// The PMIR hook (track 8, provisional): a merge or split at the identity
    /// source drops every resolution binding it could have made stale, in
    /// every session (§5.2: PMIR for the identity lifecycle; §12.5.1).
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
    /// default issuing namespace, the `OFFSET` strategy and the decomposable
    /// aggregates.
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

    /// The dedup modes a request may select with `openEHR-federation-dedup`,
    /// the default `none` first (§10, N15).
    // TODO(#73): declare dedup.default, dedup.modes and dedup.request_header in the OPTIONS {base}/ body (§7a.2, §10).
    #[must_use]
    pub fn dedup_modes() -> &'static [DedupMode] {
        &DedupMode::OFFERED
    }

    /// The aggregate functions recombined across the fan-out, in declaration
    /// order (§11.6.3).
    // TODO(#73): declare aggregates.decomposable in the OPTIONS {base}/ body (§7a.2, §11.6.3).
    #[must_use]
    pub fn decomposable_aggregates(&self) -> &BTreeSet<AggregateFunction> {
        self.context.decomposable_aggregates()
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
            .field(
                "decomposable_aggregates",
                &self.context.decomposable_aggregates(),
            )
            .finish_non_exhaustive()
    }
}

/// An empty `ehr_id` index of `capacity` entries.
fn ehr_index(capacity: NonZeroU32) -> EhrIndex {
    // NOTE: no specification governs this: our own design; a capacity past
    // `usize` is bounded by `usize`, which only a platform under 32 bits reaches.
    EhrIndex::new(NonZeroUsize::try_from(capacity).unwrap_or(NonZeroUsize::MAX))
}

/// The configuration's default `ehr_id` index capacity.
#[expect(
    clippy::expect_used,
    reason = "the default capacity is a positive literal in the Federation Default impl"
)]
fn default_index_capacity() -> NonZeroU32 {
    NonZeroU32::new(crate::config::Federation::default().ehr_index_capacity)
        .expect("the default ehr_id index capacity should be positive")
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
