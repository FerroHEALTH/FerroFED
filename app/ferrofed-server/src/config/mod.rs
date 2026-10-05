// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: an optional TOML file, then environment overrides, then one
//! typed [`Settings`](settings::Settings) the run path holds.
//!
//! Every struct refuses an unknown key, every default lives inline in its own
//! `Default` impl, every secret is reachable through a `<key>_file` sibling
//! read at boot, and a bad value refuses to boot rather than falling back. No
//! specification governs the configuration: our own design.

use ferrofed_identity::dev::Profile;
use ferrofed_identity::localizer::OnFailure;
use ferrofed_registry::secret::{Secret, SecretUrl};
use openehr_federation::aggregate::AggregateFunction;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::binding::development::DevSection;
use crate::telemetry::{DEFAULT_FILTER, Format};

pub mod auth;
pub mod error;
pub mod grant;
mod load;
pub(crate) mod resolve;
pub(crate) mod secrets;
pub mod settings;
pub mod stored_queries;
pub mod tls;
pub mod transport;

/// The prefix of every environment override.
///
/// The name after it is the dotted key with `__` between segments, upper or
/// lower case: `FERROFED__SERVER__LISTEN` sets `[server] listen`.
pub const ENV_PREFIX: &str = "FERROFED__";

/// The environment variable naming the configuration file.
pub const CONFIG_PATH_ENV: &str = "FERROFED_CONFIG";

/// The milliseconds kept for combining the answers after the fan-out budget.
///
/// `server.request_timeout_ms` must exceed `federation.overall_timeout_ms` by
/// more than this. A client "can rely on" an answer "within its declared overall budget, plus
/// combining time" (§11.5), so the request timeout never cuts the `504`
/// envelope the budget produces when it expires.
// NOTE: no specification governs this: our own design; §11.5 names combining
// time without bounding it, and one second covers the merge and the encoding.
pub const COMBINING_MARGIN_MS: u64 = 1_000;

/// The whole configuration tree, as a file and the environment state it.
///
/// The sections of a binding exist only in a build with its feature
/// (`binding-ihe`, `binding-nl`), so a build without it refuses them as
/// unknown keys; each binding resolves its own
/// ([`Binding::resolve`](crate::binding::Binding::resolve)).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The deployment profile: `production`, or `development`, the only
    /// profile that admits the static cross-reference of `[dev]`.
    pub profile: Profile,
    /// The HTTP surface.
    pub server: Server,
    /// The console.
    pub telemetry: Telemetry,
    /// The federation's membership.
    pub registry: Registry,
    /// The federated query: the budgets and the default issuing namespace.
    pub federation: Federation,
    /// The outbound credentials, one section per endpoint id
    /// (`[credentials."<endpoint id>"]`).
    ///
    /// The registry names the endpoints, and the node dispatch hands each one
    /// its credentials.
    pub credentials: BTreeMap<String, Credentials>,
    /// The static development cross-reference (`[[dev.crossref]]`), accepted
    /// only under `profile = "development"`.
    pub dev: Option<DevSection>,
    /// The PIXm resolver (`[pixm]`): the PIX Managers and each member's
    /// `ehr_id` domain there (#43).
    #[cfg(feature = "binding-ihe")]
    pub pixm: Option<crate::binding::ihe::pixm::Pixm>,
    /// The XCPD localizer (`[xcpd]`): the responding gateways and the
    /// community each member serves (Annex A.3, #85).
    #[cfg(feature = "binding-ihe")]
    pub xcpd: Option<crate::binding::ihe::xcpd::Xcpd>,
    /// The Dutch Generic Functions (`[nl_gf]`): the NVI localizer and the
    /// care provider each member holds the data of (Annex B, #87).
    #[cfg(feature = "binding-nl")]
    pub nl_gf: Option<crate::binding::nl::NlGf>,
    /// The PMIR identity feed (`[pmir]`): the Patient Identity Registry the
    /// gateway subscribes to with ITI-94, and the path its ITI-93 messages
    /// arrive at (track 8 of §16.3, Annex A.4).
    #[cfg(feature = "binding-ihe")]
    pub pmir: Option<crate::binding::ihe::pmir::config::Pmir>,
    /// The PDQm demographics step (`[pdqm]`): the Patient Demographics
    /// Supplier asked for the master identity of an identifier the
    /// cross-reference does not map (Annex A §A.2, #487).
    #[cfg(feature = "binding-ihe")]
    pub pdqm: Option<crate::binding::ihe::pdqm::Pdqm>,
    /// The federated stored-query registry (`[stored_queries]`, §12.7).
    pub stored_queries: stored_queries::StoredQueries,
    /// The metrics surface (`[metrics]`): the admin listener and the OTLP
    /// push, both off by default.
    pub metrics: Metrics,
    /// The gateway's signing keys (`[signing]`), which sign the client
    /// assertion of every OAuth 2.0 grant to a node and which the gateway
    /// publishes as a JWK Set (§13.1, N25).
    pub signing: Option<Signing>,
    /// How a caller authenticates to the gateway (`[auth]`, §13.1, N25).
    pub auth: auth::Auth,
    /// Where the audit records of the PIXm, PDQm, mCSD and PMIR transactions go
    /// (`[audit]`, #486).
    #[cfg(feature = "binding-ihe")]
    pub audit: crate::binding::ihe::audit::config::Audit,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Production,
            server: Server::default(),
            telemetry: Telemetry::default(),
            registry: Registry::default(),
            federation: Federation::default(),
            credentials: BTreeMap::new(),
            dev: None,
            #[cfg(feature = "binding-ihe")]
            pixm: None,
            #[cfg(feature = "binding-ihe")]
            xcpd: None,
            #[cfg(feature = "binding-nl")]
            nl_gf: None,
            #[cfg(feature = "binding-ihe")]
            pmir: None,
            #[cfg(feature = "binding-ihe")]
            pdqm: None,
            stored_queries: stored_queries::StoredQueries::default(),
            metrics: Metrics::default(),
            signing: None,
            auth: auth::Auth::default(),
            #[cfg(feature = "binding-ihe")]
            audit: crate::binding::ihe::audit::config::Audit::default(),
        }
    }
}

