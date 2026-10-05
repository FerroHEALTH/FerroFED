// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings the run path holds, resolved from the configuration tree.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrofed_engine::fanout::Budget;
use ferrofed_engine::onward::Grant;
use ferrofed_engine::onward::fapi2::Fapi2Grant;
use ferrofed_engine::onward::keys::KeyRing;
use ferrofed_identity::dev::Profile;
use ferrofed_identity::localizer::OnFailure;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::secret::{Secret, SecretUrl};
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::OffsetStrategy;
use openehr_federation::id::FederationId;
use openehr_federation::object::Uri;

use crate::base_path::BasePath;
use crate::binding::OnwardGrant;
use crate::binding::development::DevSection;
use crate::config::auth::AuthSettings;
use crate::config::stored_queries::Store;
use crate::config::{NodeSelection, RegistryFormat};
use crate::telemetry::{Format, SampleRatio};

/// The settings the run path holds, with every secret already read.
///
/// The sections of a binding are resolved by the binding
/// ([`Binding::resolve`](crate::binding::Binding::resolve)), each into the
/// field that carries it.
#[derive(Debug)]
pub struct Settings {
    /// The deployment profile.
    pub profile: Profile,
    /// The HTTP surface.
    pub server: ServerSettings,
    /// The console.
    pub telemetry: TelemetrySettings,
    /// The registry document, when the gateway federates.
    pub registry_document: Option<PathBuf>,
    /// The form the registry document is written in.
    pub registry_format: RegistryFormat,
    /// The mCSD care services directory the registry is read from, when it
    /// is read from one (§15.1, Annex A.5).
    #[cfg(feature = "binding-ihe")]
    pub registry_directory: Option<crate::binding::ihe::mcsd::DirectorySettings>,
    /// The federated query.
    pub federation: FederationSettings,
    /// The outbound credentials, by endpoint id.
    pub credentials: BTreeMap<EndpointId, Scheme>,
    /// The static development cross-reference, as written.
    pub dev: Option<DevSection>,
    /// The PIXm resolver, with every secret read.
    #[cfg(feature = "binding-ihe")]
    pub pixm: Option<crate::binding::ihe::pixm::PixmSettings>,
    /// The XCPD localizer, with every secret and file read.
    #[cfg(feature = "binding-ihe")]
    pub xcpd: Option<crate::binding::ihe::xcpd::XcpdSettings>,
    /// The Dutch Generic Functions, with every secret and file read.
    #[cfg(feature = "binding-nl")]
    pub nl_gf: Option<crate::binding::nl::NlGfSettings>,
    /// The PMIR identity feed, with every secret read.
    #[cfg(feature = "binding-ihe")]
    pub pmir: Option<crate::binding::ihe::pmir::config::PmirSettings>,
    /// The PDQm demographics step, with every secret read.
    #[cfg(feature = "binding-ihe")]
    pub pdqm: Option<crate::binding::ihe::pdqm::PdqmSettings>,
    /// The store of the stored-query registry, when it is offered (§12.7).
    pub stored_queries: Option<Store>,
    /// The metrics surface.
    pub metrics: MetricsSettings,
    /// The gateway's signing keys and where they are published, when
    /// `[signing]` is set (§13.1, N25).
    pub signing: Option<SigningSettings>,
    /// Where the audit records of the PIXm, PDQm, mCSD and PMIR transactions go.
    #[cfg(feature = "binding-ihe")]
    pub audit: crate::binding::ihe::audit::config::AuditSettings,
}

/// The gateway's signing keys, resolved.
#[derive(Debug, Clone)]
pub struct SigningSettings {
    /// The current key and, while its window lasts, the previous one.
    pub keys: Arc<KeyRing>,
    /// The absolute URL `OPTIONS {base}/` declares for the JWK Set.
    pub jwks_uri: Uri,
    /// How long a client assertion is valid.
    pub assertion_lifetime: Duration,
}

