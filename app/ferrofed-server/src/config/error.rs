// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of a configuration the server does not start on.

use std::path::PathBuf;

use crate::config::{ENV_PREFIX, MAX_ENDPOINT_ID_LENGTH};

/// A configuration the server refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The configuration file could not be read.
    #[error("the configuration file {} could not be read", path.display())]
    Read {
        /// The path that was tried.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// The configuration does not parse, names an unknown key, or holds a
    /// value of the wrong type.
    #[error("the configuration is not valid")]
    Parse {
        /// What the TOML reader reported, with the offending key.
        #[source]
        source: toml::de::Error,
    },
    /// The merged configuration could not be written back for re-reading.
    #[error("the configuration could not be assembled")]
    Assemble {
        /// What the TOML writer reported.
        #[source]
        source: toml::ser::Error,
    },
    /// An environment override names no key under the prefix.
    #[error("{name} names no configuration key; use {ENV_PREFIX}<SECTION>__<KEY>")]
    EnvName {
        /// The variable that was read.
        name: String,
    },
    /// An environment override addresses a key under a value that is not a
    /// section.
    #[error("{name} addresses a key under a value that is not a section")]
    EnvShape {
        /// The variable that was read.
        name: String,
    },
    /// A value and its `_file` sibling are both set.
    #[error("{key} is set together with {key}_file; set one of them")]
    Conflict {
        /// The inline key.
        key: String,
    },
    /// A `_file` sibling could not be read.
    #[error("{key} names {}, which could not be read", path.display())]
    Secret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
        /// What the file system reported.
        #[source]
        source: std::io::Error,
    },
    /// A secret read from a `_file` sibling is empty.
    #[error("{key} names {}, which holds no secret", path.display())]
    EmptySecret {
        /// The `_file` key.
        key: String,
        /// The path it named.
        path: PathBuf,
    },
    /// A key a section needs is not set.
    #[error("{key} is not set, and its section needs it")]
    Missing {
        /// The key that carries no value.
        key: String,
    },
    /// A socket address does not parse.
    #[error("{key} is not a socket address")]
    Listen {
        /// The key that holds it.
        key: String,
        /// What the address parser reported.
        #[source]
        source: std::net::AddrParseError,
    },
    /// A duration or a size that must be positive is zero.
    #[error("{key} is zero; it must be positive")]
    Zero {
        /// The key that holds it.
        key: String,
    },
    /// The log filter does not parse.
    #[error("telemetry.filter is not a valid tracing filter")]
    Filter {
        /// What the filter parser reported.
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    /// A credentials section is keyed by something that is not an endpoint id.
    #[error(
        "credentials.{key:?} is not an endpoint id: one to {MAX_ENDPOINT_ID_LENGTH} printable ASCII characters with no space"
    )]
    EndpointId {
        /// The key that was given.
        key: String,
    },
    /// A credentials section names both a bearer token and a user.
    #[error("{section} names both a bearer token and a user; set one scheme")]
    Scheme {
        /// The credentials section.
        section: String,
    },
    /// A credentials section names no scheme at all.
    #[error("{section} names no credentials; remove the section or set one scheme")]
    NoScheme {
        /// The credentials section.
        section: String,
    },
    /// The `[dev]` table does not have the shape of `[[dev.crossref]]` rows.
    ///
    /// The reader's own message is not kept: it may quote a row's value, and
    /// a row's value is a patient identifier.
    #[error(
        "the [dev] table is not valid: every [[dev.crossref]] row names namespace, value, member and ehr_id, and nothing else"
    )]
    DevTable,
    /// The fan-out budget does not end before the request timeout, so the
    /// request timeout would cut the answer and its envelope (§11.4).
    #[error(
        "federation.overall_timeout_ms ({overall_ms}) must be shorter than server.request_timeout_ms ({request_ms})"
    )]
    Budget {
        /// The overall fan-out budget.
        overall_ms: u64,
        /// The request timeout.
        request_ms: u64,
    },
}