/// The federation's membership.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Registry {
    /// The reviewed registry document naming the organisations, nodes and
    /// endpoints (§12b.1). Without it the gateway
    /// federates nothing, and the ITS-REST surface stays unserved.
    pub document: Option<PathBuf>,
    /// The form the document is written in.
    pub format: RegistryFormat,
    /// The mCSD care services directory the registry is read from and kept
    /// in step with (`[registry.mcsd]`, §15.1, Annex A.5), in place of a
    /// document.
    #[cfg(feature = "binding-ihe")]
    pub mcsd: Option<crate::binding::ihe::mcsd::McsdDirectory>,
}

impl Registry {
    /// Whether a registry is configured, as a document or as a directory.
    #[must_use]
    pub fn configured(&self) -> bool {
        #[cfg(feature = "binding-ihe")]
        if self.mcsd.is_some() {
            return true;
        }
        self.document.is_some()
    }
}

/// The form of the registry document.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RegistryFormat {
    /// The native TOML form: `[[organisation]]`, `[[node]]`, `[[endpoint]]`
    /// and `[[creating_system]]` tables.
    #[default]
    Toml,
    /// A FHIR R4 JSON `Bundle` of `Organization` and `Endpoint` resources, the
    /// form N19 recommends, read with the IHE binding's mCSD reader.
    #[cfg(feature = "binding-ihe")]
    Fhir,
}

