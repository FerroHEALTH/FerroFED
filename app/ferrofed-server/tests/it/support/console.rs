// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console a test captures: `ferrohealth-telemetry`'s, built as `serve`
//! builds it, its security events written whatever the filter says, into a
//! writer the test reads.

use std::ffi::OsStr;

use ferrofed_server::facade::security::DIRECTIVE;
use ferrofed_server::metrics::service;
use ferrohealth_telemetry::section::{Filter, Format, Rendering, TelemetrySection};
use ferrohealth_telemetry::telemetry::{Console, InitError, Layers, Telemetry};
use opentelemetry_sdk::trace::SdkTracer;
use tracing::Subscriber;
use tracing_subscriber::Layer as _;
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt as _;

/// The subscriber for `rendering` with `filter`, writing colour when `ansi`
/// holds, through `writer`, with no span export.
///
/// # Errors
/// Returns [`InitError::Filter`] when `filter` does not parse.
pub(crate) fn subscriber<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
) -> Result<impl Subscriber + Send + Sync + use<W>, InitError>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    traced(rendering, filter, ansi, writer, None)
}

/// The subscriber of [`subscriber`], exporting the gateway's spans through
/// `tracer` when one is given, as the telemetry of `serve` exports them.
///
/// # Errors
/// Returns [`InitError::Filter`] when `filter` does not parse.
pub(crate) fn traced<W>(
    rendering: Rendering,
    filter: &str,
    ansi: bool,
    writer: W,
    tracer: Option<SdkTracer>,
) -> Result<impl Subscriber + Send + Sync + use<W>, InitError>
where
    W: for<'w> MakeWriter<'w> + Send + Sync + 'static,
{
    let section = TelemetrySection {
        format: match rendering {
            Rendering::Json => Format::Json,
            Rendering::Pretty => Format::Pretty,
        },
        filter: Filter::new(filter)?,
        ..TelemetrySection::default()
    };
    let no_color = (!ansi).then_some(OsStr::new("1"));
    let console = Console::new(false, no_color).force(Filter::new(DIRECTIVE)?);
    let telemetry = Telemetry::new(&TelemetrySection::default(), service())?;
    let Layers { layers, .. } = telemetry.layers(&section, &console, writer)?;
    let exported = service();
    let export = tracer.map(move |tracer| {
        tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_threads(false)
            .with_location(false)
            .with_filter(FilterFn::new(move |metadata| exported.exports(metadata)))
    });
    Ok(tracing_subscriber::registry().with(layers).with(export))
}
