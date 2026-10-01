// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `OPTIONS {base}/` self-description of a federation gateway (§7a.2,
//! N30, CP-23), the contract of `options-root.schema.json`.
//!
//! The body has no ITS-REST counterpart, so every member here is normative in
//! the federation specification alone. The two schema conditionals are
//! invariants of [`Completeness`] (best-effort needs `opt_in`) and
//! [`DefinitionBehaviour`] (stored-query fan-out needs the registry).

use std::fmt;

use serde::de::Deserializer;
use serde::ser::{SerializeMap, Serializer};
use serde::{Deserialize, Serialize};

use crate::error::WireError;
use crate::id::{EndpointId, FederationId};
use crate::object::{Extra, Members, Uri};
use crate::outcome::optional_entry;

/// The specification version a gateway implements, in `major.minor` form
/// (§7a.2): a patch release is editorial and never appears here.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpecVersion(String);

impl SpecVersion {
    /// Checks that `text` is `major.minor`, both parts ASCII digits.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::SpecVersion`] for any other form, a three-part
    /// version included.
    pub fn new(text: impl Into<String>) -> Result<Self, WireError> {
        let text = text.into();
        let digits =
            |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        match text.split_once('.') {
            Some((major, minor)) if digits(major) && digits(minor) => Ok(Self(text)),
            _ => Err(WireError::SpecVersion),
        }
    }

    /// The `major.minor` of a release version such as `0.9.0`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::SpecVersion`] when `release` has no numeric
    /// `major.minor` prefix.
    pub fn of_release(release: &str) -> Result<Self, WireError> {
        let mut parts = release.split('.');
        match (parts.next(), parts.next()) {
            (Some(major), Some(minor)) => Self::new(format!("{major}.{minor}")),
            _ => Err(WireError::SpecVersion),
        }
    }

    /// The version as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SpecVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for SpecVersion {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for SpecVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A member endpoint's standing membership and health status (§7a.2).
///
/// The specification defines no closed vocabulary here, so this is an open,
/// non-empty string (`active` is the conventional value). The one value it
/// refuses is `not-localized`, a per-query §11.1 outcome that means nothing
/// as a membership status.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MembershipStatus(String);

impl MembershipStatus {
    /// Checks `status` against the two rules of the schema.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::EmptyMember`] for an empty status and
    /// [`WireError::MembershipNotLocalized`] for `not-localized`.
    pub fn new(status: impl Into<String>) -> Result<Self, WireError> {
        let status = status.into();
        if status.is_empty() {
            return Err(WireError::EmptyMember {
                object: "endpoint",
                member: "status",
            });
        }
        if status == "not-localized" {
            return Err(WireError::MembershipNotLocalized);
        }
        Ok(Self(status))
    }

    /// The status as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for MembershipStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for MembershipStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// How the gateway exposes the DEMOGRAPHIC area, any free-form description
/// except `federated` (N32: the DEMOGRAPHIC API is never federated).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemographicSupport(String);

impl DemographicSupport {
    /// Checks that `support` does not declare the area federated.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::FederatedDemographic`] for `federated`.
    pub fn new(support: impl Into<String>) -> Result<Self, WireError> {
        let support = support.into();
        if support == "federated" {
            return Err(WireError::FederatedDemographic);
        }
        Ok(Self(support))
    }

    /// The description as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for DemographicSupport {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for DemographicSupport {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The dedup default, which N15 fixes as `none`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct DedupDefault;

impl DedupDefault {
    /// The only conformant value.
    pub const VALUE: &'static str = "none";
}

impl Serialize for DedupDefault {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(Self::VALUE)
    }
}

impl<'de> Deserialize<'de> for DedupDefault {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if String::deserialize(deserializer)? == Self::VALUE {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom(WireError::DedupDefaultNotNone))
        }
    }
}

/// The dedup modes a gateway offers, at least one (§10, N15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupModes(Vec<String>);

impl DedupModes {
    /// Checks that `modes` lists at least one mode.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::NoDedupModes`] for an empty list.
    pub fn new(modes: Vec<String>) -> Result<Self, WireError> {
        if modes.is_empty() {
            return Err(WireError::NoDedupModes);
        }
        Ok(Self(modes))
    }