/// The federated query.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Federation {
    /// The federation's own identifier, `federation.id` of the
    /// `OPTIONS {base}/` body (§7a.2, N30). It has no default: a federating
    /// gateway names its federation.
    pub id: Option<String>,
    /// How long one node's request may take (§11.5, N38).
    pub per_node_timeout_ms: u64,
    /// How long the whole fan-out may take (§11.5, N38). With a registry
    /// configured, `server.request_timeout_ms` must exceed it by more than
    /// [`COMBINING_MARGIN_MS`], so the gateway answers with its envelope
    /// before the request timeout cuts the connection.
    pub overall_timeout_ms: u64,
    /// The issuing namespace an unqualified patient identifier resolves in
    /// (§5.2 requires the namespace; no specification governs the default: our
    /// own design). Without it, a query that names no namespace is a `400`.
    pub default_namespace: Option<String>,
    /// How long the resolution bindings of a client session live (§12.5.1
    /// step 2): a correctness bound, past which a binding is
    /// never routed on.
    pub binding_ttl_ms: u64,
    /// How many `ehr_id` bindings the resolution bindings of every caller
    /// hold together (§12.5.1 step 2); zero is refused.
    pub binding_capacity: u32,
    /// How many `ehr_id`s the `ehr_id` to node index holds (§12.5.1 step 3)
    /// before it forgets the least recently used; zero is refused.
    pub ehr_index_capacity: u32,
    /// How the node set of an undirected patient query is chosen (§4.3, N4).
    /// It has no default: a federating gateway declares it.
    pub node_selection: Option<NodeSelection>,
    /// The localizer's failure policy and budget
    /// (`[federation.localization]`, §14.1), set only under
    /// `node_selection = "localized"`.
    pub localization: Option<Localization>,
    /// Whether the gateway offers best-effort completion (§11.4, N37). A
    /// request opts into it with `openEHR-federation-completeness: partial`;
    /// all-or-nothing stays the default, and a gateway that does not offer
    /// best-effort refuses `partial` with a `400`.
    pub best_effort: bool,
    /// How `LIMIT n OFFSET k` with `k > 0` is answered across a fan-out
    /// (§11.6.2, N39): `bounded`, the default, or `reject`.
    pub offset_strategy: OffsetPaging,
    /// The most rows the `bounded` strategy asks of one node for a page,
    /// `k + n` (§11.6.2: "permitted only where the gateway can bound
    /// `k + n`"). A page past it is a `400` naming the bound; zero is refused.
    pub max_offset_window: u32,
    /// The aggregate functions an undirected query may apply, recombined at
    /// the Tier (§11.6.3, N39): by default every one, `COUNT`, `SUM`, `MIN`,
    /// `MAX` and `AVG`. An empty list refuses every undirected aggregate with
    /// a `400`.
    pub decomposable_aggregates: Vec<DecomposableAggregate>,
    /// The one member endpoint that may serve a request under
    /// `{base}/v1/demographic/`, which the request names in its targeting
    /// header (§7a.1, §12.4, §12.6, N23, N32). Without it the DEMOGRAPHIC area
    /// answers `501`; it is never federated either way. A value that names
    /// no endpoint of the registry refuses to boot.
    pub demographic_endpoint: Option<String>,
    /// Whether a template upload may fan out to several members (§12.6,
    /// N43): off by default. With it on, an ADL 1.4 or ADL 2 template upload
    /// whose `openEHR-federation-endpoint` header is `*`, or whose targeting
    /// headers select more than one endpoint, is sent to each of them
    /// independently and answered per node; `OPTIONS {base}/` declares it as
    /// `definition.fan_out_template_upload` (§7a.2, N30).
    pub fan_out_template_upload: bool,
    /// Whether a stored-query definition may be distributed to members
    /// (§12.7, N44): off by default, and only beside the stored-query
    /// registry. With it on, a `PUT` to the registry whose targeting headers
    /// name `*` or members is also sent to each of them, reported per node,
    /// and a `GET` naming them reports per node whether its copy matches;
    /// `OPTIONS {base}/` declares it as `definition.stored_query_fan_out`
    /// (§7a.2, N30).
    pub fan_out_stored_queries: bool,
    /// How a member the Step-1 consent pre-filter excludes is reported
    /// (`[federation.consent]`, N27a).
    pub consent: ConsentReporting,
}

/// How a member the Step-1 consent pre-filter excludes is reported,
/// `[federation.consent]` (N27a, §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConsentReporting {
    /// Whether the answer names the exclusion: `true`, the default, reports
    /// the member `consent-denied` (N27a); `false` reports it as a member
    /// that does not know the patient, for a deployment under Regulation
    /// (EU) 2025/327 Art 8, and `OPTIONS {base}/` declares it as
    /// `consent.disclose` (§7a.2).
    pub disclose: bool,
}

impl Default for ConsentReporting {
    fn default() -> Self {
        Self { disclose: true }
    }
}

/// An aggregate function the gateway recombines across a fan-out
/// (§11.6.3), by the name `OPTIONS {base}/` declares it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum DecomposableAggregate {
    /// `COUNT`, without `DISTINCT`: the sum of the node counts.
    Count,
    /// `SUM`: the sum of the node sums.
    Sum,
    /// `MIN`, over numbers and complete date-times.
    Min,
    /// `MAX`, over numbers and complete date-times.
    Max,
    /// `AVG`, asked of each node as its `SUM` and `COUNT`.
    Avg,
}

