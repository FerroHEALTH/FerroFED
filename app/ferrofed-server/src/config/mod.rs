// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Configuration: an optional TOML file, then environment overrides, then one
//! typed [`Settings`](settings::Settings) the run path holds.
//!
//! Every struct refuses an unknown key, every default lives inline in its own
//! `Default` impl, every secret is reachable through a `<key>_file` sibling
//! read at boot, and a bad value refuses to boot rather than falling back. No
//! specification governs the configuration: our own design.

use ferrofed_identity::profile::Profile;
use ferrofed_identity::role::localizer::OnFailure;
use ferrohealth_config::removed::Removed;
use ferrohealth_config::tls::TlsClientSection;
use ferrohealth_config::units;
use ferrohealth_secret::secret::Secret;
use openehr_federation::aggregate::AggregateFunction;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrohealth_telemetry::section::TelemetrySection;

pub mod access_log;
pub mod audit;
pub mod auth;
pub mod base_path;
pub mod catalog;
pub mod eehrxf;
pub mod emergency;
pub mod error;
pub mod fhir;
pub mod gateway;
#[cfg(feature = "binding-ihe")]
pub mod ihe;
pub mod limits;
pub mod load;
#[cfg(feature = "binding-nl")]
pub mod nl;
pub mod oauth;
pub mod postgres;
pub mod public_url;
pub mod registry;
pub(crate) mod resolve;
pub mod retired;
pub mod revision_policy;
pub mod revisions;
pub(crate) mod secrets;
pub mod server;
pub mod service;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
pub mod service_grant;
pub mod settings;
pub mod store;
pub mod stored_queries;
pub mod tls;
pub mod transport;
pub mod tree;

/// The namespace of the configuration ([`load::loader`]).
///
/// `FERROFED__<SECTION>__<KEY>` overrides a key, upper or lower case:
/// `FERROFED__SERVER__LISTEN` sets `[server] listen`; `FERROFED_CONFIG`
/// names the file.
pub const PREFIX: &str = "FERROFED";

