// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console, the trace export, and the process-wide subscriber that
//! carries both.
//!
//! Two renderings over `tracing`: `pretty` for a person and `json` for a log
//! pipeline, one object per line. `auto` picks `pretty` when stdout is a
//! terminal and `json` otherwise, so a container emits machine lines with no
//! configuration.
//!
//! When `telemetry.otlp_endpoint` names a collector, the gateway's own spans
//! are exported to it as OpenTelemetry traces over OTLP ([`Traces`]), under
//! the same resource as the metrics push. Only spans leave ([`spans`]): no
//! `tracing` event is exported, and no span of another crate. Every span field
//! is a route template, a registry id, an ITS-REST `operationId`, a status or
//! a count, never a body, a query text, a header value or a patient
//! identifier (§5.4.1, §5.4.3, N33). The console filter decides what is
//! logged and never what is traced. No specification governs telemetry: our
//! own design.

use ferrofed_registry::secret::SecretUrl;
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig as _;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::trace::{Sampler, SdkTracer, SdkTracerProvider};
use serde::Deserialize;
use std::ffi::OsStr;
use std::io;
use tracing::{Metadata, Subscriber};
use tracing_subscriber::filter::{EnvFilter, FilterFn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::util::SubscriberInitExt;

/// The filter the server runs with when the configuration names none.
///
/// The HTTP stack's own crates are quiet, so the log carries this server's
/// lines.
pub const DEFAULT_FILTER: &str = "info,hyper=warn,tower=warn,h2=warn";

/// The rendering a deployment asks for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase", deny_unknown_fields)]
pub enum Format {
    /// `pretty` on a terminal, `json` otherwise.
    #[default]
    Auto,
    /// One JSON object per line.
    Json,
    /// Human-readable lines.
    Pretty,
}

/// The rendering after `auto` is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rendering {
    /// One JSON object per line.
    Json,
    /// Human-readable lines.
    Pretty,
}

impl Format {
    /// Decides `auto` from whether stdout is a terminal.
    ///
    /// The caller passes the answer rather than reading it, so a test fixes
    /// the decision without owning a terminal.
    #[must_use]
    pub const fn resolve(self, stdout_is_terminal: bool) -> Rendering {
        match self {
            Self::Pretty => Rendering::Pretty,
            Self::Auto if stdout_is_terminal => Rendering::Pretty,
            Self::Json | Self::Auto => Rendering::Json,
        }
    }

    /// Decides whether the console writes colour, from whether stdout is a
    /// terminal and the value of the `NO_COLOR` environment variable.
    ///
    /// A `NO_COLOR` that is set and not empty switches colour off, whatever
    /// the format and the terminal (<https://no-color.org>). Otherwise an
    /// explicit `pretty` keeps its colour into a pipe, because a person asked
    /// for it, and `auto` and `json` follow the terminal. The caller passes
    /// both facts rather than reading them, so a test fixes the decision.
    #[must_use]
    pub fn colour(self, stdout_is_terminal: bool, no_color: Option<&OsStr>) -> bool {
        if no_color.is_some_and(|value| !value.is_empty()) {
            return false;
        }
        matches!(self, Self::Pretty) || stdout_is_terminal
    }
}

/// A subscriber or the trace export could not be built, installed or
/// flushed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The filter directive does not parse.
    #[error("the log filter does not parse")]
    Filter {
        /// What the filter parser reported.
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    /// A subscriber is already installed in this process.
    #[error("a log subscriber is already installed")]
    AlreadyInstalled {
        /// What `tracing-subscriber` reported.
        #[source]
        source: tracing_subscriber::util::TryInitError,
    },
    /// The OTLP span exporter could not be built for
    /// `telemetry.otlp_endpoint`.
    #[error("the OTLP trace exporter could not be built")]
    Exporter {
        /// What `opentelemetry-otlp` reported.
        #[source]
        source: opentelemetry_otlp::ExporterBuildError,
    },
    /// The tracer provider could not flush or stop its exporter.
    #[error("the tracer provider could not shut down")]
    Shutdown {
        /// What `opentelemetry_sdk` reported.
        #[source]
        source: opentelemetry_sdk::error::OTelSdkError,
    },
}

/// Returns the resource every exported span and metric is described by: the
/// service name and its version.
#[must_use]
pub fn resource() -> Resource {
    Resource::builder()
        .with_service_name(crate::metrics::SCOPE)
        .with_attribute(KeyValue::new("service.version", crate::body::VERSION))
        .build()
}

/// The share of the gateway's traces that are sampled: a finite number from
/// `0.0`, none, to `1.0`, every one.
#[derive(Debug, Clone, Copy)]
pub struct SampleRatio(f64);

impl SampleRatio {
    /// Every trace is sampled.
    pub const ALL: Self = Self(1.0);

    /// Returns `ratio` as a sample ratio, or `None` when it is not a finite
    /// number from `0.0` to `1.0`.
    #[must_use]
    pub fn new(ratio: f64) -> Option<Self> {
        (ratio.is_finite() && (0.0..=1.0).contains(&ratio)).then_some(Self(ratio))
    }

    /// The ratio.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for SampleRatio {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for SampleRatio {}

/// Returns the sampler of the trace export: a root span is sampled by its
/// trace id at `ratio`, and every other span as its parent was.
///
/// Every root is the gateway's own request span, since a client's trace is
/// only ever a link, so `ratio` is the share of the gateway's requests whose
/// spans are exported, and a request's whole span tree is exported or none
/// of it is.
#[must_use]
pub fn sampler(ratio: SampleRatio) -> Sampler {
    Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio.get())))
}

