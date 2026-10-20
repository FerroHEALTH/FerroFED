// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! How a federation is built: at boot from the settings, after a registry
//! reload over what the process learned, and from parts for a test.

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::Arc;

use ehds_logging::category::Category;
use ferrofed_eehrxf::face::EPS_COMPOSITION;
use ferrofed_eehrxf::serving;
use ferrofed_engine::dispatch::NodeClients;
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::role::resolver::Resolver;
use ferrofed_identity::session::ResolutionBindings;
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::{Endpoint, RegistrySnapshot};
use jsonwebtoken::jwk::Jwk;
use openehr_federation::aql::{Context, Targeting};
use openehr_federation::id::FederationId;

use crate::access::AccessLog;
use crate::binding::seam::PublicDocument;
use crate::binding::{self, Role};
use crate::config::NodeSelection;
use crate::config::auth::PatientBinding;
use crate::config::settings::{ConsentDisclosure, Scheme, Settings};
use crate::facade::options;
use crate::health::dependencies::Dependencies;
use crate::localization::{self, LocalizationPolicy};
use crate::metrics::nodes::NodeRequests;
use crate::onward::NodeTransport;
use ferrofed_identity::dev::Profile;

use super::error::FederationError;
use super::registry::read_registry;
use super::{CrossReference, DemographicsStep, Federation, Observed, Reconciled, widened};

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
    /// load, a `[dev]` table that is refused, a credentials key or a
    /// `federation.demographic_endpoint` that names no endpoint of the
    /// registry, and an HTTP client that cannot be built.
    pub fn load(settings: &Settings) -> Result<Option<Self>, FederationError> {
        Self::assemble(settings, read_registry(settings), None)
    }

    /// Builds the federation `settings` describe over `document`, the
    /// registry document [`read_registry`] read from the same settings.
    ///
    /// The boot reads the document once, describes it in the startup banner,
    /// and builds over it here, so the gateway serves the document the banner
    /// described. The checks are [`Federation::load`]'s, in the same order.
    ///
    /// # Errors
    /// Returns the [`FederationError`] [`Federation::load`] returns, the
    /// read's own error included.
    pub fn load_read(
        settings: &Settings,
        document: Option<Result<RegistrySnapshot, FederationError>>,
    ) -> Result<Option<Self>, FederationError> {
        Self::assemble(settings, document, None)
    }

    /// Builds the federation `settings` describe after a registry reload over
    /// `document`, the registry [`read_registry`] read again or a refresh of
    /// the care services directory read, checked as [`Federation::load_read`]
    /// checks it at boot.
    ///
    /// The new federation has its own snapshot, node clients and resolver,
    /// and keeps what this one learned: the resolution bindings, the `ehr_id`
    /// index and the learned `creating_system_id` map, which
    /// [`Federation::reconcile`] then holds to the new snapshot, and records
    /// its node requests through this one's instruments. This federation is
    /// unchanged, so a request that took it finishes on it.
    ///
    /// # Errors
    /// Returns the [`FederationError`] [`Federation::load_read`] returns for
    /// the same settings and document.
    pub fn reloaded(
        &self,
        settings: &Settings,
        document: Option<Result<RegistrySnapshot, FederationError>>,
    ) -> Result<Option<Self>, FederationError> {
        let mut next = Self::assemble(settings, document, Some(Arc::clone(&self.observed)))?;
        if let (Some(next), Some(instruments)) = (next.as_mut(), self.requests.instruments()) {
            next.meter(instruments.clone());
        }
        Ok(next)
    }

    /// Holds what the process learned to this federation's snapshot, after a
    /// reload in which the members `departed` left the registry.
    ///
    /// Every learned `creating_system_id` route the new document contradicts
    /// is withdrawn with its incident ([`LearnedMap::reconcile`](ferrofed_registry::creating_system::LearnedMap::reconcile)), and every
    /// `ehr_id` index entry and resolution binding naming a member that left
    /// is dropped.
    #[must_use]
    pub fn reconcile(&self, departed: &BTreeSet<NodeId>) -> Reconciled {
        let incidents = self.learned().reconcile(&self.snapshot);
        Reconciled {
            incidents,
            index_dropped: self.observed.index.forget_members(departed),
            bindings_dropped: self.observed.bindings.forget_members(departed),
        }
    }

    /// Builds the federation over `document`, and over `observed` when a
    /// reload carries it over.
    fn assemble(
        settings: &Settings,
        document: Option<Result<RegistrySnapshot, FederationError>>,
        observed: Option<Arc<Observed>>,
    ) -> Result<Option<Self>, FederationError> {
        let Some(document) = document else {
            for binding in binding::compiled() {
                binding.unregistered(settings)?;
            }
            if settings.federation.demographic_endpoint.is_some() {
                return Err(FederationError::DemographicWithoutRegistry);
            }
            if let Some((key, _)) = patient_bindings(settings).next() {
                return Err(FederationError::PatientWithoutRegistry { key });
            }
            return Ok(None);
        };
        let Some(selection) = settings.federation.node_selection else {
            return Err(FederationError::NodeSelectionUndeclared);
        };
        let Some(id) = settings.federation.id.clone() else {
            return Err(FederationError::IdUndeclared);
        };
        let snapshot = document?;
        if let Some(endpoint) = &settings.federation.demographic_endpoint
            && snapshot.endpoint(endpoint).is_none()
        {
            return Err(FederationError::DemographicEndpointUnknown {
                endpoint: endpoint.clone(),
            });
        }
        let offers = binding::offers(settings);
        binding::single(&offers, Role::Resolver)?;
        let resolving = binding::resolver(settings, &snapshot)?;
        let resolver = resolving
            .as_ref()
            .map(|seam| -> Arc<dyn Resolver> { Arc::clone(&seam.resolver) });
        patient_bound(settings, &snapshot, resolver.is_some())?;
        faced(settings, &snapshot, resolver.is_some())?;
        let (observed, demographics) = carry(settings, observed, resolver.is_some())?;
        if selection == NodeSelection::Localized {
            binding::single(&offers, Role::Localizer)?;
        }
        let localization =
            localization::policy(settings, selection, &offers, resolving.as_ref(), &snapshot)
                .map_err(FederationError::Localization)?;
        // NOTE: N27a: at most one consent pre-filter is active, so two
        // bindings' pre-filters are never combined.
        binding::single(&offers, Role::ConsentPrefilter)?;
        let consent = binding::prefilter(settings, &snapshot)?;
        // NOTE: §11.5 deadlines live on each call; the client's own timeout
        // only backstops a connection the call deadline cannot reach.
        let transport = crate::onward::node_transport(reqwest::Client::builder(), settings)
            .map_err(|source| FederationError::Transport(Box::new(source)))?;
        let onward = crate::onward::onward(settings, transport, &snapshot)?;
        let signer = Arc::new(crate::conveyed::signer(settings, &id)?);
        let clients = NodeClients::from_snapshot_over(
            &snapshot,
            (&onward.transport, &onward.transports),
            &onward.credentials,
        )
        .and_then(|clients| clients.with_on_behalf(&onward.on_behalf))
        .and_then(|clients| clients.with_dpop(&onward.dpop))
        .map(|clients| limited(clients, &settings.federation))
        .map_err(FederationError::Clients)?;
        let mut context = Context::new(targeting(selection))
            .with_offset_strategy(settings.federation.offset)
            .with_decomposable_aggregates(settings.federation.decomposable.iter().copied());
        if let Some(namespace) = &settings.federation.default_namespace {
            context = context.with_default_namespace(namespace.clone());
        }
        let seams = (resolver.is_some(), consent.is_some());
        let dependencies = dependencies(settings, &snapshot, seams, &localization)?
            .with_demographics(demographics.is_some());
        let requests = NodeRequests::new(snapshot.endpoints().map(Endpoint::id));
        let documents = documents(settings)?;
        let access = access_log(settings, &snapshot)?.map(Arc::new);
        let federation = Self {
            id,
            snapshot: Arc::new(snapshot),
            clients,
            // NOTE: no specification governs this: our own design; one binding offers the resolver,
            // so a resolver with `[dev]` set is the static development cross-reference.
            cross_reference: CrossReference::of(settings.dev.is_some() && resolver.is_some()),
            resolver,
            demographics,
            localization,
            consent,
            consent_disclosure: settings.federation.consent_disclosure,
            consent_emergency: settings.federation.consent_emergency,
            observed,
            context,
            budget: settings.federation.budget,
            max_node_rows: Some(settings.federation.max_node_rows),
            best_effort: settings.federation.best_effort,
            demographic: settings.federation.demographic_endpoint.clone(),
            dependencies,
            requests,
            template_fan_out: settings.federation.fan_out_template_upload,
            stored_query_fan_out: settings.federation.fan_out_stored_queries,
            signing: settings.signing.clone(),
            signer: Some(signer),
            client_keys: client_keys(settings),
            documents,
            access,
        };
        options::describe(&federation, false).map_err(FederationError::Describe)?;
        Ok(Some(federation))
    }

    /// Assembles a federation from parts, for a test that builds its own.
    ///
    /// It offers best-effort completion, as the configuration does by
    /// default; [`Federation::with_best_effort`] withdraws it. It fans no
    /// template upload out, as the configuration does not by default;
    /// [`Federation::with_template_fan_out`] offers it. Its clients read at
    /// most the configuration's default `max_node_rows` of one node for a
    /// query with no `LIMIT`.
    #[must_use]
    pub fn new(
        id: FederationId,
        snapshot: RegistrySnapshot,
        clients: NodeClients<NodeTransport>,
        resolver: Option<Arc<dyn Resolver>>,
        context: Context,
        budget: Budget,
    ) -> Self {
        let dependencies =
            Dependencies::new(snapshot.endpoints().map(Endpoint::id), resolver.is_some());
        let requests = NodeRequests::new(snapshot.endpoints().map(Endpoint::id));
        let max_node_rows = NonZeroU32::new(crate::config::Federation::default().max_node_rows);
        let clients = match max_node_rows {
            Some(ceiling) => clients.with_row_ceiling(ceiling),
            None => clients,
        };
        Self {
            max_node_rows,
            id,
            snapshot: Arc::new(snapshot),
            clients,
            resolver,
            cross_reference: CrossReference::default(),
            demographics: None,
            localization: LocalizationPolicy::none(),
            consent: None,
            consent_disclosure: ConsentDisclosure::of(
                crate::config::Federation::default().consent.disclose,
            ),
            consent_emergency: crate::config::Federation::default().consent.emergency,
            observed: Arc::new(Observed::new(
                ResolutionBindings::new(std::time::Duration::from_millis(
                    crate::config::Federation::default().binding_ttl_ms,
                )),
                default_index_capacity(),
            )),
            context,
            budget,
            best_effort: crate::config::Federation::default().best_effort,
            demographic: None,
            dependencies,
            requests,
            template_fan_out: crate::config::Federation::default().fan_out_template_upload,
            stored_query_fan_out: crate::config::Federation::default().fan_out_stored_queries,
            signing: None,
            signer: None,
            client_keys: Vec::new(),
            documents: Vec::new(),
            access: None,
        }
    }
}

