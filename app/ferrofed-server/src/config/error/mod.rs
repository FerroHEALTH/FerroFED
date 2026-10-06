// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of a configuration the server does not start on.

use std::fmt;
use std::path::{Path, PathBuf};

use ferrofed_registry::error::IdError;
use openehr_its::rest::client::InvalidCredentials;

use crate::config::ENV_PREFIX;
use crate::config::error::parse::{ParseFault, Stage};
use crate::config::stored_queries::Backend;
use crate::config::transport::{CleartextError, TrustAnchorError};

pub mod parse;

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
    ///
    /// It carries a [`ParseFault`] in place of the TOML reader's error: that
    /// error quotes the offending source line, and a line of `[dev]` holds a
    /// patient identifier, a line of `[credentials]` a secret (§5.4.1, N33).
    #[error("{fault}")]
    Parse {
        /// Where the fault is and what kind it is, with no source text.
        fault: ParseFault,
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
    /// Stored-query definitions would be distributed with no stored-query
    /// registry to distribute from (§12.7, N44).
    #[error(
        "federation.fan_out_stored_queries distributes the stored-query registry's definitions, \
         and no registry is offered: configure [stored_queries], or turn the setting off (§12.7)"
    )]
    StoredQueryFanOutWithoutRegistry,
    /// Stored-query definitions would be distributed from a read-only
    /// registry, which stores none to distribute (§12.7, N44).
    #[error(
        "federation.fan_out_stored_queries distributes the definitions a PUT stores, \
         and stored_queries.backend = \"files\" refuses every PUT: turn the setting off, \
         or choose a backend that stores (§12.7)"
    )]
    StoredQueryFanOutReadOnly,
    /// A `[stored_queries]` key the chosen backend does not read is set.
    #[error("{key} does not apply to stored_queries.backend = \"{backend}\"; remove it")]
    StoreKey {
        /// The key that is set.
        key: String,
        /// The backend chosen.
        backend: Backend,
    },
    /// The chosen stored-query backend is not built into this binary.
    #[error(
        "stored_queries.backend = \"{backend}\" is not built into this binary; build ferrofed-server with its {backend} feature"
    )]
    StoreBackendUnavailable {
        /// The backend chosen.
        backend: Backend,
    },
    /// The PostgreSQL connection string does not parse.
    ///
    /// The parser's message is not kept: it may quote a character of the
    /// string, which holds a password.
    #[error("{key} is not a PostgreSQL connection string, a URL or libpq key/value pairs")]
    StoreUrl {
        /// The key the string was read from: the inline key or its `_file`
        /// sibling.
        key: String,
    },
    /// A key a section needs is not set.
    #[error("{key} is not set, and its section needs it")]
    Missing {
        /// The key that carries no value.
        key: String,
    },
    /// `[auth]` is refused: the key and why, never a value it holds.
    #[error("{key} {fault}")]
    Auth {
        /// The key that carries the fault.
        key: String,
        /// Why it is refused.
        fault: crate::config::auth::AuthFault,
    },
    /// A URL does not parse.
    #[error("{key} is not a URL")]
    Url {
        /// The key that holds it.
        key: String,
        /// What the URL parser reported.
        #[source]
        source: url::ParseError,
    },
    /// A URL carries a user name or a password in its userinfo, where the
    /// credentials belong in their own section.
    #[error("{key} carries a user name or password; set them in {section} instead")]
    UrlCredentials {
        /// The key that holds the URL.
        key: String,
        /// The section the credentials belong in.
        section: String,
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
    /// The base path is not one a request can be served under (§4.1, N28).
    #[error("{key} is not a base path")]
    BasePath {
        /// The key that holds it.
        key: String,
        /// Why the path is refused.
        #[source]
        source: crate::base_path::BasePathError,
    },
    /// A duration or a size that must be positive is zero.
    #[error("{key} is zero; it must be positive")]
    Zero {
        /// The key that holds it.
        key: String,
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
    /// A credentials section is keyed by something that is not an endpoint id.
    #[error("credentials.{key:?} is not an endpoint id")]
    EndpointId {
        /// The key that was given.
        key: String,
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// An `[auth.issuer.patient]` binding names something that is not an
    /// endpoint id.
    #[error("{key} is not an endpoint id")]
    PatientEndpoint {
        /// The key that carries it.
        key: String,
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// `federation.demographic_endpoint` is not an endpoint id.
    #[error("federation.demographic_endpoint is not an endpoint id")]
    DemographicEndpoint {
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// A credentials section names more than one scheme: a bearer token, a
    /// basic user, an OAuth 2.0 grant.
    #[error("{section} names more than one credentials scheme; set one")]
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
    /// A credential does not form the `Authorization` value the node client
    /// sends, as a bearer token that is not the `b64token` of RFC 6750 §2.1
    /// does not.
    ///
    /// The source is the node client's refusal, which quotes nothing.
    #[error("{key} cannot be sent in the Authorization header")]
    Authorization {
        /// The key the credential was read from: the inline key or its
        /// `_file` sibling.
        key: String,
        /// What the node client reported.
        #[source]
        source: InvalidCredentials,
    },
    /// A basic user or password holds a character RFC 7617 §2 forbids.
    #[error("{key} cannot be sent: it holds {fault}, which RFC 7617 §2 forbids")]
    Basic {
        /// The key the value was read from: the inline key or its `_file`
        /// sibling.
        key: String,
        /// The kind of character found, never the character's position.
        fault: BasicFault,
        /// What the node client reported, which quotes nothing.
        #[source]
        source: InvalidCredentials,
    },
    /// A signing key cannot be used: it is no P-256 or P-384 private key in
    /// PKCS#8 PEM, the previous key is the current one, or the next key is
    /// the current or the previous one.
    #[error("{key} is not a usable signing key")]
    SigningKey {
        /// The key the file was named by.
        key: String,
        /// Why the key is refused; it quotes no part of the key.
        #[source]
        source: ferrofed_engine::onward::keys::KeyError,
    },
    /// The next signing key is on another curve than the algorithm it is
    /// meant to sign with: `signing.next_key_algorithm`, or the current key's
    /// when that is unset (RFC 7518 §3.4).
    #[error(
        "{key} holds a key that signs {found}, and the next key is meant to sign {intended}; set signing.next_key_algorithm when the rotation changes the algorithm"
    )]
    NextKeyAlgorithm {
        /// The key the file was named by.
        key: String,
        /// The algorithm the key's curve signs with.
        found: &'static str,
        /// The algorithm the next key is meant to sign with.
        intended: &'static str,
    },
    /// A `DPoP` key cannot be used: it is no P-256 or P-384 private key in
    /// PKCS#8 PEM (RFC 9449).
    #[error("{key} is not a usable DPoP key")]
    DpopKey {
        /// The key the file was named by.
        key: String,
        /// Why the key is refused; it quotes no part of the key.
        #[source]
        source: ferrofed_engine::onward::dpop::DpopKeyError,
    },
    /// A Nuts grant cannot be built: its authorization server is no issuer
    /// URL without userinfo, query or fragment, its scope holds a character
    /// RFC 6749 §3.3 does not admit, or its `client_id` is empty (RFC 8414
    /// §2).
    #[error("{key} is not usable in a Nuts grant")]
    #[cfg(feature = "binding-nl")]
    NutsGrant {
        /// The key of the refused value.
        key: String,
        /// Why it is refused.
        #[source]
        source: nl_generic_functions::nuts_auth::error::InvalidInput,
    },
    /// The holder of a Nuts grant cannot present: its DID is no `did:web`
    /// DID, its key is not one of the DID's or no P-256 or P-384 key, or a
    /// credential is no JWT credential issued to it (Nuts RFC021 §4.2).
    #[error("{section} names a holder that cannot present")]
    #[cfg(feature = "binding-nl")]
    NutsHolder {
        /// The section of the holder.
        section: String,
        /// Why it is refused; it quotes no key and no credential.
        #[source]
        source: nl_generic_functions::nuts_auth::holder::HolderError,
    },
    /// `signing.assertion_lifetime_s` is zero or longer than the five minutes
    /// a client assertion may live.
    #[error(
        "signing.assertion_lifetime_s is {seconds}; a client assertion lives 1 to {max} seconds"
    )]
    AssertionLifetime {
        /// The lifetime given.
        seconds: u64,
        /// The longest lifetime allowed.
        max: u64,
    },
    /// The rotation overlap is shorter than an assertion's lifetime plus the
    /// time a node caches the JWK Set, so a node could meet an assertion signed
    /// by a key the set no longer publishes.
    #[error(
        "signing.rotation_overlap_s ({overlap_s}) must be at least signing.assertion_lifetime_s ({lifetime_s}) plus signing.node_jwks_cache_s ({cache_s})"
    )]
    RotationOverlap {
        /// The overlap window.
        overlap_s: u64,
        /// The assertion lifetime.
        lifetime_s: u64,
        /// How long a node caches the JWK Set.
        cache_s: u64,
    },
    /// A URL that must be absolute `http` or `https` is not.
    #[error("{key} must be an http or https URL with no user name or password")]
    HttpUrl {
        /// The key that holds it.
        key: String,
    },
    /// An OAuth 2.0 grant cannot be built from its section.
    #[error("{section} is not a usable OAuth 2.0 client-credentials grant")]
    Grant {
        /// The section.
        section: String,
        /// What the grant refused.
        #[source]
        source: ferrofed_engine::onward::GrantError,
    },
    /// A scope is not one the gateway may request onward.
    #[error("{key} is not a SMART on openEHR system scope")]
    Scope {
        /// The key that holds it.
        key: String,
        /// What the scope grammar refused.
        #[source]
        source: ferrofed_engine::onward::ScopeError,
    },
    /// An OAuth 2.0 grant is configured, but no signing key signs its client
    /// assertion.
    #[error("{section} needs [signing], whose key signs its client assertion (RFC 7523 §2.2)")]
    GrantWithoutSigning {
        /// The section.
        section: String,
    },
    /// An identity service's scope is not the space-delimited scopes of RFC
    /// 6749 §3.3.
    #[error("{key} is not a space-delimited list of RFC 6749 §3.3 scope-tokens")]
    ServiceScope {
        /// The key that holds it.
        key: String,
        /// What the scope grammar refused.
        #[source]
        source: ferrofed_engine::onward::ScopeError,
    },
    /// An identity service's credentials section names a Nuts grant or a
    /// FAPI 2.0 grant.
    #[error(
        "{section} takes a bearer token, basic credentials or an oauth2 client-credentials grant, not a nuts or fapi2 grant"
    )]
    ServiceGrantNotHere {
        /// The section.
        section: String,
    },
    /// A credentials section that takes a bearer token or basic credentials
    /// names an OAuth 2.0 grant, a Nuts grant or a FAPI 2.0 grant.
    #[error(
        "{section} takes a bearer token or basic credentials, not an oauth2, nuts or fapi2 grant"
    )]
    GrantNotHere {
        /// The section.
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
    /// `metrics.listen` names an address other than a loopback one, and
    /// `metrics.allow_remote` does not allow it.
    #[error(
        "metrics.listen is {address}, which is not a loopback address; the metrics listener has no authentication, so set metrics.allow_remote = true to serve it beyond this host"
    )]
    MetricsRemote {
        /// The address `metrics.listen` names.
        address: std::net::SocketAddr,
    },
    /// `metrics.listen` names the address `server.listen` binds.
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
    /// Audit messages are turned off outside a configuration marked for
    /// development.
    #[error(
        "{key} = \"off\" records no ITI-55 audit message, which only profile = \"development\" accepts; set it to \"log\" (ITI TF-2 §3.55.5.1)"
    )]
    #[cfg(feature = "binding-ihe")]
    AuditOff {
        /// The key that turned the audit off.
        key: String,
    },
    /// `[audit] destination = "off"` outside development.
    #[error(
        "{key} = \"off\" records no PIXm, mCSD or PMIR audit record, which only profile = \"development\" accepts; set it to \"log\" or \"repository\" (PIXm §2:3.83.5.1, mCSD §2:3.90.5.1, PMIR §2:3.93.5.1)"
    )]
    #[cfg(feature = "binding-ihe")]
    FeedAuditOff {
        /// The key that turned the audit off.
        key: String,
    },
    /// `[audit] destination = "log"` outside development while a registry
    /// is configured, whose accesses the log target cannot record.
    #[error(
        "{key} = \"log\" records no caller and no patient, so the access log of a gateway with a registry needs \"repository\" outside profile = \"development\" (Regulation (EU) 2025/327 Annex II 3.2)"
    )]
    #[cfg(feature = "binding-ihe")]
    AccessAuditLog {
        /// The key that sent the access log to the log target.
        key: String,
    },
    /// `[audit.repository]` is set while the records go elsewhere.
    #[error(
        "[audit.repository] applies only under audit.destination = \"repository\"; remove it, or send the audit records there"
    )]
    #[cfg(feature = "binding-ihe")]
    FeedAuditRepositoryUnused,
    /// `[xcpd.audit_repository]` is set while the audit messages go
    /// elsewhere.
    #[error(
        "[xcpd.audit_repository] applies only under xcpd.audit = \"repository\"; remove it, or send the audit messages there"
    )]
    #[cfg(feature = "binding-ihe")]
    AuditRepositoryUnused,
    /// `nl_gf.nvi.namespaces` lists a BSN system as standing for the
    /// pseudonymised BSN, which would send a BSN to the NVI.
    #[error(
        "nl_gf.nvi.namespaces lists {namespace}, a BSN system: the NVI is keyed on the pseudonymised BSN, and the gateway never sends it a BSN"
    )]
    #[cfg(feature = "binding-nl")]
    BsnAsPseudonym {
        /// The namespace as written, a naming system and never a value.
        namespace: String,
    },
    /// `nl_gf.nvi.credentials` names an OAuth 2.0 or FAPI 2.0 grant, which
    /// the Generic Functions IG defines for no data user of the Localization
    /// Service.
    #[error(
        "{key} is not a credential the NVI takes: nl_gf.nvi.credentials takes a bearer token, basic credentials or the nuts grant of GF-Authentication"
    )]
    #[cfg(feature = "binding-nl")]
    NviGrant {
        /// The key of the refused table, such as
        /// `nl_gf.nvi.credentials.oauth2`.
        key: String,
    },
    /// A `[nl_gf.mitz]` value the closed authorization question does not
    /// take: the key and why, never a patient value.
    #[error("{key} {fault}")]
    #[cfg(feature = "binding-nl")]
    Mitz {
        /// The key.
        key: String,
        /// Why it is refused.
        fault: &'static str,
    },
    /// A `[pdqm]` value the demographics step does not take: the key and
    /// why, never a patient value.
    #[error("{key} {fault}")]
    #[cfg(feature = "binding-ihe")]
    Pdqm {
        /// The key.
        key: String,
        /// Why it is refused.
        fault: &'static str,
    },
    /// A syslog header value is empty, too long, or holds a character
    /// syslog cannot carry (RFC 5424 §6).
    #[error("{key} cannot be a syslog header field")]
    #[cfg(feature = "binding-ihe")]
    SyslogHeader {
        /// The key that holds it.
        key: String,
        /// What the syslog writer reported.
        #[source]
        source: ihe_iti::atna::syslog::HeaderError,
    },
    /// A URL that carries a patient identifier or a credential is not
    /// `https`, outside a configuration marked for development.
    #[error(transparent)]
    Cleartext(#[from] CleartextError),
    /// A FAPI 2.0 grant, or an `oauth2` grant's assertion audience, is
    /// refused.
    #[error(transparent)]
    GrantFault(#[from] crate::config::grant::GrantFault),
    /// TLS material, or a use of mutual TLS, is refused (RFC 8705).
    #[error(transparent)]
    TlsFault(#[from] crate::config::tls::TlsFault),
    /// The certificate, the key or the client CA of a listener's TLS cannot
    /// be served with.
    #[error(transparent)]
    ListenerTls(#[from] crate::listener::certificates::CertificateError),
    /// A healthcheck identity is named where `ferrofed healthcheck` never
    /// presents it.
    #[error(
        "{key} is read only by ferrofed healthcheck, which presents it to a [server.tls] listener that sets client_ca_file; remove it here"
    )]
    HealthcheckIdentityUnused {
        /// The key that names it.
        key: String,
    },
    /// A URL the gateway verifies its callers against is plain `http` to a
    /// host that is not loopback.
    #[error(transparent)]
    TrustAnchor(#[from] TrustAnchorError),
    /// Both a registry document and a directory are configured, and the
    /// registry has one source.
    #[error("set registry.document or [registry.mcsd], not both: the registry has one source")]
    #[cfg(feature = "binding-ihe")]
    TwoRegistrySources,
    /// The path the ITI-93 feed is served at is not a path of its own: it
    /// must start with `/`, carry no query or fragment, and stay off the
    /// ITS-REST surface, the health family and the well-known documents.
    #[error(
        "{key} must be a path that starts with /, carries no query or fragment, and is not under /v1, /health or /.well-known"
    )]
    #[cfg(feature = "binding-ihe")]
    FeedPath {
        /// The key that carries the path.
        key: String,
    },
    /// The localizer's budget does not end before the overall budget, so the
    /// localizer could leave no time to resolve and ask the members (§11.5,
    /// §14.1).
    #[error(
        "federation.localization.timeout_ms ({timeout_ms}) must be below federation.overall_timeout_ms ({overall_ms}), of which it is a part (§11.5)"
    )]
    LocalizationBudget {
        /// The localizer's budget.
        timeout_ms: u64,
        /// The overall fan-out budget.
        overall_ms: u64,
    },
    /// The demographics step's budget and the localizer's do not end before
    /// the overall budget, of which each is a part, so the two could leave
    /// no time to resolve and ask the members (§11.5; no specification
    /// governs the step's budget: our own design).
    #[error(
        "pdqm.timeout_ms ({timeout_ms}) plus federation.localization.timeout_ms ({localization_ms}) must be below federation.overall_timeout_ms ({overall_ms}), of which both are a part (§11.5)"
    )]
    #[cfg(feature = "binding-ihe")]
    DemographicsBudget {
        /// The demographics step's budget.
        timeout_ms: u64,
        /// The localizer's budget, zero with no localizer.
        localization_ms: u64,
        /// The overall fan-out budget.
        overall_ms: u64,
    },
    /// The consent pre-filter's budget, the demographics step's and the
    /// localizer's do not end before the overall budget, of which each is a
    /// part, so the three could leave no time to resolve and ask the members
    /// (§11.5; no specification governs the pre-filter's budget: our own
    /// design).
    #[error(
        "nl_gf.mitz.timeout_ms ({timeout_ms}) plus pdqm.timeout_ms ({demographics_ms}) plus federation.localization.timeout_ms ({localization_ms}) must be below federation.overall_timeout_ms ({overall_ms}), of which all three are a part (§11.5)"
    )]
    #[cfg(feature = "binding-nl")]
    PrefilterBudget {
        /// The consent pre-filter's budget.
        timeout_ms: u64,
        /// The demographics step's budget, zero with no `[pdqm]`.
        demographics_ms: u64,
        /// The localizer's budget, zero with no localizer.
        localization_ms: u64,
        /// The overall fan-out budget.
        overall_ms: u64,
    },
    /// `[access_log]` declares a category map the logging component refuses.
    #[error("[access_log] declares a category map that cannot be used")]
    AccessLogMap(#[source] ehds_logging::map::MapError),
}

impl Error {
    /// Builds an [`Error::Parse`] from what the TOML reader reported about
    /// `text`, keeping positions and key names and dropping the quoted source.
    pub(crate) fn parse(error: &toml::de::Error, text: &str, stage: Stage) -> Self {
        Self::Parse {
            fault: ParseFault::from_toml(error, text, stage),
        }
    }

    /// Names `path` as the file a parse fault sits in; any other error is
    /// returned unchanged.
    #[must_use]
    pub(crate) fn in_file(self, path: &Path) -> Self {
        match self {
            Self::Parse { mut fault } => {
                fault.file = Some(path.to_path_buf());
                Self::Parse { fault }
            }
            other => other,
        }
    }
}

/// What RFC 7617 §2 forbids in a basic user-id or password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BasicFault {
    /// A control character (`CTL` of RFC 5234 Appendix B.1), forbidden in
    /// both the user-id and the password.
    ControlCharacter,
    /// A colon, forbidden in the user-id.
    Colon,
}

impl fmt::Display for BasicFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ControlCharacter => f.write_str("a control character"),
            Self::Colon => f.write_str("a colon"),
        }
    }
}