/// The trace export: one tracer provider that sends the gateway's spans to
/// an OTLP collector in batches.
pub struct Traces {
    provider: SdkTracerProvider,
}

impl Traces {
    /// Returns the export to the OTLP collector at `endpoint`, over gRPC,
    /// sampling as [`sampler`] does at `ratio`.
    ///
    /// The exporter speaks gRPC through `tonic`, so this runs inside the
    /// Tokio runtime that will carry the export.
    ///
    /// # Errors
    /// Returns [`Error::Exporter`] when the exporter cannot be built.
    pub fn new(endpoint: &SecretUrl, ratio: SampleRatio) -> Result<Self, Error> {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint.expose())
            .build()
            .map_err(|source| Error::Exporter { source })?;
        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(resource())
            .with_sampler(sampler(ratio))
            .build();
        Ok(Self { provider })
    }

    /// Returns the tracer the [`spans`] layer records through.
    #[must_use]
    pub fn tracer(&self) -> SdkTracer {
        self.provider.tracer(crate::metrics::SCOPE)
    }

    /// Flushes every span still held and stops the exporter.
    ///
    /// # Errors
    /// Returns [`Error::Shutdown`] when the exporter cannot flush or stop.
    pub fn shutdown(&self) -> Result<(), Error> {
        self.provider
            .shutdown()
            .map_err(|source| Error::Shutdown { source })
    }
}

impl std::fmt::Debug for Traces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Traces").finish_non_exhaustive()
    }
}

/// Returns the layer that exports the gateway's own spans through `tracer`.
///
/// The layer sees only what [`exported`] admits, so no event field and no
/// span of another crate reaches the collector. It records no thread and no
/// source location.
pub fn spans<S>(tracer: SdkTracer) -> impl Layer<S>
where
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_threads(false)
        .with_location(false)
        .with_filter(FilterFn::new(exported))
}

/// Whether the trace export sees `metadata`: a span, never an event, of one
/// of the gateway's own crates.
#[must_use]
pub fn exported(metadata: &Metadata<'_>) -> bool {
    // NOTE: §5.4.3, an event may carry a value the gateway stripped; only the
    // spans whose fields are listed in the module docs leave the process.
    metadata.is_span()
        && metadata
            .target()
            .split("::")
            .next()
            .is_some_and(|krate| krate.starts_with("ferrofed_"))
}

/// Builds the subscriber for `rendering` with `filter`, writing through
/// `writer`, with no trace export.
///
/// # Errors
/// Returns [`Error::Filter`] when `filter` does not parse. The configuration
/// refuses such a filter at boot, so a running server never reaches this.
pub fn subscriber<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
) -> Result<impl Subscriber + Send + Sync + use<W>, Error>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    traced(rendering, filter, ansi, writer, None)
}

/// Builds the subscriber for `rendering` with `filter`, writing through
/// `writer`, and exporting spans through `tracer` when one is given.
///
/// `filter` decides what the console writes and nothing else, so a quieter
/// log never thins a trace.
///
/// # Errors
/// Returns [`Error::Filter`] when `filter` does not parse.
pub fn traced<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
    tracer: Option<SdkTracer>,
) -> Result<impl Subscriber + Send + Sync + use<W>, Error>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let filter = EnvFilter::try_new(filter).map_err(|source| Error::Filter { source })?;
    let console: Box<dyn Layer<tracing_subscriber::Registry> + Send + Sync> = match rendering {
        Rendering::Json => Box::new(
            tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(false)
                .with_span_list(false)
                .with_writer(writer),
        ),
        Rendering::Pretty => Box::new(
            tracing_subscriber::fmt::layer()
                .with_target(false)
                .with_ansi(ansi)
                .with_writer(writer),
        ),
    };
    Ok(tracing_subscriber::registry()
        .with(console.with_filter(filter))
        .with(tracer.map(spans)))
}

/// Installs the process-wide subscriber on stdout, exporting spans through
/// `tracer` when one is given, and returns its rendering.
///
/// The `pretty` rendering writes colour as [`Format::colour`] decides from
/// `stdout_is_terminal` and `no_color`, the value of `NO_COLOR`.
///
/// # Errors
/// Returns [`Error::Filter`] when `filter` does not parse and
/// [`Error::AlreadyInstalled`] when this process already has a subscriber.
pub fn init(
    format: Format,
    filter: &str,
    stdout_is_terminal: bool,
    no_color: Option<&OsStr>,
    tracer: Option<SdkTracer>,
) -> Result<Rendering, Error> {
    let rendering = format.resolve(stdout_is_terminal);
    traced(
        rendering,
        filter,
        format.colour(stdout_is_terminal, no_color),
        io::stdout,
        tracer,
    )?
    .try_init()
    .map_err(|source| Error::AlreadyInstalled { source })?;
    Ok(rendering)
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_FILTER, Format, Rendering};
    use tracing_subscriber::EnvFilter;

    #[test]
    fn auto_follows_the_terminal_and_an_explicit_format_does_not() {
        assert_eq!(Rendering::Pretty, Format::Auto.resolve(true));
        assert_eq!(Rendering::Json, Format::Auto.resolve(false));
        assert_eq!(Rendering::Json, Format::Json.resolve(true));
        assert_eq!(Rendering::Pretty, Format::Pretty.resolve(false));
    }

    #[test]
    fn the_default_filter_parses_and_quiets_the_http_stack() {
        assert!(EnvFilter::try_new(DEFAULT_FILTER).is_ok());
        assert!(DEFAULT_FILTER.contains("hyper=warn"));
        assert!(DEFAULT_FILTER.contains("tower=warn"));
        assert!(DEFAULT_FILTER.contains("h2=warn"));
    }
}
