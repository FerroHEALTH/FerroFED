// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The settings the run path holds, resolved from the configuration tree.

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::time::Duration;

use ferrofed_engine::fanout::Budget;
use ferrofed_identity::dev::Profile;
use ferrofed_registry::id::EndpointId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::OffsetStrategy;
use openehr_federation::id::FederationId;
use secrecy::SecretString;

use crate::config::{DevSection, NodeSelection, RegistryFormat};
use crate::telemetry::Format;

/// The settings the run path holds, with every secret already read.
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
    /// The federated query.
    pub federation: FederationSettings,
    /// The outbound credentials, by endpoint id.
    pub credentials: BTreeMap<EndpointId, Scheme>,
    /// The static development cross-reference, as written.
    pub dev: Option<DevSection>,
    /// The PIXm resolver, with every secret read.
    pub pixm: Option<PixmSettings>,
    /// The store file of the stored-query registry, when it is offered
    /// (§12.7).
    pub stored_queries: Option<PathBuf>,
}

/// The PIXm resolver, resolved.
#[derive(Debug)]
pub struct PixmSettings {
    /// The PIX Managers.
    pub managers: Vec<PixManagerSettings>,
    /// A client's issuing namespace mapped to a PIX assigning authority.
    pub namespaces: BTreeMap<String, String>,
}

/// One PIX Manager, resolved.
#[derive(Debug)]
pub struct PixManagerSettings {
    /// The Manager's FHIR base URL.
    pub url: url::Url,
    /// Each member it resolves, mapped to that member's `ehr_id` domain.
    pub members: BTreeMap<String, String>,
    /// How the gateway authenticates to it.
    pub credentials: Option<Scheme>,
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
    /// How many `ehr_id`s the `ehr_id` to node index holds.
    pub ehr_index_capacity: NonZeroU32,
    /// How the node set of an undirected patient query is chosen, as
    /// declared; a federation refuses to load without it.
    pub node_selection: Option<NodeSelection>,
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
}

/// The HTTP surface, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    /// The socket address to bind.
    pub listen: SocketAddr,
    /// How long one request may take before the server answers `408`.
    pub request_timeout: Duration,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout: Duration,
    /// The largest request body the server reads before answering `413`.
    pub body_limit: usize,
}

/// The console, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetrySettings {
    /// The rendering.
    pub format: Format,
    /// The `tracing` filter directive, already known to parse.
    pub filter: String,
}

/// The authentication scheme a credentials section resolves to.
///
/// `Debug` redacts every secret, because [`SecretString`] does.
#[derive(Debug)]
#[non_exhaustive]
pub enum Scheme {
    /// An RFC 6750 bearer token.
    Bearer(SecretString),
    /// RFC 7617 basic authentication.
    Basic {
        /// The user name, which is not a secret.
        user: String,
        /// The password.
        password: SecretString,
    },
}

impl Settings {
    /// Logs what this process is configured to reach, never a value.
    ///
    /// The line names the endpoints that carry credentials and never the
    /// credentials, so a start-up log states what the process can reach
    /// without stating any of it.
    pub fn log_summary(&self) {
        let endpoints: Vec<&str> = self.credentials.keys().map(EndpointId::as_str).collect();
        let decomposable: Vec<&str> = self
            .federation
            .decomposable
            .iter()
            .map(|function| function.name())
            .collect();
        tracing::info!(
            listen = %self.server.listen,
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
            pix_managers = self.pixm.as_ref().map_or(0, |pixm| pixm.managers.len()),
            stored_query_registry = self.stored_queries.is_some(),
            credentials = endpoints.join(","),
            "configuration resolved"
        );
    }
}
