// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The metrics surface: the gateway's instruments on the one meter provider.
//!
//! The provider is `ferrohealth-telemetry`'s, read by the Prometheus text
//! exposition of `GET /metrics` on the admin listener and, under
//! `[telemetry.otlp]`, pushed over OTLP. One provider feeds both readers, so
//! an instrument cannot exist on one surface and not the other. Every label value is drawn from a closed set
//! or from the registry: `kind` from the integrity incident kinds, `outcome`
//! from the §11.1 statuses or, for the consent pre-filter's calls, from its
//! three decisions, `result` from the reload outcomes and from what a
//! replica did with a revision another one marked running, `endpoint` from
//! the registry's endpoint ids, the inbound request labels from the method set
//! and the route templates ([`inbound`]), `event` and `reason` from the
//! security events and the refusal reasons ([`security`]), and `limit` from
//! the overload limits ([`nodes::Limit`]). Nothing a request carries becomes
//! a label, so no patient identifier and no client text can reach the
//! surface (§5.4.1, N33). The audit spools are read with no label at all
//! ([`crate::audit::metrics`]), and each binding adds its own instruments
//! under the same rule ([`crate::binding::process::Instruments`]). The instruments
//! fill from what the gateway already observes: each request it answers, an
//! incident's event, the per-endpoint report of each node request, each
//! resolver call, each security event, each refusal of a limit, and a
//! registry reload's outcome. No specification governs metrics: our own
//! design.
//!
//! Instrument names follow the `OpenTelemetry` naming convention, dotted and
//! without a unit or a `_total`; the Prometheus exporter writes `.` as `_`,
//! appends `_total` to a counter and the unit to a histogram
//! (<https://opentelemetry.io/docs/specs/otel/compatibility/prometheus_and_openmetrics/>).

pub mod dispensations;
pub mod inbound;
pub mod latency;
pub mod nodes;
pub mod resolver;
pub mod security;

use std::sync::Arc;

use axum::Router;
use ferrofed_registry::incident::Kind;
use ferrohealth_telemetry::section::TelemetrySection;
use ferrohealth_telemetry::telemetry::{InitError, Service, Telemetry, prometheus_route};
use ferrohealth_usage_report::window::Window;
use http::StatusCode;
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, ObservableCounter};

use crate::metrics::nodes::Instruments;

/// The instrumentation scope every instrument is created under.
pub const SCOPE: &str = "ferrofed";

/// The path the admin listener serves the Prometheus text exposition at.
pub const PATH: &str = "/metrics";

/// The integrity incidents emitted, by `kind`; Prometheus
/// `ferrofed_integrity_incidents_total`.
pub const INTEGRITY_INCIDENTS: &str = "ferrofed.integrity.incidents";

/// The requests sent to a member node, by `endpoint`, `outcome` and `class`;
/// Prometheus `ferrofed_node_requests_total`.
pub const NODE_REQUESTS: &str = "ferrofed.node.requests";

/// The time a member node took to answer, by `endpoint`; Prometheus
/// `ferrofed_node_request_duration_seconds`.
pub const NODE_REQUEST_DURATION: &str = "ferrofed.node.request.duration";

/// The calls to the consent pre-filter, by `outcome`; Prometheus
/// `ferrofed_consent_prefilter_requests_total`.
///
/// The outcome is `denied`, `no-signal`, `not-asked`, `unavailable` or
/// `partial`, and a `not-asked` call carries a `reason`: `namespace`,
/// `caller-claims`, `caller-claims-invalid` or `patient-value`.
pub const CONSENT_PREFILTER_REQUESTS: &str = "ferrofed.consent.prefilter.requests";

/// The calls to the localizer, by `outcome` (`candidates`, `no-records`,
/// `not-configured`, `unavailable` or `audit-failed`); Prometheus
/// `ferrofed_localizer_requests_total`.
pub const LOCALIZER_REQUESTS: &str = "ferrofed.localizer.requests";

/// The calls to the demographics service, by `outcome` (`identified`,
/// `no-match`, `ambiguous`, `unavailable` or `audit-failed`); Prometheus
/// `ferrofed_demographics_requests_total`.
pub const DEMOGRAPHICS_REQUESTS: &str = "ferrofed.demographics.requests";

/// The time the localizer took to answer a call; Prometheus
/// `ferrofed_localizer_request_duration_seconds`.
pub const LOCALIZER_REQUEST_DURATION: &str = "ferrofed.localizer.request.duration";

/// The time the demographics service took to answer a call; Prometheus
/// `ferrofed_demographics_request_duration_seconds`.
pub const DEMOGRAPHICS_REQUEST_DURATION: &str = "ferrofed.demographics.request.duration";