/// The federated query, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationSettings {
    /// The federation's own identifier, as declared; a federation refuses to
    /// load without it (§7a.2, N30).
    pub id: Option<FederationId>,
    /// The per-node timeout and the overall budget of each fan-out.
    pub budget: Budget,
    /// The issuing namespace an unqualified patient identifier resolves in.
    pub default_namespace: Option<String>,
    /// How long the resolution bindings of a client session live.
    pub binding_ttl: Duration,
    /// How many `ehr_id` bindings the resolution bindings hold together.
    pub binding_capacity: NonZeroU32,
    /// How many `ehr_id`s the `ehr_id` to node index holds.
    pub ehr_index_capacity: NonZeroU32,
    /// How the node set of an undirected patient query is chosen, as
    /// declared; a federation refuses to load without it.
    pub node_selection: Option<NodeSelection>,
    /// The localizer's failure policy and budget, when
    /// `[federation.localization]` is set (§14.1).
    pub localization: Option<LocalizationSettings>,
    /// Whether a request may opt into best-effort completion (§11.4).
    pub best_effort: bool,
    /// How `OFFSET k > 0` is answered across a fan-out, with its bound: the
    /// `paging` an `OPTIONS {base}/` body declares (§11.6.2, §7a.2, N39).
    pub offset: OffsetStrategy,
    /// The aggregate functions recombined across a fan-out: the
    /// `aggregates.decomposable` an `OPTIONS {base}/` body declares
    /// (§11.6.3, §7a.2).
    pub decomposable: BTreeSet<AggregateFunction>,
    /// The one member endpoint that may serve the DEMOGRAPHIC area, or
    /// `None` when that area answers `501` (§7a.1, N32).
    pub demographic_endpoint: Option<EndpointId>,
    /// Whether a template upload may fan out to several members (§12.6,
    /// N43), as `definition.fan_out_template_upload` declares it (§7a.2).
    pub fan_out_template_upload: bool,
    /// Whether a stored-query definition may be distributed to members and
    /// checked there for drift (§12.7, N44), as
    /// `definition.stored_query_fan_out` declares it (§7a.2); only ever on
    /// beside the stored-query registry.
    pub fan_out_stored_queries: bool,
    /// Whether an answer names a member the Step-1 consent pre-filter
    /// excludes as `consent-denied` (N27a), as `consent.disclose` declares
    /// it where a pre-filter is configured (§7a.2).
    pub consent_disclosure: ConsentDisclosure,
}

/// Whether an answer names a member the Step-1 consent pre-filter excludes
/// (N27a, `[federation.consent] disclose`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDisclosure {
    /// The specification's behaviour: the member is `consent-denied` (N27a,
    /// §11.1).
    Disclosed,
    /// The member is reported as one that does not know the patient, for a
    /// deployment under Regulation (EU) 2025/327 Art 8, where the fact of a
    /// restriction "shall not be visible to healthcare providers".
    Withheld,
}

impl ConsentDisclosure {
    /// The disclosure `[federation.consent] disclose` sets.
    #[must_use]
    pub fn of(disclose: bool) -> Self {
        if disclose {
            Self::Disclosed
        } else {
            Self::Withheld
        }
    }

    /// Whether an answer names the exclusion.
    #[must_use]
    pub fn is_disclosed(self) -> bool {
        self == Self::Disclosed
    }
}

/// The localizer's failure policy and budget, resolved (§14.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizationSettings {
    /// What the gateway does when the localizer does not answer.
    pub on_failure: OnFailure,
    /// How long the localizer may take, already known to end before the
    /// overall budget.
    pub timeout: Duration,
}

/// The HTTP surface, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// The socket address to bind.
    pub listen: SocketAddr,
    /// The path every route sits under (§4.1, N28).
    pub base_path: BasePath,
    /// How long one request may take before the server answers `408`.
    pub request_timeout: Duration,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout: Duration,
    /// The largest request body the server reads before answering `413`.
    pub body_limit: usize,
    /// Who may call the ITS-REST surface and `OPTIONS {base}/`, from
    /// `[auth]` (§13.1, N25).
    pub auth: AuthSettings,
}

/// The console and the trace export, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// The rendering.
    pub format: Format,
    /// The `tracing` filter directive, already known to parse.
    pub filter: String,
    /// The OTLP collector the spans are exported to, already known to be an
    /// `http://` URL; `None` exports nothing.
    pub otlp_endpoint: Option<SecretUrl>,
    /// The share of the gateway's traces that are sampled.
    pub trace_sample_ratio: SampleRatio,
}