/// The milliseconds kept for combining the answers after the fan-out budget.
///
/// `server.request_timeout` must exceed `federation.overall_timeout` by
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
/// unknown keys. The IHE binding's sections are resolved here
/// (`config::ihe::resolve`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The deployment profile: `production`, or `development`.
    pub profile: Profile,
    /// The HTTP surface, the section of `ferrohealth-server` with the
    /// gateway's [`server::defaults`].
    pub server: ferrohealth_server::section::ServerSection,
    /// The gateway's own keys beside `[server]`: its public URL, the name
    /// of this process and the bindings' drain.
    pub gateway: gateway::Gateway,
    /// The console, the trace export and the metrics push
    /// (`ferrohealth-telemetry`).
    pub telemetry: TelemetrySection,
    /// The federation's membership.
    pub registry: registry::Registry,
    /// The federated query: the budgets and the default issuing namespace.
    pub federation: Federation,
    /// The outbound credentials, one section per endpoint id
    /// (`[credentials."<endpoint id>"]`).
    ///
    /// The registry names the endpoints, and the node dispatch hands each one
    /// its credentials.
    pub credentials: BTreeMap<String, Credentials>,
    /// The PIXm resolver (`[pixm]`): the PIX Managers and each member's
    /// `ehr_id` domain there (#43).
    #[cfg(feature = "binding-ihe")]
    pub pixm: Option<ihe::pixm::Pixm>,
    /// The XCPD localizer (`[xcpd]`): the responding gateways and the
    /// community each member serves (Annex A.3, #85).
    #[cfg(feature = "binding-ihe")]
    pub xcpd: Option<ihe::xcpd::Xcpd>,
    /// The Dutch Generic Functions (`[nl_gf]`): the NVI localizer and the
    /// care provider each member holds the data of (Annex B, #87).
    #[cfg(feature = "binding-nl")]
    pub nl_gf: Option<nl::NlGf>,
    /// The PMIR identity feed (`[pmir]`): the Patient Identity Registry the
    /// gateway subscribes to with ITI-94, and the path its ITI-93 messages
    /// arrive at (track 8 of §16.3, Annex A.4).
    #[cfg(feature = "binding-ihe")]
    pub pmir: Option<ihe::pmir::Pmir>,
    /// The PDQm demographics step (`[pdqm]`): the Patient Demographics
    /// Supplier asked for the master identity of an identifier the
    /// cross-reference does not map (Annex A §A.2, #487).
    #[cfg(feature = "binding-ihe")]
    pub pdqm: Option<ihe::pdqm::Pdqm>,
    /// The federated stored-query registry (`[stored_queries]`, §12.7).
    pub stored_queries: stored_queries::StoredQueries,
    /// FerroFED's own PostgreSQL store (`[store]`), unset unless the table
    /// is written.
    pub store: Option<store::Store>,
    /// The limit of each relayed health professional's emergency accesses
    /// (`[emergency]`, eHDSI Section II Security Services 10.0.0 §2.5).
    pub emergency: emergency::Emergency,
    /// The FHIR R4 face of the European exchange format (`[fhir]`,
    /// Regulation (EU) 2025/327 Annex II 2.1), off unless the table is set.
    pub fhir: Option<fhir::Fhir>,
    /// The interoperability component (`[eehrxf]`): the optional FHIR
    /// terminology server it asks.
    pub eehrxf: eehrxf::Eehrxf,
    /// The metrics surface (`[metrics]`): the admin listener and the OTLP
    /// push, both off by default.
    pub metrics: server::Metrics,
    /// The gateway's signing keys (`[signing]`), which sign the client
    /// assertion of every OAuth 2.0 grant to a node and which the gateway
    /// publishes as a JWK Set (§13.1, N25).
    pub signing: Option<Signing>,
    /// How a caller authenticates to the gateway (`[auth]`, §13.1, N25):
    /// the section `ferrohealth-smart` reads.
    pub auth: auth::Auth,
    /// What each trusted issuer's tokens are honoured for, the introspected
    /// issuer, the edge mode and the purpose-of-use rule (`[trust]`).
    pub trust: auth::trust::Trust,
    /// The category map every access record is classified with
    /// (`[access_log]`, #623).
    pub access_log: access_log::AccessLog,
    /// Where the access records and the IHE binding's BALP records go
    /// (`[audit]`, #486, #697).
    pub audit: audit::Audit,
    /// Configuration revisions (`[config]`): `ui`, off by default, and the store.
    #[serde(rename = "config")]
    pub revisions: revisions::Revisions,
    /// The installation report to `FerroPULSE` (`[usage_report]`), on by
    /// default.
    pub usage_report: ferrohealth_usage_report::section::UsageReportSection,
    /// The removed `[dev]` table (#1377), read only so that a configuration
    /// still setting it is refused by name
    /// ([`Error::DevRemoved`](error::Error::DevRemoved)); its rows are never
    /// kept, since one held a patient identifier.
    pub dev: Option<serde::de::IgnoredAny>,
    /// Whether a source set `server.shutdown_timeout`; unset, the drain
    /// lasts `server.request_timeout` ([`Config::load`] and
    /// [`Config::from_sources`] read it from the provenance).
    #[serde(skip)]
    pub shutdown_timeout_written: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            profile: Profile::Production,
            server: server::defaults(),
            gateway: gateway::Gateway::default(),
            telemetry: TelemetrySection::default(),
            registry: registry::Registry::default(),
            federation: Federation::default(),
            credentials: BTreeMap::new(),
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
            store: None,
            emergency: emergency::Emergency::default(),
            fhir: None,
            eehrxf: eehrxf::Eehrxf::default(),
            metrics: server::Metrics::default(),
            signing: None,
            auth: auth::Auth::default(),
            trust: auth::trust::Trust::default(),
            access_log: access_log::AccessLog::default(),
            audit: audit::Audit::default(),
            revisions: revisions::Revisions::default(),
            usage_report: ferrohealth_usage_report::section::UsageReportSection::default(),
            dev: None,
            shutdown_timeout_written: false,
        }
    }
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
    pub per_node_timeout: units::Duration,
    /// The retired `per_node_timeout_ms`, which `per_node_timeout` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub per_node_timeout_ms: Removed,
    /// How long the whole fan-out may take (§11.5, N38). With a registry
    /// configured, `server.request_timeout` must exceed it by more than
    /// [`COMBINING_MARGIN_MS`], so the gateway answers with its envelope
    /// before the request timeout cuts the connection.
    pub overall_timeout: units::Duration,
    /// The retired `overall_timeout_ms`, which `overall_timeout` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub overall_timeout_ms: Removed,
    /// The issuing namespace an unqualified patient identifier resolves in
    /// (§5.2 requires the namespace; no specification governs the default: our
    /// own design). Without it, a query that names no namespace is a `400`.
    pub default_namespace: Option<String>,
    /// How long each resolution binding of a client session lives after the
    /// last resolution that returned it (§12.5.1 step 2): a correctness
    /// bound, past which the binding is never routed on.
    pub binding_ttl: units::Duration,
    /// The retired `binding_ttl_ms`, which `binding_ttl` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub binding_ttl_ms: Removed,
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
    /// The most rows the gateway reads of one node for a query whose AQL
    /// carries no `LIMIT`: each node is sent `fetch` set to it plus one, so
    /// no node's own default page applies, and a node that answers more
    /// rows is `node-error` (§11.1, §11.5); zero is refused. A query with a
    /// `LIMIT` is sent that `LIMIT` as its `fetch`.
    pub max_node_rows: u32,
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
    /// The most requests the gateway sends to one member endpoint at once. A
    /// request past it waits for a slot until its per-node deadline, and one
    /// still waiting then is `time-out` with nothing sent (§11.5, N38). Zero is
    /// refused.
    pub max_in_flight_per_node: u32,
    /// The most bytes read of one member answer, past which it is `node-error` (§11.1).
    pub max_node_answer_size: units::ByteSize,
    /// The retired `max_node_answer_bytes`, which `max_node_answer_size` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub max_node_answer_bytes: Removed,
    /// What the gateway needs of each national contact point, one
    /// `[[federation.contact_point]]` per `[[auth.issuer]]` whose
    /// `[[auth.federation]]` table sets `contact_point = true`.
    pub contact_point: Vec<auth::contact_point::ContactPointTable>,
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
    /// Whether the pre-filter's exclusion applies to a request whose
    /// verified token declares an `[[access_log.emergency_purpose]]`
    /// (Regulation (EU) 2025/327 Art 11(5)): `"apply"`, the default, or
    /// `"pass-to-node"`; `OPTIONS {base}/` declares it as
    /// `consent.emergency` (§7a.2).
    pub emergency: EmergencyConsent,
}