/// `clients` held to the per-node limits `federation` sets: the requests in
/// flight to one endpoint (§11.5, N38), and the rows read of one node for a
/// query with no `LIMIT` (§11.5).
fn limited(
    clients: NodeClients<NodeTransport>,
    federation: &crate::config::settings::FederationSettings,
) -> NodeClients<NodeTransport> {
    clients
        .with_in_flight_cap(federation.max_in_flight_per_node)
        .with_row_ceiling(federation.max_node_rows)
}

/// The access log `settings` describe: the category map, the retention and
/// the labels, over the `[audit]` trail every build carries; `None` under
/// development with the records off.
///
/// # Errors
///
/// [`FederationError::RetentionEndpointUnknown`] for a retention origin
/// `snapshot` does not hold, [`FederationError::LabelEndpointUnknown`] for a
/// labelled origin it does not hold, [`FederationError::Audit`] for a trail
/// that cannot start, and [`FederationError::AccessLogUnavailable`] outside
/// development when the records go nowhere (Regulation (EU) 2025/327 Annex II
/// 3.2).
fn access_log(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
) -> Result<Option<AccessLog>, FederationError> {
    let retention = &settings.access_log.retention;
    let labels = &settings.access_log.labels;
    let unknown = |origin: &&str| {
        !snapshot
            .endpoints()
            .any(|endpoint| endpoint.id().as_str() == *origin)
    };
    if let Some(endpoint) = retention.origins().find(unknown) {
        return Err(FederationError::RetentionEndpointUnknown {
            endpoint: endpoint.to_owned(),
        });
    }
    if let Some(endpoint) = labels.origins().find(unknown) {
        return Err(FederationError::LabelEndpointUnknown {
            endpoint: endpoint.to_owned(),
        });
    }
    // NOTE: no specification governs this: our own design; the access record is due from every
    // deployment, so its sink is the `[audit]` trail every build carries, never a binding's.
    if let Some(sink) = crate::audit::access_sink(settings).map_err(FederationError::Audit)? {
        return Ok(Some(
            AccessLog::new(settings.access_log.map.clone(), sink)
                .with_retention(retention.clone())
                .with_labels(labels.clone())
                .with_emergency(settings.access_log.emergency.clone())
                .with_patient_namespaces(settings.access_log.patient_namespaces.clone())
                .with_max_named_ehrs(settings.access_log.max_named_ehrs),
        ));
    }
    // NOTE: Regulation (EU) 2025/327 Annex II 3.2 asks for a record of every access, so a
    // gateway that intermediates patient data records none only under development.
    if settings.profile == Profile::Development {
        return Ok(None);
    }
    Err(FederationError::AccessLogUnavailable)
}

