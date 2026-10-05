// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The metrics surface: one `OpenTelemetry` meter provider, read by the
//! Prometheus text exposition of `GET /metrics` on the admin listener and,
//! when a collector is configured, pushed over OTLP.
//!
//! One provider feeds both readers, so an instrument cannot exist on one
//! surface and not the other. Every label value is drawn from a closed set
//! or from the registry: `kind` from the integrity incident kinds, `outcome`
//! from the §11.1 statuses or, for the consent pre-filter's calls, from its
//! three decisions, `result` from the reload outcomes, `endpoint` from the
//! registry's endpoint ids, the inbound request labels from the method set
//! and the route templates ([`inbound`]), `event` and `reason` from the
//! security events and the refusal reasons ([`security`]), and `limit` from
//! the overload limits ([`nodes::Limit`]). Nothing a request carries becomes
//! a label, so no patient identifier and no client text can reach the
//! surface (§5.4.1, N33). Each binding adds its own instruments under the
//! same rule ([`crate::binding::process::Instruments`]). The instruments
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

pub mod inbound;
pub mod nodes;
pub mod resolver;
pub mod security;

use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use ferrofed_registry::incident::Kind;
use http::{HeaderValue, StatusCode, header};
use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, MeterProvider as _, ObservableCounter};
use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider, Temporality};

use crate::config::settings::MetricsSettings;
use crate::metrics::nodes::Instruments;

/// The instrumentation scope every instrument is created under.
pub const SCOPE: &str = "ferrofed";

/// The path the admin listener serves the Prometheus text exposition at.
pub const PATH: &str = "/metrics";

/// The integrity incidents emitted, by `kind`; Prometheus
/// `ferrofed_integrity_incidents_total`.
pub const INTEGRITY_INCIDENTS: &str = "ferrofed.integrity.incidents";

/// The requests sent to a member node, by `endpoint` and `outcome`;
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

/// The upper bounds of the node request duration buckets, in seconds: 5 ms
/// to 30 s, past the default per-node timeout of 10 s.
pub const NODE_DURATION_BUCKETS: [f64; 12] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0,
];

/// The media type of the Prometheus text exposition format, version 0.0.4
/// (<https://prometheus.io/docs/instrumenting/exposition_formats/>).
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

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

