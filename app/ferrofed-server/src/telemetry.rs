// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The console: the format choice and the process-wide subscriber.
//!
//! Two renderings over `tracing`: `pretty` for a person and `json` for a log
//! pipeline, one object per line. `auto` picks `pretty` when stdout is a
//! terminal and `json` otherwise, so a container emits machine lines with no
//! configuration. No specification governs the console: our own design.

use serde::Deserialize;
use std::io;
use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
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
    /// terminal.
    ///
    /// An explicit `pretty` keeps its colour into a pipe, because a person
    /// asked for it; `auto` and `json` follow the terminal.
    #[must_use]
    pub const fn colour(self, stdout_is_terminal: bool) -> bool {
        matches!(self, Self::Pretty) || stdout_is_terminal
    }
}

/// A subscriber could not be built or installed.
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
}

/// Builds the subscriber for `rendering` with `filter`, writing through
/// `writer`.
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
    let filter = EnvFilter::try_new(filter).map_err(|source| Error::Filter { source })?;
    let layer: Box<dyn tracing_subscriber::Layer<_> + Send + Sync> = match rendering {
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
    Ok(tracing_subscriber::registry().with(filter).with(layer))
}

/// Installs the process-wide subscriber on stdout and returns its rendering.
///
/// # Errors
/// Returns [`Error::Filter`] when `filter` does not parse and
/// [`Error::AlreadyInstalled`] when this process already has a subscriber.
pub fn init(format: Format, filter: &str, stdout_is_terminal: bool) -> Result<Rendering, Error> {
    let rendering = format.resolve(stdout_is_terminal);
    subscriber(
        rendering,
        filter,
        format.colour(stdout_is_terminal),
        io::stdout,
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