/// The calls to the cross-reference resolver, by `outcome` (`resolved`,
/// `not-resolved`, `unavailable` or `time-out`); Prometheus
/// `ferrofed_resolver_requests_total`.
pub const RESOLVER_REQUESTS: &str = "ferrofed.resolver.requests";

/// The time the cross-reference resolver took to answer a call; Prometheus
/// `ferrofed_resolver_request_duration_seconds`.
pub const RESOLVER_REQUEST_DURATION: &str = "ferrofed.resolver.request.duration";

/// The requests a limit of the gateway refused, by `limit`
/// (`concurrency`, `caller-rate` or `node-in-flight`, the last with its
/// `endpoint`); Prometheus `ferrofed_overload_refusals_total`.
pub const OVERLOAD_REFUSALS: &str = "ferrofed.overload.refusals";

/// The registry reloads, by `result`; Prometheus
/// `ferrofed_registry_reloads_total`.
pub const REGISTRY_RELOADS: &str = "ferrofed.registry.reloads";

/// Under `config.ui = "managed"`, the revisions another replica marked
/// running that this replica applied or refused, by `result`; Prometheus
/// `ferrofed_config_revision_follows_total`.
pub const CONFIG_REVISION_FOLLOWS: &str = "ferrofed.config.revision.follows";

/// Under `config.ui = "managed"`, `1` while this replica runs the revision
/// the store marks running and `0` while it does not; Prometheus
/// `ferrofed_config_revision_in_step`.
pub const CONFIG_REVISION_IN_STEP: &str = "ferrofed.config.revision.in_step";

/// The upper bounds of the node request duration buckets, in seconds: 5 ms
/// to 30 s, past the default per-node timeout of 10 s.
pub const NODE_DURATION_BUCKETS: [f64; 12] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// How a registry reload ended, the `result` label of
/// [`REGISTRY_RELOADS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadResult {
    /// The reloaded registry replaced the running one.
    Applied,
    /// The reload was refused, and the running registry stayed.
    Refused,
}

impl ReloadResult {
    /// Every result, in declaration order.
    pub const ALL: [Self; 2] = [Self::Applied, Self::Refused];

    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Refused => "refused",
        }
    }
}

/// What this replica did with a revision another replica marked running,
/// the `result` label of [`CONFIG_REVISION_FOLLOWS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowResult {
    /// The replica applied the revision.
    Applied,
    /// The replica refused the revision and kept its running configuration.
    Refused,
}

impl FollowResult {
    /// The label value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Refused => "refused",
        }
    }
}

/// Returns the service the gateway's telemetry describes: `ferrofed` at
/// this build's version, exporting the spans of every `ferrofed` crate.
#[must_use]
pub fn service() -> Service {
    Service::new(SCOPE, crate::body::VERSION)
}

/// The telemetry whose provider the gateway's instruments sit on, and the
/// instruments the gateway records through.
pub struct Metrics {
    telemetry: Arc<Telemetry>,
    nodes: Instruments,
    requests: inbound::Requests,
    reloads: Counter<u64>,
    follows: Counter<u64>,
    in_step: Gauge<u64>,
    /// Kept for the life of the provider: its callback reads the security
    /// event counts at each collection.
    _security: ObservableCounter<u64>,
    /// Kept for the life of the provider: its callback reads the dispensation
    /// counts at each collection.
    _dispensations: ObservableCounter<u64>,
    /// Kept for the life of the provider: its callback reads the incident
    /// counts at each collection.
    _incidents: ObservableCounter<u64>,
    /// Kept for the life of the provider: their callbacks read the audit
    /// spools at each collection.
    _audit: crate::audit::metrics::Instruments,
    /// The instruments each binding records through, kept for the life of
    /// the provider.
    #[cfg_attr(
        not(feature = "binding-ihe"),
        expect(
            dead_code,
            reason = "no compiled binding records through instruments of its own"
        )
    )]
    bindings: crate::binding::process::Instruments,
    /// The window of the installation report, which the request log and the
    /// node requests record into beside the instruments.
    usage: Arc<Window>,
}

