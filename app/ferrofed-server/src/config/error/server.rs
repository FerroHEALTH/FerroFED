// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of the `[server]`, `[metrics]` and `[telemetry]` sections.

/// A `[server]`, `[metrics]` or `[telemetry]` value the server refuses to
/// start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServerFault {
    /// `server.replica` is no replica name.
    #[error("server.replica is no replica name")]
    Replica(#[source] ferrofed_registry::replica::ReplicaError),
    /// The base path is not one a request can be served under (§4.1, N28).
    #[error("{key} is not a base path")]
    BasePath {
        /// The key that holds it.
        key: String,
        /// Why the path is refused.
        #[source]
        source: crate::base_path::BasePathError,
    },
    /// `telemetry.trace_sample_ratio` is not a number from `0.0` to `1.0`.
    #[error("telemetry.trace_sample_ratio is {value}; it must be a number from 0.0 to 1.0")]
    SampleRatio {
        /// The value the key holds.
        value: f64,
    },
    /// The log filter does not parse.
    #[error("telemetry.filter is not a valid tracing filter")]
    Filter {
        /// What the filter parser reported.
        #[source]
        source: tracing_subscriber::filter::ParseError,
    },
    /// An entry of `server.trusted_proxies` is neither an IP address nor a
    /// CIDR block.
    #[error("{key} is neither an IP address nor a CIDR block such as 10.0.0.0/8")]
    TrustedProxy {
        /// The entry's key.
        key: String,
        /// Why it does not parse as an address.
        #[source]
        source: std::net::AddrParseError,
    },
    /// `server.public_url` is no `http` or `https` URL, or carries a user
    /// name, a password, a query or a fragment.
    #[error(
        "server.public_url must be an http or https URL with no user name, password, query or fragment"
    )]
    PublicUrlForm,
    /// The path of `server.public_url` is not `server.base_path`.
    #[error(
        "server.public_url names the path {path} and server.base_path is {base}; the public URL ends in the base path"
    )]
    PublicUrlPath {
        /// The path the public URL names, with no trailing `/`.
        path: String,
        /// The base path.
        base: String,
    },
    /// A key that repeats the public base URL names another route than the
    /// one the gateway serves under it.
    #[error(
        "{key} is not the route the gateway serves under server.public_url, {served}; remove {key} to take that value, or correct it"
    )]
    PublicUrlDisagrees {
        /// The key that repeats the public URL.
        key: String,
        /// The URL it should name.
        served: String,
    },
    /// `metrics.listen` names an address other than a loopback one, and
    /// `metrics.allow_remote` does not allow it.
    #[error(
        "metrics.listen is {address}, which is not a loopback address; set metrics.allow_remote = true to serve the admin listener beyond this host"
    )]
    MetricsRemote {
        /// The address `metrics.listen` names.
        address: std::net::SocketAddr,
    },
    /// `metrics.listen` names an address other than a loopback one outside
    /// the development profile, and neither a scrape token nor a client CA
    /// authenticates the scrape there.
    #[error(
        "metrics.listen is {address}, which is not a loopback address, and nothing authenticates GET /metrics there: set metrics.scrape_token_file, or metrics.tls.client_ca_file for mutual TLS, or keep the listener on loopback"
    )]
    MetricsUnauthenticated {
        /// The address `metrics.listen` names.
        address: std::net::SocketAddr,
    },
    /// `metrics.listen` names the address `server.listen` binds, at a port
    /// other than 0.
    #[error("metrics.listen is {address}, the address server.listen binds; give it its own")]
    MetricsShared {
        /// The address both keys name.
        address: std::net::SocketAddr,
    },
    /// `metrics.otlp_endpoint` or `telemetry.otlp_endpoint` is not an
    /// `http://` URL.
    #[error(
        "{key} must be an http:// URL: the OTLP exporters speak gRPC without TLS, to a collector beside the gateway"
    )]
    OtlpScheme {
        /// The key of the collector URL.
        key: String,
    },
    /// The request timeout does not exceed the overall fan-out budget plus
    /// the combining margin, so it could cut the answer and the `504`
    /// envelope the budget produces when it expires (§11.4, §11.5).
    #[error(
        "server.request_timeout_ms ({request_ms}) must exceed federation.overall_timeout_ms ({overall_ms}) plus {margin_ms} ms for combining the answers (§11.5)"
    )]
    Budget {
        /// The overall fan-out budget.
        overall_ms: u64,
        /// The combining margin the request timeout must leave past it.
        margin_ms: u64,
        /// The request timeout.
        request_ms: u64,
    },
    /// The drain is shorter than the request timeout, so a stop could cut a
    /// request the server accepted before its listener closed (no
    /// specification governs this: our own design).
    #[error(
        "server.shutdown_timeout_ms ({shutdown_ms}) must be at least server.request_timeout_ms ({request_ms}), so the drain outlasts every request accepted before the listener closed; leave it unset to take the request timeout"
    )]
    Drain {
        /// The drain.
        shutdown_ms: u64,
        /// The request timeout.
        request_ms: u64,
    },
    /// A healthcheck identity is named where `ferrofed healthcheck` never
    /// presents it.
    #[error(
        "{key} is read only by ferrofed healthcheck, which presents it to a [server.tls] listener that sets client_ca_file; remove it here"
    )]
    HealthcheckIdentityUnused {
        /// The key that names it.
        key: String,
    },
}