    /// The modes as listed.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl Serialize for DedupModes {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DedupModes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::new(Vec::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// The whole `OPTIONS {base}/` body: the gateway's self-description and its
/// member endpoints, siblings at the top level (§7a.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsRoot {
    /// The gateway self-description.
    pub federation: GatewayDescription,
    /// The member endpoints, the identifiers a directive can name. Federation
    /// membership information, never gated on a patient identifier.
    pub endpoints: Vec<MemberEndpoint>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(OptionsRoot, "options", {
    federation: required,
    endpoints: required,
});

/// The `federation` object of the `OPTIONS` body: every behaviour N30
/// requires a gateway to declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayDescription {
    /// The federation's own identifier (N30).
    pub id: FederationId,
    /// The specification version implemented, `major.minor` (§7a.2).
    pub spec_version: SpecVersion,
    /// AQL-level federation behaviour (§8).
    pub aql: AqlBehaviour,
    /// The deduplication policy (§10, N15).
    pub dedup: DedupPolicy,
    /// The timeout policy (§11.5, N38).
    pub timeout: TimeoutPolicy,
    /// The completion strategies (§11.4, N37).
    pub completeness: Completeness,
    /// The `OFFSET > 0` strategy (§11.6.2, N39).
    pub paging: Paging,
    /// The decomposable aggregates supported across nodes (§11.6.3).
    pub aggregates: Aggregates,
    /// The definition-area behaviour (§12.6, N43, §12.7, N44).
    pub definition: DefinitionBehaviour,
    /// The behaviour when localization is unavailable (§14.1).
    pub localization: Localization,
    /// The JWKS location (§13.1), absent when none is configured.
    pub auth: Option<AuthDescription>,
    /// Which ITS-REST areas are exposed and how (§7a.1, N30, N32).
    pub its_rest: ItsRestAreas,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(GatewayDescription, "federation", {
    id: required,
    spec_version: required,
    aql: required,
    dedup: required,
    timeout: required,
    completeness: required,
    paging: required,
    aggregates: required,
    definition: required,
    localization: required,
    auth: optional,
    its_rest: required,
});

/// AQL-level federation behaviour, `federation.aql` (§8).
///
/// The targeting mechanisms are not declared: both the directive and the
/// header are mandatory at every conformant gateway (N35).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AqlBehaviour {
    /// Whether an untargeted query fans out.
    pub fan_out: bool,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(AqlBehaviour, "aql", { fan_out: required });

/// The deduplication policy, `federation.dedup` (§10, N15).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupPolicy {
    /// The default mode, which N15 fixes as `none`.
    pub default: DedupDefault,
    /// The modes offered.
    pub modes: DedupModes,
    /// The request header that selects a mode.
    pub request_header: Option<String>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(DedupPolicy, "dedup", {
    default: required,
    modes: required,
    request_header: optional,
});

/// The timeout policy, `federation.timeout` (§11.5, N38).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeoutPolicy {
    /// The budget for one node's request, in milliseconds.
    pub per_node_ms: u64,
    /// The budget for the whole fan-out, in milliseconds.
    pub overall_ms: u64,
    /// The completion policy the budget applies under.
    pub policy: String,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(TimeoutPolicy, "timeout", {
    per_node_ms: required,
    overall_ms: required,
    policy: required,
});

/// How a request selects best-effort completion, `completeness.opt_in`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OptIn {
    /// The request header.
    pub header: Option<String>,
    /// The header value.
    pub value: Option<String>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(OptIn, "opt_in", {
    header: optional,
    value: optional,
});

/// The completion strategies, `federation.completeness` (§11.4, N37).
///
/// The default is all-or-nothing, the only conformant value. A gateway that
/// offers best-effort says how a request selects it, which is why the
/// best-effort constructor takes the [`OptIn`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completeness {
    best_effort: bool,
    opt_in: Option<OptIn>,
    extra: Extra,
}