/// The public documents every compiled binding has the gateway serve, each
/// at a path of its own outside the ITS-REST surface, `{base}/v1/`.
///
/// # Errors
///
/// A binding's [`FederationError`] for a document it cannot build,
/// [`FederationError::DocumentInSurface`] for one inside the surface, and
/// [`FederationError::DocumentTwice`] for two at one path.
fn documents(settings: &Settings) -> Result<Vec<PublicDocument>, FederationError> {
    let surface = settings.server.base_path.join(crate::ITS_REST_PREFIX);
    let mut documents: Vec<PublicDocument> = Vec::new();
    for binding in binding::compiled() {
        for document in binding.documents(settings)? {
            // NOTE: no specification governs this: our own design; the client
            // authentication gate reads every request under the surface.
            if document.path.starts_with(&surface) {
                return Err(FederationError::DocumentInSurface {
                    path: document.path,
                    surface,
                });
            }
            if documents.iter().any(|known| known.path == document.path) {
                return Err(FederationError::DocumentTwice {
                    path: document.path,
                });
            }
            documents.push(document);
        }
    }
    Ok(documents)
}

/// The public half of the client key of every FAPI 2.0 grant `settings`
/// configures, then its previous key while it is rotated, and of the
/// assertion key of every `oauth` grant, each once, in endpoint order.
fn client_keys(settings: &Settings) -> Vec<Jwk> {
    let mut keys: Vec<Jwk> = Vec::new();
    for scheme in settings.credentials.values() {
        let published: Vec<Jwk> = match scheme {
            Scheme::Fapi2(grant) => grant.published_client_keys().keys,
            // NOTE: RFC 7517 §4, the crate renders the public JWK, which reads as one;
            // a key no JWK reader takes is legitimately not published.
            Scheme::Oauth { grant, .. } => grant
                .assertion_key
                .iter()
                .filter_map(|key| serde_json::from_value(key.public_jwk()).ok())
                .collect(),
            _ => Vec::new(),
        };
        for key in published {
            if !keys
                .iter()
                .any(|known| known.common.key_id == key.common.key_id)
            {
                keys.push(key);
            }
        }
    }
    keys
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

/// What the process observed, `observed` when a reload carries it over or
/// nothing yet at boot, and the demographics step `settings` describe.
///
/// # Errors
///
/// Returns the binding's error for a demographics step with no
/// cross-reference `resolving` the master identity it finds, such as
/// [`FederationError::PdqmWithoutResolver`], and for a step that cannot be
/// built.
fn carry(
    settings: &Settings,
    observed: Option<Arc<Observed>>,
    resolving: bool,
) -> Result<(Arc<Observed>, Option<DemographicsStep>), FederationError> {
    let observed = observed.unwrap_or_else(|| {
        let federation = &settings.federation;
        Arc::new(Observed::new(
            ResolutionBindings::new(federation.binding_ttl)
                .with_capacity(widened(federation.binding_capacity)),
            federation.ehr_index_capacity,
        ))
    });
    let demographics = binding::demographics(settings, resolving)?;
    Ok((observed, demographics))
}

/// Holds every `[auth.issuer.patient]` binding of `settings` to the
/// registry `snapshot`, and to a configured resolver when `resolving`.
///
/// # Errors
///
/// Returns [`FederationError::PatientEndpointUnknown`] for a binding whose
/// endpoint the registry lacks, and
/// [`FederationError::PatientWithoutResolver`] for one without a resolver.
fn patient_bound(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    resolving: bool,
) -> Result<(), FederationError> {
    for (key, binding) in patient_bindings(settings) {
        // NOTE: no specification governs this: our own design; a patient token's
        // member is held to the registry as every configured endpoint is.
        if snapshot.endpoint(&binding.endpoint).is_none() {
            let endpoint = binding.endpoint.clone();
            return Err(FederationError::PatientEndpointUnknown { key, endpoint });
        }
        if !resolving {
            return Err(FederationError::PatientWithoutResolver { key });
        }
    }
    Ok(())
}

/// Holds the member of every `[[fhir.receive]]` of `settings` to the
/// registry `snapshot`, and to a configured resolver when `resolving`.
///
/// # Errors
///
/// Returns [`FederationError::ReceivingMemberUnknown`] for a category whose
/// member is no node of the registry, and
/// [`FederationError::ReceivingWithoutResolver`] when a category is received
/// and no resolver is configured.
fn receiving(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    resolving: bool,
) -> Result<(), FederationError> {
    let Some(fhir) = &settings.fhir else {
        return Ok(());
    };
    if !resolving && !fhir.receivers.is_empty() {
        return Err(FederationError::ReceivingWithoutResolver);
    }
    for (category, member) in fhir.receivers.members() {
        // NOTE: Federation Tier §2.3, N23: a new object targets one explicitly chosen node,
        // so the node a category is written to must be a member the registry holds.
        let known = NodeId::new(member).is_ok_and(|node| snapshot.node(&node).is_some());
        if !known {
            return Err(FederationError::ReceivingMemberUnknown {
                category: category.code(),
                member: member.to_owned(),
            });
        }
    }
    Ok(())
}

/// Holds the FHIR face of `settings` to the registry `snapshot`, and to a
/// configured resolver when `resolving`: the receiving members, every
/// member's own endpoint and the serving members, in that order.
///
/// # Errors
///
/// The [`FederationError`] of [`receiving`], [`natives`] or [`served`].
fn faced(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    resolving: bool,
) -> Result<(), FederationError> {
    receiving(settings, snapshot, resolving)?;
    natives(snapshot)?;
    served(settings, snapshot, resolving)
}

/// Holds every `[[fhir.serve]]` member of `settings` to the registry
/// `snapshot`, to an endpoint of its own there that serves the category,
/// and to a configured resolver when `resolving`.
///
/// # Errors
///
/// Returns [`FederationError::ServingWithoutResolver`] when a category is
/// served and no resolver is configured,
/// [`FederationError::ServingMemberUnknown`] for a member that is no node
/// of the registry, and [`FederationError::ServingWithoutEndpoint`] for one
/// with no `hl7-fhir-rest` endpoint serving the category.
fn served(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    resolving: bool,
) -> Result<(), FederationError> {
    let Some(fhir) = &settings.fhir else {
        return Ok(());
    };
    if !resolving && !fhir.serving.is_empty() {
        return Err(FederationError::ServingWithoutResolver);
    }
    for (category, members) in fhir.serving.members() {
        let code = category.code();
        for member in members {
            let Some(node) = NodeId::new(member)
                .ok()
                .filter(|node| snapshot.node(node).is_some())
            else {
                return Err(FederationError::ServingMemberUnknown {
                    category: code,
                    member: member.clone(),
                });
            };
            // NOTE: no specification governs this: our own design; a member serves the
            // category on its own endpoint, whose profile `natives` holds, and nowhere else.
            let endpoint = snapshot
                .native_endpoints()
                .any(|native| native.node() == &node && native.serves(code));
            if !endpoint {
                return Err(FederationError::ServingWithoutEndpoint {
                    category: code,
                    member: member.clone(),
                });
            }
        }
    }
    Ok(())
}

/// Holds every member's own exchange-format endpoint of `snapshot` to the
/// priority categories of Regulation (EU) 2025/327 Art 14(1), and one that
/// serves patient summaries, prescriptions or dispensations to the profile
/// the face asks for them in.
///
/// # Errors
///
/// Returns [`FederationError::NativeCategoryUnknown`] for a code that is no
/// priority category, and [`FederationError::NativeProfileMissing`] for a
/// patient summary endpoint that does not declare the EPS composition, or a
/// prescription or dispensation one that does not declare its HL7 Europe
/// MPD profile.
fn natives(snapshot: &RegistrySnapshot) -> Result<(), FederationError> {
    for native in snapshot.native_endpoints() {
        for code in native.categories() {
            let Some(category) = Category::priority(code) else {
                return Err(FederationError::NativeCategoryUnknown {
                    endpoint: native.id().clone(),
                    category: code.clone(),
                });
            };
            // NOTE: IPS OperationDefinition-summary `profile`, HL7 Europe MPD 1.0.0; the face
            // asks every member for the profile it answers in itself, so a native one declares it.
            let profile = if category == Category::PatientSummary {
                Some(EPS_COMPOSITION)
            } else {
                eehrxf::category::Category::of_code(code).and_then(serving::profile)
            };
            if let Some(profile) = profile
                && !native.profiles().iter().any(|declared| declared == profile)
            {
                return Err(FederationError::NativeProfileMissing {
                    endpoint: native.id().clone(),
                    category: code.clone(),
                    profile,
                });
            }
        }
    }
    Ok(())
}

/// The health record of the members of `snapshot`, of the resolver and the
/// consent pre-filter when `(resolver, consent)` say they are configured, of
/// the localizer and what it records through, and of what every binding of
/// `settings` records through, such as an audit trail.
fn dependencies(
    settings: &Settings,
    snapshot: &RegistrySnapshot,
    (resolver, consent): (bool, bool),
    localization: &LocalizationPolicy,
) -> Result<Dependencies, FederationError> {
    let mut indicators = localization.indicators().to_vec();
    indicators.extend(crate::audit::indicators(&settings.audit).map_err(FederationError::Audit)?);
    indicators.extend(binding::indicators(settings)?);
    Ok(
        Dependencies::new(snapshot.endpoints().map(Endpoint::id), resolver)
            .with_consent(consent)
            .with_localizer(localization.localizer().is_some())
            .with_indicators(indicators),
    )
}

/// Every `[auth.issuer.patient]` binding of `settings`, with its key.
fn patient_bindings(settings: &Settings) -> impl Iterator<Item = (String, &PatientBinding)> {
    settings.server.auth.issuers.iter().filter_map(|issuer| {
        let binding = issuer.patient.as_ref()?;
        Some((
            format!("trust.issuer \"{}\".patient", issuer.issuer),
            binding,
        ))
    })
}

/// The rewrite's targeting for an undirected query under the declared node
/// selection (§4.3, N4), which `OPTIONS {base}/` declares as `aql.fan_out`
/// (§7a.2).
fn targeting(selection: NodeSelection) -> Targeting {
    match selection {
        NodeSelection::AskAll => Targeting::AskAll,
        NodeSelection::Localized => Targeting::Localized,
    }
}
