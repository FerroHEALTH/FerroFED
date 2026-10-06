// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the federated query runs over.
//!
//! The registry snapshot, one node client per endpoint with its onward
//! credentials, the cross-reference resolver, the optional consent
//! pre-filter, the rewrite's context and the
//! fan-out budget (§5.2, §7.1, §11.5).
//!
//! [`Federation::load`] builds it at boot from the resolved settings, and
//! [`Federation::reloaded`] builds its successor when the registry is
//! reloaded ([`crate::reload`]). Without a registry document the gateway
//! federates nothing, so there is no federation and the ITS-REST surface
//! stays unserved.

mod build;
pub mod error;
pub mod registry;

use std::collections::BTreeSet;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ferrofed_engine::conveyance::Signer;
use ferrofed_engine::dispatch::NodeClients;
use ferrofed_engine::fanout::Budget;
use ferrofed_identity::role::consent::ConsentPrefilter;
use ferrofed_identity::role::demographics::Demographics;
use ferrofed_identity::role::resolver::Resolver;
use ferrofed_identity::session::{IdentityChange, ResolutionBindings};
use ferrofed_registry::creating_system::LearnedMap;
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::incident::Incident;
use ferrofed_registry::snapshot::RegistrySnapshot;
use jsonwebtoken::jwk::Jwk;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::{Context, OffsetStrategy, Targeting};
use openehr_federation::dedup::DedupMode;
use openehr_federation::id::FederationId;

use crate::access::AccessLog;
use crate::binding::seam::PublicDocument;
use crate::config::settings::{ConsentDisclosure, SigningSettings};
use crate::health::dependencies::Dependencies;
use crate::localization::LocalizationPolicy;
use crate::metrics::nodes::{Instruments, NodeRequests};
use crate::metrics::resolver::Metered;
use crate::onward::NodeTransport;

/// The federation a server serves the federated query over.
pub struct Federation {
    id: FederationId,
    snapshot: Arc<RegistrySnapshot>,
    clients: NodeClients<NodeTransport>,
    resolver: Option<Arc<dyn Resolver>>,
    demographics: Option<DemographicsStep>,
    localization: LocalizationPolicy,
    consent: Option<Arc<dyn ConsentPrefilter>>,
    consent_disclosure: ConsentDisclosure,
    observed: Arc<Observed>,
    context: Context,
    budget: Budget,
    best_effort: bool,
    demographic: Option<EndpointId>,
    dependencies: Dependencies,
    requests: NodeRequests,
    template_fan_out: bool,
    stored_query_fan_out: bool,
    signing: Option<SigningSettings>,
    signer: Option<Arc<Signer>>,
    client_keys: Vec<Jwk>,
    documents: Vec<PublicDocument>,
    access: Option<Arc<AccessLog>>,
}

/// The demographics step a patient identifier the cross-reference does not
/// map is taken to before localization and resolution, with its budget
/// (Annex A §A.2).
///
/// A registry reload builds it again from `[pdqm]`, as it builds the resolver.
#[derive(Clone)]
pub struct DemographicsStep {
    step: Arc<dyn Demographics>,
    timeout: Duration,
}

impl DemographicsStep {
    /// The step `step`, each exchange of which may take `timeout`.
    #[must_use]
    pub fn new(step: Arc<dyn Demographics>, timeout: Duration) -> Self {
        Self { step, timeout }
    }

    /// The demographics service.
    #[must_use]
    pub fn step(&self) -> &dyn Demographics {
        self.step.as_ref()
    }

    /// How long one exchange may take.
    #[must_use]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
}

impl std::fmt::Debug for DemographicsStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DemographicsStep")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// What the process learns while it serves, which a registry reload carries
/// over to the federation it builds.
struct Observed {
    bindings: ResolutionBindings,
    index: EhrIndex,
    learned: Mutex<LearnedMap>,
}