impl Completeness {
    /// The member names this type models, as the schema spells them.
    pub const MEMBERS: &'static [&'static str] = &["default", "best_effort", "opt_in"];

    /// The only conformant `default`.
    pub const DEFAULT: &'static str = "all-or-nothing";

    /// A gateway that offers no best-effort mode.
    #[must_use]
    pub fn all_or_nothing_only() -> Self {
        Self {
            best_effort: false,
            opt_in: None,
            extra: Extra::new(),
        }
    }

    /// A gateway that offers best-effort completion, selected as `opt_in`
    /// says.
    #[must_use]
    pub fn with_best_effort(opt_in: OptIn) -> Self {
        Self {
            best_effort: true,
            opt_in: Some(opt_in),
            extra: Extra::new(),
        }
    }

    /// Whether best-effort completion is offered.
    #[must_use]
    pub fn best_effort(&self) -> bool {
        self.best_effort
    }

    /// How a request selects best-effort completion, if declared.
    #[must_use]
    pub fn opt_in(&self) -> Option<&OptIn> {
        self.opt_in.as_ref()
    }

    /// The members this type does not model.
    #[must_use]
    pub fn extra(&self) -> &Extra {
        &self.extra
    }

    /// The members this type does not model.
    pub fn extra_mut(&mut self) -> &mut Extra {
        &mut self.extra
    }
}

impl Serialize for Completeness {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("default", Self::DEFAULT)?;
        map.serialize_entry("best_effort", &self.best_effort)?;
        optional_entry(&mut map, "opt_in", self.opt_in.as_ref())?;
        self.extra.write(&mut map, "completeness", Self::MEMBERS)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for Completeness {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut members = Members::read("completeness", deserializer)?;
        read_completeness(&mut members)
            .map(|mut completeness| {
                completeness.extra = members.into_extra();
                completeness
            })
            .map_err(serde::de::Error::custom)
    }
}

fn read_completeness(members: &mut Members) -> Result<Completeness, WireError> {
    let default: String = members.required("default")?;
    if default != Completeness::DEFAULT {
        return Err(WireError::CompletenessDefault);
    }
    let best_effort = members.required("best_effort")?;
    let opt_in = members.optional("opt_in")?;
    if best_effort && opt_in.is_none() {
        return Err(WireError::BestEffortWithoutOptIn);
    }
    Ok(Completeness {
        best_effort,
        opt_in,
        extra: Extra::new(),
    })
}

/// The `OFFSET > 0` strategy, `federation.paging` (§11.6.2, N39).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paging {
    /// The strategy, for example `reject`.
    pub offset_strategy: String,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(Paging, "paging", { offset_strategy: required });

/// The decomposable aggregates, `federation.aggregates` (§11.6.3). An empty
/// list declares that none is supported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aggregates {
    /// The aggregate functions supported across nodes.
    pub decomposable: Vec<String>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(Aggregates, "aggregates", { decomposable: required });

/// The definition-area behaviour, `federation.definition` (§12.6, N43,
/// §12.7, N44).
///
/// The stored-query members describe an optional facility, and an absent
/// member means it is not offered. Fan-out of stored-query definitions is a
/// facility of the registry, so declaring it requires
/// `stored_query_registry: true`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionBehaviour {
    fan_out_template_upload: bool,
    stored_query_registry: Option<bool>,
    stored_query_fan_out: Option<bool>,
    extra: Extra,
}

impl DefinitionBehaviour {
    /// The member names this type models, as the schema spells them.
    pub const MEMBERS: &'static [&'static str] = &[
        "fan_out_template_upload",
        "stored_query_registry",
        "stored_query_fan_out",
    ];

    /// A gateway that declares only its template-upload behaviour (N43).
    #[must_use]
    pub fn new(fan_out_template_upload: bool) -> Self {
        Self {
            fan_out_template_upload,
            stored_query_registry: None,
            stored_query_fan_out: None,
            extra: Extra::new(),
        }
    }

    /// Declares whether the stored-query registry is offered (N44).
    ///
    /// # Errors
    ///
    /// Returns [`WireError::FanOutWithoutRegistry`] when stored-query
    /// fan-out is declared and `offered` is false.
    pub fn with_stored_query_registry(mut self, offered: bool) -> Result<Self, WireError> {
        self.stored_query_registry = Some(offered);
        self.check()?;
        Ok(self)
    }

    /// Declares whether stored-query definitions fan out to the nodes (N44).
    ///
    /// # Errors
    ///
    /// Returns [`WireError::FanOutWithoutRegistry`] when `offered` is true and
    /// the registry is not declared offered.
    pub fn with_stored_query_fan_out(mut self, offered: bool) -> Result<Self, WireError> {
        self.stored_query_fan_out = Some(offered);
        self.check()?;
        Ok(self)
    }

    /// Whether a single template `PUT` is applied to every member (N43).
    #[must_use]
    pub fn fan_out_template_upload(&self) -> bool {
        self.fan_out_template_upload
    }

    /// Whether the stored-query registry is offered, if declared.
    #[must_use]
    pub fn stored_query_registry(&self) -> Option<bool> {
        self.stored_query_registry
    }

    /// Whether stored-query definitions fan out, if declared.
    #[must_use]
    pub fn stored_query_fan_out(&self) -> Option<bool> {
        self.stored_query_fan_out
    }

    /// The members this type does not model.
    #[must_use]
    pub fn extra(&self) -> &Extra {
        &self.extra
    }

    /// The members this type does not model.
    pub fn extra_mut(&mut self) -> &mut Extra {
        &mut self.extra
    }

    fn check(&self) -> Result<(), WireError> {
        if self.stored_query_fan_out == Some(true) && self.stored_query_registry != Some(true) {
            return Err(WireError::FanOutWithoutRegistry);
        }
        Ok(())
    }
}