impl Metrics {
    /// Returns the gateway's instruments on the meter provider of
    /// `telemetry`, with `usage` the window of the installation report the
    /// request log and the node requests record into.
    #[must_use]
    pub fn new(telemetry: Arc<Telemetry>, usage: Window) -> Self {
        let meter = telemetry.meter(SCOPE);
        let incidents = meter
            .u64_observable_counter(INTEGRITY_INCIDENTS)
            .with_description("Integrity incidents the gateway emitted, by kind")
            .with_callback(|observer| {
                for kind in Kind::ALL {
                    observer.observe(kind.emitted(), &[KeyValue::new("kind", kind.as_str())]);
                }
            })
            .build();
        let reloads = meter
            .u64_counter(REGISTRY_RELOADS)
            .with_description("Registry reloads, applied or refused")
            .build();
        for result in ReloadResult::ALL {
            reloads.add(0, &[KeyValue::new("result", result.as_str())]);
        }
        let follows = meter
            .u64_counter(CONFIG_REVISION_FOLLOWS)
            .with_description(
                "Configuration revisions another replica marked running, applied or refused here",
            )
            .build();
        for result in [FollowResult::Applied, FollowResult::Refused] {
            follows.add(0, &[KeyValue::new("result", result.as_str())]);
        }
        let in_step = meter
            .u64_gauge(CONFIG_REVISION_IN_STEP)
            .with_description(
                "Whether this replica runs the configuration revision the store marks running",
            )
            .build();
        let usage = Arc::new(usage);
        let nodes = Instruments::new(&meter).with_usage(Arc::clone(&usage));
        let requests = inbound::Requests::new(&meter);
        let security = security::observe(&meter);
        let dispensations = dispensations::observe(&meter);
        let bindings = crate::binding::process::Instruments::new(&meter);
        Self {
            telemetry,
            nodes,
            requests,
            reloads,
            follows,
            in_step,
            _security: security,
            _dispensations: dispensations,
            _incidents: incidents,
            _audit: crate::audit::metrics::Instruments::new(&meter),
            bindings,
            usage,
        }
    }

    /// Returns the telemetry whose provider the instruments sit on.
    #[must_use]
    pub const fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }

    /// Returns the window of the installation report.
    #[must_use]
    pub const fn usage(&self) -> &Arc<Window> {
        &self.usage
    }

    /// Returns the node request instruments a federation records through.
    #[must_use]
    pub fn nodes(&self) -> Instruments {
        self.nodes.clone()
    }

    /// Returns the counter of the requests the gateway served, which the
    /// request log feeds.
    #[must_use]
    pub fn requests(&self) -> inbound::Requests {
        self.requests.clone()
    }

    /// Counts one request `limit` refused.
    pub fn shed(&self, limit: nodes::Limit) {
        self.nodes.shed(limit);
    }

    /// Counts a registry reload that ended as `result`.
    pub fn reloaded(&self, result: ReloadResult) {
        self.reloads
            .add(1, &[KeyValue::new("result", result.as_str())]);
    }

    /// Counts a revision another replica marked running that this replica
    /// handled as `result`.
    pub fn followed(&self, result: FollowResult) {
        self.follows
            .add(1, &[KeyValue::new("result", result.as_str())]);
    }

    /// Records whether this replica runs the revision the store marks
    /// running.
    pub fn in_step(&self, in_step: bool) {
        self.in_step.record(u64::from(in_step), &[]);
    }

    /// Returns the instruments the bindings record through.
    #[cfg(feature = "binding-ihe")]
    pub(crate) fn bindings(&self) -> &crate::binding::process::Instruments {
        &self.bindings
    }

    /// Returns the Prometheus text exposition of every instrument.
    ///
    /// # Errors
    /// Returns the [`InitError`] the metrics could not be encoded with.
    pub fn render(&self) -> Result<String, InitError> {
        self.telemetry.render_prometheus()
    }
}

impl Default for Metrics {
    /// Returns the instruments on a provider with the Prometheus reader alone,
    /// with no OTLP push.
    #[expect(
        clippy::expect_used,
        reason = "with no [telemetry.otlp] the provider builds the pull reader alone, in a registry built empty beside it, so the registration cannot collide"
    )]
    fn default() -> Self {
        let telemetry = Telemetry::new(&TelemetrySection::default(), service())
            .expect("the Prometheus reader should register in its own empty registry");
        Self::new(
            Arc::new(telemetry),
            crate::usage_report::group::window(crate::base_path::BasePath::default(), None),
        )
    }
}

impl std::fmt::Debug for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Metrics").finish_non_exhaustive()
    }
}

/// Builds the admin listener's application: `GET /metrics` answers the
/// Prometheus text exposition of `metrics`, and every other path `404`.
///
/// The listener carries no other route and no authentication, so it binds a
/// loopback address unless the operator allows a remote one.
pub fn router(metrics: &Metrics) -> Router {
    routes(metrics).fallback(|| async { StatusCode::NOT_FOUND })
}

/// Returns the `GET /metrics` route over `metrics` alone, with no fallback,
/// for the admin listener to merge beside its other routes.
pub fn routes(metrics: &Metrics) -> Router {
    Router::new().route(PATH, prometheus_route(Arc::clone(metrics.telemetry())))
}