/// The metrics surface, resolved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetricsSettings {
    /// The admin listener's address, already held to loopback unless remote
    /// serving was allowed; `None` runs no listener.
    pub listen: Option<SocketAddr>,
    /// The OTLP collector the metrics are pushed to; `None` pushes nothing.
    pub otlp_endpoint: Option<SecretUrl>,
}

/// The authentication scheme a credentials section resolves to.
///
/// `Debug` redacts every secret, because [`Secret`] does.
#[derive(Debug)]
#[non_exhaustive]
pub enum Scheme {
    /// An RFC 6750 bearer token.
    Bearer(Secret),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: Secret,
    },
    /// An OAuth 2.0 client-credentials grant with a JWT client assertion
    /// (RFC 6749 §4.4, RFC 7523 §2.2).
    OAuth2(Box<Grant>),
    /// A grant under the FAPI 2.0 Security Profile, the track of Annex B
    /// §B.4a: a `DPoP`-bound token for an ES256 `private_key_jwt` assertion,
    /// with RFC 9396 `authorization_details` where configured.
    Fapi2(Box<Fapi2Grant>),
    /// A grant a binding adds, such as the Nuts grant of Annex B §B.4.
    Binding(Box<dyn OnwardGrant>),
}

impl Scheme {
    /// Whether the scheme is a grant, which only a node's onward credentials
    /// take; an identity, localization, consent or directory service takes a
    /// bearer token or basic credentials.
    #[must_use]
    pub fn is_grant(&self) -> bool {
        matches!(self, Self::OAuth2(_) | Self::Fapi2(_) | Self::Binding(_))
    }
}

impl Settings {
    /// Whether the gateway federates: a registry document or a binding's
    /// registry source names its members.
    #[must_use]
    pub fn federates(&self) -> bool {
        self.registry_document.is_some() || crate::binding::sources_registry(self)
    }

    /// Logs what this process is configured to reach, never a value.
    ///
    /// The line names the endpoints that carry credentials and never the
    /// credentials, so a start-up log states what the process can reach
    /// without stating any of it; each binding then logs what it reaches.
    pub fn log_summary(&self) {
        let endpoints: Vec<&str> = self.credentials.keys().map(EndpointId::as_str).collect();
        let decomposable: Vec<&str> = self
            .federation
            .decomposable
            .iter()
            .map(|function| function.name())
            .collect();
        let bindings: Vec<&str> = crate::binding::compiled()
            .iter()
            .map(|binding| binding.name())
            .collect();
        tracing::info!(
            listen = %self.server.listen,
            base_path = %self.server.base_path,
            profile = ?self.profile,
            registry = self.registry_document.is_some(),
            registry_format = ?self.registry_format,
            federation_id = self.federation.id.as_ref().map(FederationId::as_str),
            node_selection = ?self.federation.node_selection,
            best_effort = self.federation.best_effort,
            offset_strategy = self.federation.offset.name(),
            max_offset_window = self.federation.offset.max_window().map(NonZeroU32::get),
            decomposable_aggregates = decomposable.join(","),
            demographic_endpoint = self
                .federation
                .demographic_endpoint
                .as_ref()
                .map(EndpointId::as_str),
            fan_out_template_upload = self.federation.fan_out_template_upload,
            fan_out_stored_queries = self.federation.fan_out_stored_queries,
            consent_disclose = self.federation.consent_disclosure.is_disclosed(),
            stored_query_backend = self
                .stored_queries
                .as_ref()
                .map(|store| store.backend().name()),
            metrics_listen = self.metrics.listen.map(|address| address.to_string()),
            metrics_otlp_push = self.metrics.otlp_endpoint.is_some(),
            traces_otlp_export = self.telemetry.otlp_endpoint.is_some(),
            trace_sample_ratio = self.telemetry.trace_sample_ratio.get(),
            credentials = endpoints.join(","),
            auth_issuers = self.server.auth.issuers.len(),
            auth_edge = matches!(self.server.auth.mode, crate::config::auth::AuthMode::Edge(_)),
            purpose_of_use_required = self.server.auth.purpose_required,
            bindings = bindings.join(","),
            "configuration resolved"
        );
        for binding in crate::binding::compiled() {
            binding.log_summary(self);
        }
    }
}