impl Default for ConsentReporting {
    fn default() -> Self {
        Self {
            disclose: true,
            emergency: EmergencyConsent::Apply,
        }
    }
}

/// What the Step-1 consent pre-filter does for a request whose verified
/// token declares an emergency purpose of use, `[federation.consent]
/// emergency` (N27a, Regulation (EU) 2025/327 Art 11(5)).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EmergencyConsent {
    /// The pre-filter applies as to any request: a member it denies is not
    /// asked.
    #[default]
    Apply,
    /// The pre-filter is still asked, and a member it denies is asked all
    /// the same, its node deciding (N26, N27); the access record names
    /// every member whose denial was set aside.
    PassToNode,
}

impl EmergencyConsent {
    /// The value as the configuration and `OPTIONS {base}/` write it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::PassToNode => "pass-to-node",
        }
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
    pub timeout: units::Duration,
    /// The retired `timeout_ms`, which `timeout` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub timeout_ms: Removed,
}

impl Default for Localization {
    fn default() -> Self {
        Self {
            on_failure: OnFailure::Closed,
            timeout: units::Duration::from_secs(5),
            timeout_ms: Removed::default(),
        }
    }
}

impl Default for Federation {
    fn default() -> Self {
        Self {
            id: None,
            per_node_timeout: units::Duration::from_secs(10),
            per_node_timeout_ms: Removed::default(),
            overall_timeout: units::Duration::from_secs(25),
            overall_timeout_ms: Removed::default(),
            default_namespace: None,
            binding_ttl: units::Duration::from_mins(15),
            binding_ttl_ms: Removed::default(),
            binding_capacity: 100_000,
            ehr_index_capacity: 100_000,
            node_selection: None,
            localization: None,
            best_effort: true,
            offset_strategy: OffsetPaging::Bounded,
            max_offset_window: 1000,
            // NOTE: no specification governs the ceiling: our own design; one under 10 000, so the
            // probe row it adds fits a node that serves 10 000 rows a page, FerroEHR's default.
            max_node_rows: 9_999,
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
            max_in_flight_per_node: 64,
            max_node_answer_size: units::ByteSize::from_mib(16),
            max_node_answer_bytes: Removed::default(),
            contact_point: Vec::new(),
        }
    }
}