impl From<DecomposableAggregate> for AggregateFunction {
    fn from(function: DecomposableAggregate) -> Self {
        match function {
            DecomposableAggregate::Count => Self::Count,
            DecomposableAggregate::Sum => Self::Sum,
            DecomposableAggregate::Min => Self::Min,
            DecomposableAggregate::Max => Self::Max,
            DecomposableAggregate::Avg => Self::Avg,
        }
    }
}

/// How `LIMIT n OFFSET k` with `k > 0` is answered across a fan-out
/// (§11.6.2, N39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum OffsetPaging {
    /// Every `OFFSET k > 0` is refused with a `400`.
    Reject,
    /// The page is computed from `k + n` rows per node, merged, ordered and
    /// sliced, within `max_offset_window`.
    Bounded,
}

/// How the node set of an undirected patient query is chosen (§4.3, N4,
/// N10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum NodeSelection {
    /// No localizer is configured: every active member is a candidate, its
    /// cross-reference decides, and a member that does not know the patient is
    /// `not-resolved` (§4.3 Variant B, N4 last sentence, N6).
    AskAll,
    /// A localizer derives the node set from the patient: a member it does
    /// not name is `not-localized` and never asked, and an undirected query
    /// that names no patient is refused, since no node set is defined (N4,
    /// N10, §14.1).
    Localized,
}

/// The localizer's failure policy and budget, `[federation.localization]`
/// (§14.1).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Localization {
    /// What the gateway does when the localizer does not answer: `closed`,
    /// the default, dispatches to no member and reports every member
    /// `not-localized` with the error; `ask-all` asks every member instead.
    /// `OPTIONS {base}/` declares it as `localization.on_failure` (§14.1,
    /// N30).
    pub on_failure: OnFailure,
    /// How long the localizer may take, a part of the overall budget it must
    /// end before (§11.5); zero is refused.
    pub timeout_ms: u64,
}

impl Default for Localization {
    fn default() -> Self {
        Self {
            on_failure: OnFailure::Closed,
            timeout_ms: 5_000,
        }
    }
}

impl Default for Federation {
    fn default() -> Self {
        Self {
            id: None,
            per_node_timeout_ms: 10_000,
            overall_timeout_ms: 25_000,
            default_namespace: None,
            binding_ttl_ms: 900_000,
            binding_capacity: 100_000,
            ehr_index_capacity: 100_000,
            node_selection: None,
            localization: None,
            best_effort: true,
            offset_strategy: OffsetPaging::Bounded,
            max_offset_window: 1000,
            decomposable_aggregates: vec![
                DecomposableAggregate::Count,
                DecomposableAggregate::Sum,
                DecomposableAggregate::Min,
                DecomposableAggregate::Max,
                DecomposableAggregate::Avg,
            ],
            demographic_endpoint: None,
            fan_out_template_upload: false,
            fan_out_stored_queries: false,
            consent: ConsentReporting::default(),
        }
    }
}

/// The HTTP surface.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    /// The socket address to bind.
    pub listen: String,
    /// The path of the deployment's base URL, `{base}`, which every route
    /// sits under: `/`, the default, or a path such as `/fed/openehr` with no
    /// trailing `/`, query or fragment (§4.1, N28). The specification
    /// reserves no prefix.
    pub base_path: String,
    /// How long one request may take before the server answers `408`. With a
    /// registry configured, it must exceed `federation.overall_timeout_ms`
    /// by more than [`COMBINING_MARGIN_MS`].
    pub request_timeout_ms: u64,
    /// How long the drain may take after the stop signal.
    pub shutdown_timeout_ms: u64,
    /// The largest request body the server reads before answering `413`.
    pub body_limit_bytes: usize,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            listen: String::from("127.0.0.1:8080"),
            base_path: String::from("/"),
            request_timeout_ms: 30_000,
            shutdown_timeout_ms: 10_000,
            body_limit_bytes: 1024 * 1024,
        }
    }
}