/// The metrics surface could not be built, rendered or flushed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MetricsError {
    /// The OTLP exporter could not be built for `metrics.otlp_endpoint`.
    #[error("the OTLP metrics exporter could not be built")]
    Otlp(#[source] opentelemetry_otlp::ExporterBuildError),
    /// The Prometheus reader could not be registered.
    #[error("the Prometheus metrics reader could not be built")]
    Prometheus(#[source] opentelemetry_sdk::error::OTelSdkError),
    /// The gathered metrics could not be encoded as text.
    #[error("the metrics could not be encoded")]
    Encode(#[source] prometheus::Error),
    /// The encoded metrics are not UTF-8.
    #[error("the encoded metrics are not UTF-8")]
    Utf8(#[source] std::string::FromUtf8Error),
    /// The meter provider could not flush or stop its readers.
    #[error("the meter provider could not shut down")]
    Shutdown(#[source] opentelemetry_sdk::error::OTelSdkError),
}

/// The meter provider, the Prometheus registry its pull reader writes into,
/// and the instruments the gateway records through.
pub struct Metrics {
    provider: SdkMeterProvider,
    registry: prometheus::Registry,
    nodes: Instruments,
    inbound: inbound::Instruments,
    reloads: Counter<u64>,
    /// Kept for the life of the provider: its callback reads the security
    /// event counts at each collection.
    _security: ObservableCounter<u64>,
    /// Kept for the life of the provider: its callback reads the incident
    /// counts at each collection.
    _incidents: ObservableCounter<u64>,
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
}

impl Metrics {
    /// Returns the metrics `settings` describe: the Prometheus reader always,
    /// and the OTLP push when `settings.otlp_endpoint` is set.
    ///
    /// The OTLP exporter speaks gRPC through `tonic`, so with a collector
    /// configured this runs inside a Tokio runtime context.
    ///
    /// # Errors
    /// Returns [`MetricsError::Otlp`] when the OTLP exporter cannot be built,
    /// and [`MetricsError::Prometheus`] when the pull reader cannot be.
    pub fn new(settings: &MetricsSettings) -> Result<Self, MetricsError> {
        let otlp = settings
            .otlp_endpoint
            .as_ref()
            .map(|endpoint| {
                opentelemetry_otlp::MetricExporter::builder()
                    .with_tonic()
                    .with_endpoint(endpoint.expose())
                    .with_temporality(Temporality::Cumulative)
                    .build()
                    .map(|exporter| PeriodicReader::builder(exporter).build())
                    .map_err(MetricsError::Otlp)
            })
            .transpose()?;
        Self::with_readers(otlp)
    }

    /// Builds the provider over the Prometheus reader and `otlp`, and the
    /// instruments over the provider.
    fn with_readers(
        otlp: Option<PeriodicReader<opentelemetry_otlp::MetricExporter>>,
    ) -> Result<Self, MetricsError> {
        let registry = prometheus::Registry::new();
        // NOTE: no specification governs this: our own design; the scope label
        // is left off, so every series carries only the labels listed above.
        let pull = opentelemetry_prometheus::exporter()
            .with_registry(registry.clone())
            .scope_info_enabled(false)
            .build()
            .map_err(MetricsError::Prometheus)?;
        // NOTE: no specification governs this: our own design; one provider
        // feeds both readers, so the pull and the push expose the same metrics.
        let mut builder = SdkMeterProvider::builder()
            .with_resource(crate::telemetry::resource())
            .with_reader(pull);
        if let Some(reader) = otlp {
            builder = builder.with_reader(reader);
        }
        let provider = builder.build();
        let meter = provider.meter(SCOPE);
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
        let nodes = Instruments::new(&meter);
        let inbound = inbound::Instruments::new(&meter);
        let security = security::observe(&meter);
        let bindings = crate::binding::process::Instruments::new(&meter);
        Ok(Self {
            provider,
            registry,
            nodes,
            inbound,
            reloads,
            _security: security,
            _incidents: incidents,
            bindings,
        })
    }

    /// Returns the node request instruments a federation records through.
    #[must_use]
    pub fn nodes(&self) -> Instruments {
        self.nodes.clone()
    }

    /// Returns the inbound request instruments the request log records
    /// through.
    #[must_use]
    pub fn inbound(&self) -> inbound::Instruments {
        self.inbound.clone()
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

    /// Returns the instruments the bindings record through.
    #[cfg(feature = "binding-ihe")]
    pub(crate) fn bindings(&self) -> &crate::binding::process::Instruments {
        &self.bindings
    }

    /// Returns the Prometheus text exposition of every instrument.
    ///
    /// # Errors
    /// Returns [`MetricsError::Encode`] when the gathered families cannot be
    /// encoded, and [`MetricsError::Utf8`] when the encoding is not UTF-8.
    pub fn render(&self) -> Result<String, MetricsError> {
        use prometheus::Encoder as _;

        let mut text = Vec::new();
        prometheus::TextEncoder::new()
            .encode(&self.registry.gather(), &mut text)
            .map_err(MetricsError::Encode)?;
        String::from_utf8(text).map_err(MetricsError::Utf8)
    }

    /// Flushes the OTLP push, when configured, and stops every reader.
    ///
    /// # Errors
    /// Returns [`MetricsError::Shutdown`] when a reader cannot flush or stop.
    pub fn shutdown(&self) -> Result<(), MetricsError> {
        self.provider.shutdown().map_err(MetricsError::Shutdown)
    }
}

impl Default for Metrics {
    /// Returns the Prometheus reader alone, with no OTLP push.
    #[expect(
        clippy::expect_used,
        reason = "the pull reader registers one collector in a registry built empty beside it, so the registration cannot collide"
    )]
    fn default() -> Self {
        Self::with_readers(None)
            .expect("the Prometheus reader should register in its own empty registry")
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
pub fn router(metrics: Arc<Metrics>) -> Router {
    routes(metrics).fallback(|| async { StatusCode::NOT_FOUND })
}

/// Returns the `GET /metrics` route over `metrics` alone, with no fallback,
/// for the admin listener to merge beside its other routes.
pub fn routes(metrics: Arc<Metrics>) -> Router {
    Router::new()
        .route(PATH, get(exposition))
        .with_state(metrics)
}

/// `GET /metrics`: the text exposition, or `500` when it cannot be encoded.
async fn exposition(State(metrics): State<Arc<Metrics>>) -> Response {
    match metrics.render() {
        Ok(text) => (
            [(header::CONTENT_TYPE, HeaderValue::from_static(CONTENT_TYPE))],
            text,
        )
            .into_response(),
        Err(error) => {
            tracing::error!(error = %crate::chain(&error), "the metrics could not be rendered");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