impl Observed {
    /// Nothing learned yet: `bindings`, and an `ehr_id` index of `capacity`
    /// entries.
    fn new(bindings: ResolutionBindings, capacity: NonZeroU32) -> Self {
        Self {
            bindings,
            index: EhrIndex::new(widened(capacity)),
            learned: Mutex::new(LearnedMap::new()),
        }
    }
}

/// What a registry reload did to what the process had learned
/// ([`Federation::reconcile`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// The incidents the reload raised, each emitted once when it was raised.
    pub incidents: Vec<Incident>,
    /// How many `ehr_id` index entries named a member that left.
    pub index_dropped: usize,
    /// How many resolution bindings named a member that left.
    pub bindings_dropped: usize,
}

impl Federation {
    /// This federation, pre-filtering the candidates of every patient query
    /// through `prefilter` at Step 1 (N27a, §13.2.1).
    #[must_use]
    pub fn with_consent_prefilter(mut self, prefilter: Arc<dyn ConsentPrefilter>) -> Self {
        self.consent = Some(prefilter);
        self.dependencies = self.dependencies.with_consent(true);
        self
    }

    /// This federation, reporting a member the consent pre-filter excludes
    /// under `disclosure` ([`Federation::discloses_consent`]).
    #[must_use]
    pub fn with_consent_disclosure(mut self, disclosure: ConsentDisclosure) -> Self {
        self.consent_disclosure = disclosure;
        self
    }

    /// Whether an answer names a member the consent pre-filter excludes.
    ///
    /// `true` reports it `consent-denied` (N27a, §11.1). `false` is for a
    /// deployment under Regulation (EU) 2025/327 Art 8, where the fact of a
    /// restriction "shall not be visible to healthcare providers": the
    /// member is reported as one that does not know the patient, a read by
    /// subject answers as for a subject with no EHR there, and
    /// `OPTIONS {base}/` declares the choice as `consent.disclose` (§7a.2).
    /// The pre-filter metrics and the log count every exclusion either way.
    #[must_use]
    pub fn discloses_consent(&self) -> bool {
        self.consent_disclosure.is_disclosed()
    }

    /// The gateway's signing keys and where they are published, when
    /// `[signing]` is set: the JWK Set `{base}/.well-known/jwks.json` serves,
    /// and the `auth.jwks_uri` `OPTIONS {base}/` declares (§13.1, N25, N30).
    #[must_use]
    pub fn signing(&self) -> Option<&SigningSettings> {
        self.signing.as_ref()
    }

    /// The public keys of the FAPI 2.0 grants' client keys, which the JWK
    /// Set publishes beside the `[signing]` keys (FAPI 2.0 Security Profile
    /// §5.4.2).
    #[must_use]
    pub fn client_keys(&self) -> &[Jwk] {
        &self.client_keys
    }

    /// The public document a binding has the gateway serve at the request
    /// path `path`, such as a Nuts holder's DID document.
    #[must_use]
    pub fn document(&self, path: &str) -> Option<&PublicDocument> {
        self.documents.iter().find(|document| document.path == path)
    }

    /// The signer of what every request to a node conveys about its caller
    /// ([`crate::conveyed`]; §13.1, N24).
    #[must_use]
    pub fn signer(&self) -> Option<&Arc<Signer>> {
        self.signer.as_ref()
    }

    /// This federation, recording every access to patient data it serves in
    /// `log` (Regulation (EU) 2025/327 Annex II 3.2).
    #[must_use]
    pub fn with_access_log(mut self, log: AccessLog) -> Self {
        self.access = Some(Arc::new(log));
        self
    }

    /// The access log every access to patient data is recorded in, when
    /// one is configured.
    #[must_use]
    pub fn access_log(&self) -> Option<&Arc<AccessLog>> {
        self.access.as_ref()
    }

    /// This federation, conveying each caller signed by `signer`.
    #[must_use]
    pub fn with_signer(mut self, signer: Arc<Signer>) -> Self {
        self.signer = Some(signer);
        self
    }

    /// This federation, offering best-effort completion when `offered` is
    /// `true` (§11.4).
    #[must_use]
    pub fn with_best_effort(mut self, offered: bool) -> Self {
        self.best_effort = offered;
        self
    }