/// The console and the trace export.
///
/// The export is off by default. No specification governs telemetry: our own
/// design.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Telemetry {
    /// The rendering: `auto`, `json` or `pretty`.
    pub format: Format,
    /// The `tracing` filter directive of the console.
    pub filter: String,
    /// The `http://` URL of an OTLP collector the gateway's spans are
    /// exported to over gRPC. Unset, no span is exported and no node
    /// request carries a `traceparent`.
    pub otlp_endpoint: Option<SecretUrl>,
    /// The share of the gateway's traces that are sampled, from `0.0`, none,
    /// to `1.0`, every one; `1.0` unless set.
    pub trace_sample_ratio: f64,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self {
            format: Format::Auto,
            filter: String::from(DEFAULT_FILTER),
            otlp_endpoint: None,
            trace_sample_ratio: 1.0,
        }
    }
}

/// The metrics surface: one meter provider read by the Prometheus text
/// exposition on its own listener and, when set, pushed over OTLP.
///
/// Both are off by default. No specification governs metrics: our own design.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Metrics {
    /// The socket address the admin listener binds to serve `GET /metrics`.
    /// Unset, no listener runs; it never shares `server.listen`.
    pub listen: Option<String>,
    /// Whether `listen` may name an address other than a loopback one. A
    /// remote address is refused unless this is set, because the listener
    /// has no authentication of its own.
    pub allow_remote: bool,
    /// The `http://` URL of an OTLP collector the metrics are pushed to over
    /// gRPC. Unset, nothing is pushed.
    pub otlp_endpoint: Option<SecretUrl>,
}

/// The credentials one endpoint expects.
///
/// A section names a bearer token, basic credentials, an OAuth 2.0 grant, a
/// Nuts grant or a FAPI 2.0 grant, and for a node the TLS client
/// certificate the gateway presents to it and to its authorization server.
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error, and so is naming two schemes. A node's section may
/// name its TLS material alone, when the transport authenticates the
/// gateway. An inline secret is a [`Secret`], which no rendering shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    /// An RFC 6750 bearer token.
    pub bearer_token: Option<Secret>,
    /// A file holding the bearer token, read at boot.
    pub bearer_token_file: Option<PathBuf>,
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<Secret>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
    /// An OAuth 2.0 client-credentials grant at the node's token endpoint
    /// (`[credentials."<endpoint id>".oauth2]`), the default onward mechanism
    /// of §13.1 (N25).
    pub oauth2: Option<OAuth2>,
    /// The Nuts grant of the Dutch Generic Functions
    /// (`[credentials."<endpoint id>".nuts]`), the regional realisation of
    /// §13.3 (Annex B §B.4).
    #[cfg(feature = "binding-nl")]
    pub nuts: Option<crate::binding::nl::nuts::Nuts>,
    /// A grant under the FAPI 2.0 Security Profile
    /// (`[credentials."<endpoint id>".fapi2]`), the BgZ/eOverdracht track of
    /// Annex B §B.4a.
    pub fapi2: Option<grant::Fapi2>,
    /// The gateway's TLS client certificate chain and private key, PEM,
    /// which it presents to the node and to its authorization server (RFC
    /// 8705); a node's section alone takes it, and the node is then reached
    /// over `https` alone.
    pub client_identity: Option<Secret>,
    /// A file holding `client_identity`, read at boot.
    pub client_identity_file: Option<PathBuf>,
    /// A PEM bundle of trust roots, beside the platform's, the node and its
    /// authorization server are trusted by; a node's section alone takes it.
    pub trust_roots_file: Option<PathBuf>,
}

impl Credentials {
    /// Whether the section names TLS material, which only a node's section
    /// takes.
    #[must_use]
    pub fn names_tls(&self) -> bool {
        self.client_identity.is_some()
            || self.client_identity_file.is_some()
            || self.trust_roots_file.is_some()
    }
}