impl Serialize for DefinitionBehaviour {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("fan_out_template_upload", &self.fan_out_template_upload)?;
        optional_entry(
            &mut map,
            "stored_query_registry",
            self.stored_query_registry.as_ref(),
        )?;
        optional_entry(
            &mut map,
            "stored_query_fan_out",
            self.stored_query_fan_out.as_ref(),
        )?;
        self.extra.write(&mut map, "definition", Self::MEMBERS)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for DefinitionBehaviour {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut members = Members::read("definition", deserializer)?;
        read_definition(&mut members)
            .map(|mut definition| {
                definition.extra = members.into_extra();
                definition
            })
            .map_err(serde::de::Error::custom)
    }
}

fn read_definition(members: &mut Members) -> Result<DefinitionBehaviour, WireError> {
    let definition = DefinitionBehaviour {
        fan_out_template_upload: members.required("fan_out_template_upload")?,
        stored_query_registry: members.optional("stored_query_registry")?,
        stored_query_fan_out: members.optional("stored_query_fan_out")?,
        extra: Extra::new(),
    };
    definition.check()?;
    Ok(definition)
}

/// The behaviour when localization is unavailable,
/// `federation.localization` (§14.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Localization {
    /// The declared policy, for example `closed` or `open`.
    pub on_failure: String,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(Localization, "localization", { on_failure: required });

/// The JWKS location, `federation.auth` (§13.1).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthDescription {
    /// The JWKS document location.
    pub jwks_uri: Option<Uri>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(AuthDescription, "auth", { jwks_uri: optional });

/// The ITS-REST areas the gateway exposes, `federation.its_rest` (§7a.1,
/// N30, N32). The values are free-form descriptions a client must not parse
/// as a closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItsRestAreas {
    /// The QUERY area.
    pub query: String,
    /// The EHR area.
    pub ehr: String,
    /// The DEFINITION area.
    pub definition: String,
    /// The DEMOGRAPHIC area, never federated (N32).
    pub demographic: DemographicSupport,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(ItsRestAreas, "its_rest", {
    query: required,
    ehr: required,
    definition: required,
    demographic: required,
});

/// A member endpoint of the `OPTIONS` body (§7a.2, N30): the federation's
/// standing configuration, a different thing from a per-query
/// `meta.federation.endpoints[]` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberEndpoint {
    /// The `endpoint_id`, usable in a directive or the endpoint header.
    pub id: EndpointId,
    /// The managing Organization (N20, N30).
    pub organisation: String,
    /// The membership and health status.
    pub status: MembershipStatus,
    /// The owning node.
    pub node_id: Option<String>,
    /// The node's openEHR `system_id`, where known.
    pub system_id: Option<String>,
    /// The product name.
    pub product: Option<String>,
    /// The product version.
    pub version: Option<String>,
    /// The median latency over recent requests, an aggregate rather than one
    /// request's measurement.
    pub latency_ms_p50: Option<u64>,
    /// The CDR base URL.
    pub url: Option<Uri>,
    /// The members this type does not model.
    pub extra: Extra,
}

plain_record!(MemberEndpoint, "endpoint", {
    id: required,
    organisation: required,
    status: required,
    node_id: optional,
    system_id: optional,
    product: optional,
    version: optional,
    latency_ms_p50: optional,
    url: optional,
});