/// The credentials one endpoint expects.
///
/// A section names basic credentials, the OAuth 2.0 client credentials grant
/// of `ferrohealth-smart` (`oauth`), under the FAPI 2.0 Security Profile
/// where it says so, or a Nuts grant, and for a node its onward lane and the
/// TLS client certificate the gateway presents to it and to its
/// authorization server. Machine-to-machine authentication is HTTP
/// Basic or an OAuth 2.0 grant, never a static bearer token.
/// Every secret is reachable inline or through its `_file` sibling; setting
/// both is a boot error, and so is naming two schemes. A node's section may
/// name its TLS material alone, when the transport authenticates the
/// gateway. An inline secret is a [`Secret`], which no rendering shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Credentials {
    /// The user name of RFC 7617 basic authentication.
    pub user: Option<String>,
    /// The password of RFC 7617 basic authentication.
    pub password: Option<Secret>,
    /// A file holding the password, read at boot.
    pub password_file: Option<PathBuf>,
    /// The OAuth 2.0 client credentials grant (RFC 6749 §4.4) of
    /// `ferrohealth-smart` (`[credentials."<endpoint id>".oauth]`), the
    /// default onward mechanism of §13.1 (N25): a token obtained with a
    /// client secret or a `private_key_jwt` assertion, cached, and renewed
    /// once on a `401`.
    pub oauth: Option<oauth::Oauth>,
    /// The onward lane of a node (`[credentials."<endpoint id>".onward]`),
    /// the `[<lane>.credentials.onward]` table of `ferrohealth-smart`: every
    /// request a verified caller originates reaches the node under the
    /// token the table's issuer exchanges the caller's for (RFC 8693 §2.1,
    /// RFC 7523 §2.1), authenticated by `oauth`, whose own token carries
    /// only the gateway's own requests. A node's section alone takes it.
    pub onward: Option<ferrohealth_smart::config::onward::OnwardSection>,
    /// Whether the node's onward lane carries another party's organisation,
    /// a request no named person stands behind, which its `federation`
    /// issuer exchanges for a token naming the organisation alone and the
    /// node is reached with read-only; the deployment's choice per
    /// agreement, `false` by default. Only an `onward` table of kind
    /// `federation` takes it.
    pub onward_organisations: bool,
    /// The Nuts grant of the Dutch Generic Functions
    /// (`[credentials."<endpoint id>".nuts]`), the regional realisation of
    /// §13.3 (Annex B §B.4).
    #[cfg(feature = "binding-nl")]
    pub nuts: Option<nl::nuts::Nuts>,
    /// The TLS material the node and its authorization server are reached
    /// with, `[credentials."<endpoint id>".tls]` (RFC 8705); a node's section
    /// alone takes it, and the node is then reached over `https` alone when
    /// it names a client identity.
    pub tls: Option<TlsClientSection>,
}

impl Credentials {
    /// Whether the section names TLS material, which only a node's section
    /// takes.
    #[must_use]
    pub fn names_tls(&self) -> bool {
        self.tls.is_some()
    }
}

/// The gateway's signing keys and their publication (§13.1, N25).
///
/// The current key signs every client assertion and every conveyance. The
/// previous key, during a rotation, is published beside it for
/// `rotation_overlap` from the start of the process and never signs. The
/// next key, ahead of a rotation, is published beside them and never signs,
/// so every node holds it before any replica signs with it. Each
/// is a P-256 or a P-384 private key in PKCS#8 PEM, read from its file at
/// boot, and signs ES256 or ES384 as its curve says (RFC 7518 §3.4). The
/// JWK Set is served at
/// `{base}/.well-known/jwks.json`, and `jwks_uri` is the absolute URL the
/// `OPTIONS {base}/` body declares for it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Signing {
    /// The file holding the current key.
    pub key_file: Option<PathBuf>,
    /// The file holding the previous key, during a rotation.
    pub previous_key_file: Option<PathBuf>,
    /// The file holding the next key, published ahead of a rotation and
    /// never signing.
    pub next_key_file: Option<PathBuf>,
    /// The algorithm the next key is meant to sign with, `ES256` or `ES384`;
    /// unset, the current key's. A next key on another curve is refused.
    pub next_key_algorithm: Option<SigningAlgorithm>,
    /// The absolute URL nodes fetch the JWK Set from: the gateway's own
    /// `{base}/.well-known/jwks.json` at its public address, or wherever the
    /// deployment publishes the keys (§13.1).
    pub jwks_uri: Option<String>,
    /// How long the nodes cache the JWK Set.
    pub node_jwks_cache: units::Duration,
    /// The retired `node_jwks_cache_s`, which `node_jwks_cache` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub node_jwks_cache_s: Removed,
    /// How long the previous key stays published, in seconds: at least 300
    /// plus `node_jwks_cache`.
    pub rotation_overlap: units::Duration,
    /// The retired `rotation_overlap_s`, which `rotation_overlap` replaced in
    /// 0.0.18: set, it refuses the configuration.
    #[serde(skip_serializing)]
    pub rotation_overlap_s: Removed,
}

impl Default for Signing {
    fn default() -> Self {
        Self {
            key_file: None,
            previous_key_file: None,
            next_key_file: None,
            next_key_algorithm: None,
            jwks_uri: None,
            node_jwks_cache: units::Duration::from_hours(1),
            node_jwks_cache_s: Removed::default(),
            rotation_overlap: units::Duration::from_mins(65),
            rotation_overlap_s: Removed::default(),
        }
    }
}

/// The algorithm a signing key signs with, which its curve decides (RFC 7518
/// §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum SigningAlgorithm {
    /// ECDSA over P-256 with SHA-256.
    #[serde(rename = "ES256")]
    Es256,
    /// ECDSA over P-384 with SHA-384.
    #[serde(rename = "ES384")]
    Es384,
}

impl SigningAlgorithm {
    /// The algorithm as the JOSE library names it.
    #[must_use]
    pub const fn algorithm(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::Es256 => jsonwebtoken::Algorithm::ES256,
            Self::Es384 => jsonwebtoken::Algorithm::ES384,
        }
    }
}