    /// This federation, fanning a template upload out to several members
    /// when `offered` is `true` (§12.6, N43).
    #[must_use]
    pub fn with_template_fan_out(mut self, offered: bool) -> Self {
        self.template_fan_out = offered;
        self
    }

    /// This federation, deriving the node set of an undirected patient query
    /// from the localizer `localization` names (N4, §14.1).
    #[must_use]
    pub fn with_localization(mut self, localization: LocalizationPolicy) -> Self {
        self.context = self.context.with_targeting(Targeting::Localized);
        self.dependencies = self
            .dependencies
            .with_localizer(localization.localizer().is_some());
        self.localization = localization;
        self
    }

    /// The localizer of an undirected patient query, with its failure policy
    /// and budget (§14.1).
    #[must_use]
    pub fn localization(&self) -> &LocalizationPolicy {
        &self.localization
    }

    /// This federation, recording its node requests through `instruments`
    /// ([`NodeRequests`]); a registry reload keeps them.
    #[must_use]
    pub fn metered(mut self, instruments: Instruments) -> Self {
        self.meter(instruments);
        self
    }

    /// Records this federation's node requests through `instruments`, and
    /// counts and times every call to its resolver ([`Metered`]).
    pub(crate) fn meter(&mut self, instruments: Instruments) {
        self.resolver = self.resolver.take().map(|inner| -> Arc<dyn Resolver> {
            Arc::new(Metered::new(inner, instruments.clone()))
        });
        self.requests.metered(instruments);
    }

    /// The resolution bindings of every client session (§12.5.1 step 2).
    #[must_use]
    pub fn bindings(&self) -> &ResolutionBindings {
        &self.observed.bindings
    }

    /// The `ehr_id` to node index every request of this process shares
    /// (§12.5.1 step 3).
    #[must_use]
    pub fn index(&self) -> &EhrIndex {
        &self.observed.index
    }

    /// The `creating_system_id` mappings learned from answers, which every
    /// request of this process shares (§12.2, N21).
    ///
    /// The guard is held for one lookup or one answer's sightings, never
    /// across an `.await`.
    pub fn learned(&self) -> MutexGuard<'_, LearnedMap> {
        // NOTE: a panic while the lock was held leaves mappings that may be
        // incomplete; each is still only a routing hint, so they stay usable.
        self.observed
            .learned
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// The PMIR hook (track 8, provisional): a merge or split at the identity
    /// source drops every resolution binding it could have made stale, in
    /// every session (§5.2: PMIR for the identity lifecycle; §12.5.1).
    ///
    /// A PMIR subscription (ITI-94) that receives a merge or split (ITI-93)
    /// calls this; until one is configured, the bindings' time-to-live is the
    /// bound. The event names how many bindings went, never an identifier.
    pub fn identity_changed(&self, change: &IdentityChange) -> usize {
        let dropped = self.observed.bindings.identity_changed(change);
        tracing::info!(
            dropped,
            scoped = matches!(change, IdentityChange::Ehrs(_)),
            "an identity change dropped resolution bindings"
        );
        dropped
    }

    /// The last state the gateway observed of each member endpoint and of
    /// the resolver, one slot each.
    ///
    /// Every slot is unknown when the federation is built, on a registry
    /// reload too, because a reload can move an endpoint or swap the resolver.
    #[must_use]
    pub fn dependencies(&self) -> &Dependencies {
        &self.dependencies
    }

    /// The record of the requests sent to each member endpoint, for the
    /// metrics surface.
    #[must_use]
    pub fn requests(&self) -> &NodeRequests {
        &self.requests
    }

    /// The federation's own identifier (§7a.2, N30).
    #[must_use]
    pub fn id(&self) -> &FederationId {
        &self.id
    }

    /// The registry snapshot every query that took this federation runs over.
    #[must_use]
    pub fn snapshot(&self) -> &RegistrySnapshot {
        &self.snapshot
    }