/// An OAuth 2.0 grant at one node's token endpoint.
///
/// The gateway authenticates with a JWT client assertion signed by the
/// `[signing]` key (RFC 7523 §2.2), and every token is requested with
/// `scope`. No field has a default.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OAuth2 {
    /// The grant: `client_credentials` (RFC 6749 §4.4), or `token_exchange`
    /// (RFC 8693), a token per verified caller.
    pub grant: Option<GrantKind>,
    /// How the gateway authenticates at the token endpoint:
    /// `private_key_jwt`, a JWT client assertion (RFC 7523 §2.2), or
    /// `tls_client_auth` or `self_signed_tls_client_auth`, the section's
    /// TLS client certificate (RFC 8705 §2).
    pub client_auth: Option<ClientAuth>,
    /// The token endpoint, an `http` or `https` URL with no userinfo; by
    /// default also the `aud` of every client assertion (RFC 7523 §3).
    pub token_endpoint: Option<SecretUrl>,
    /// What every client assertion names as its `aud`: `token_endpoint`,
    /// the default, or `issuer` (RFC 7523 §3; FAPI 2.0 §5.3.2.1).
    pub assertion_audience: grant::AssertionAudience,
    /// The authorization server's issuer identifier (RFC 8414 §2), set with
    /// `assertion_audience = "issuer"` and never without it.
    pub issuer: Option<String>,
    /// The client the node's authorization server registered the gateway
    /// as, the `iss` and `sub` of every client assertion (RFC 7523 §3).
    pub client_id: String,
    /// The scope every token is requested with, space-delimited (RFC 6749
    /// §3.3), each a SMART on openEHR resource scope of the `system`
    /// compartment, such as `system/aql-*.s`.
    pub scope: String,
    /// The target service the token is for (RFC 8707 §2), when the
    /// authorization server takes one.
    pub resource: Option<String>,
    /// The audience the token is asked for, when the authorization server
    /// takes one.
    pub audience: Option<String>,
    /// A file holding the private key the tokens are bound to with `DPoP`
    /// (RFC 9449), a P-256 or P-384 key in PKCS#8 PEM, read at boot. Unset,
    /// the tokens are bearer tokens.
    pub dpop_key_file: Option<PathBuf>,
    /// Whether the tokens are bound to the section's TLS client certificate
    /// (RFC 8705 §3, named as the client metadata of §3.4 names it): a
    /// token whose stated binding names another certificate is refused.
    /// Never set with `dpop_key_file`.
    pub tls_client_certificate_bound_access_tokens: bool,
}

/// The OAuth 2.0 grant the gateway uses at a node's token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum GrantKind {
    /// The client-credentials grant (RFC 6749 §4.4).
    ClientCredentials,
    /// Token exchange (RFC 8693): the verified caller's token exchanged for
    /// a token of the node, the gateway as the actor. The gateway's own
    /// requests use the client-credentials grant.
    TokenExchange,
}

/// How the gateway authenticates at a node's token endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ClientAuth {
    /// A JWT client assertion signed with the gateway's key (RFC 7523 §2.2,
    /// RFC 7521 §4.2).
    PrivateKeyJwt,
    /// The section's TLS client certificate, validated against the
    /// authorization server's PKI (RFC 8705 §2.1).
    #[serde(rename = "tls_client_auth")]
    Tls,
    /// The section's TLS client certificate, matched against the one the
    /// authorization server registered for the client (RFC 8705 §2.2).
    #[serde(rename = "self_signed_tls_client_auth")]
    SelfSignedTls,
}

/// The gateway's signing keys and their publication (§13.1, N25).
///
/// The current key signs every client assertion. The previous key, during a
/// rotation, is published beside it for `rotation_overlap_s` from the start
/// of the process and never signs. Both are ES384 (P-384) private keys in
/// PKCS#8 PEM, read from files at boot. The JWK Set is served at
/// `{base}/.well-known/jwks.json`, and `jwks_uri` is the absolute URL the
/// `OPTIONS {base}/` body declares for it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Signing {
    /// The file holding the current key.
    pub key_file: Option<PathBuf>,
    /// The file holding the previous key, during a rotation.
    pub previous_key_file: Option<PathBuf>,
    /// The absolute URL nodes fetch the JWK Set from: the gateway's own
    /// `{base}/.well-known/jwks.json` at its public address, or wherever the
    /// deployment publishes the keys (§13.1).
    pub jwks_uri: Option<String>,
    /// How long a client assertion is valid, in seconds: at most 300.
    pub assertion_lifetime_s: u64,
    /// How long the nodes cache the JWK Set, in seconds.
    pub node_jwks_cache_s: u64,
    /// How long the previous key stays published, in seconds: at least
    /// `assertion_lifetime_s` plus `node_jwks_cache_s`.
    pub rotation_overlap_s: u64,
}

impl Default for Signing {
    fn default() -> Self {
        Self {
            key_file: None,
            previous_key_file: None,
            jwks_uri: None,
            assertion_lifetime_s: 300,
            node_jwks_cache_s: 3600,
            rotation_overlap_s: 3900,
        }
    }
}