    /// The node clients, one per endpoint.
    #[must_use]
    pub fn clients(&self) -> &NodeClients<NodeTransport> {
        &self.clients
    }

    /// The cross-reference resolver, when one is configured.
    #[must_use]
    pub fn resolver(&self) -> Option<&dyn Resolver> {
        self.resolver.as_deref()
    }

    /// The Step-1 consent pre-filter, when one is configured (N27a).
    #[must_use]
    pub fn consent_prefilter(&self) -> Option<&dyn ConsentPrefilter> {
        self.consent.as_deref()
    }

    /// The demographics step ahead of localization and resolution, when one
    /// is configured (Annex A §A.2).
    #[must_use]
    pub fn demographics(&self) -> Option<&DemographicsStep> {
        self.demographics.as_ref()
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
    /// `openEHR-federation-completeness: partial` (§11.4, N37), as
    /// `completeness` declares it in `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn best_effort(&self) -> bool {
        self.best_effort
    }

    /// The one member endpoint a DEMOGRAPHIC request may name and be routed
    /// to, or `None` when that area answers `501` (§7a.1, §12.6, N32), as
    /// `its_rest.demographic` declares it in `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn demographic_endpoint(&self) -> Option<&EndpointId> {
        self.demographic.as_ref()
    }

    /// Whether a template upload naming `*` or several endpoints fans out to
    /// each of them (§12.6, N43), as `definition.fan_out_template_upload`
    /// declares it in `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn fans_out_template_upload(&self) -> bool {
        self.template_fan_out
    }

    /// Whether the stored-query registry distributes a definition to the
    /// members a `PUT` names, and reports per member whether its copy
    /// matches (§12.7, N44), as `definition.stored_query_fan_out` declares it
    /// in `OPTIONS {base}/` where the registry is offered (§7a.2).
    #[must_use]
    pub fn fans_out_stored_queries(&self) -> bool {
        self.stored_query_fan_out
    }

    /// How `OFFSET k > 0` is answered across the fan-out, with its bound
    /// (§11.6.2, N39), as `paging` declares it in `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn offset_strategy(&self) -> OffsetStrategy {
        self.context.offset_strategy()
    }

    /// The dedup modes a request may select with `openEHR-federation-dedup`,
    /// the default `none` first (§10, N15), as `dedup` declares them in
    /// `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn dedup_modes() -> &'static [DedupMode] {
        &DedupMode::OFFERED
    }

    /// The aggregate functions recombined across the fan-out, in declaration
    /// order (§11.6.3), as `aggregates.decomposable` declares them in
    /// `OPTIONS {base}/` (§7a.2).
    #[must_use]
    pub fn decomposable_aggregates(&self) -> &BTreeSet<AggregateFunction> {
        self.context.decomposable_aggregates()
    }
}

impl std::fmt::Debug for Federation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Federation")
            .field("id", &self.id)
            .field("endpoints", &self.clients.len())
            .field("resolver", &self.resolver.is_some())
            .field("demographics", &self.demographics)
            .field("localization", &self.localization)
            .field(
                "consent",
                &self.consent.as_ref().map(|consent| consent.mode()),
            )
            .field("consent_disclosure", &self.consent_disclosure)
            .field("budget", &self.budget)
            .field("best_effort", &self.best_effort)
            .field("demographic", &self.demographic)
            .field("template_fan_out", &self.template_fan_out)
            .field("stored_query_fan_out", &self.stored_query_fan_out)
            .field("offset_strategy", &self.context.offset_strategy())
            .field(
                "decomposable_aggregates",
                &self.context.decomposable_aggregates(),
            )
            .finish_non_exhaustive()
    }
}

/// The configured `capacity` as a count of held entries.
fn widened(capacity: NonZeroU32) -> NonZeroUsize {
    // NOTE: no specification governs this: our own design; a capacity past
    // `usize` is bounded by `usize`, which only a platform under 32 bits reaches.
    NonZeroUsize::try_from(capacity).unwrap_or(NonZeroUsize::MAX)
}
