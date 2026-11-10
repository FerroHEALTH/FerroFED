// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The body of `OPTIONS {base}/`, the gateway's self-description (§7a.2,
//! N30, CP-23), built from the running federation.
//!
//! [`describe`] builds the body from the running [`Federation`] alone: the
//! budget, the completion strategies, the `OFFSET` strategy, the dedup modes,
//! the decomposable aggregates, the node selection and the registry members
//! are each read from what the query path itself uses, so the declaration
//! cannot drift from the behaviour. [`Federation::load`] builds it once at
//! boot as well, so a configuration it cannot describe refuses to start.
//!
//! The body declares no targeting mechanism and no patient-resolution
//! carrier: both are mandatory at every conformant gateway, so §7a.2 leaves
//! them out on purpose. It needs no patient identifier and holds none.
//! A facility the gateway does not offer and the schema gives no member
//! (asynchronous queries, §11.7) is declared by its absence.
//! A configured Step-1 consent pre-filter is declared under
//! `federation.consent`, with what a query does when it cannot answer
//! (N27a, §13.2.1) and whether an answer names a member it excludes.
//! The manufacturer is named at the top level, beside `federation` and
//! `endpoints`, as Regulation (EU) 2025/327 Art 30(1)(g) asks of an EHR
//! system. Beside it, `supported_until` names the last day of the
//! release's support period, until which fixed releases are published, or
//! is `null` for a build with none (Regulation (EU) 2024/2847 Art 13(19)). The issuers declared national contact points
//! are declared under `federation.national_contact_point`
//! (`crate::auth::contact_point::ContactPoints`), with how their requests
//! are served.

use ferrofed_identity::role::consent::{ConsentPrefilter, ON_UNAVAILABLE};
use ferrofed_operator_api::manufacturer;
use ferrofed_registry::snapshot::{Endpoint, EndpointStatus, RegistrySnapshot};
use openehr_federation::aql::Targeting;
use openehr_federation::error::WireError;
use openehr_federation::headers;
use openehr_federation::id::EndpointId;
use openehr_federation::object::Extra;
use openehr_federation::options::{
    Aggregates, AqlBehaviour, AuthDescription, Completeness, DedupDefault, DedupModes, DedupPolicy,
    DefinitionBehaviour, DemographicSupport, GatewayDescription, ItsRestAreas, Localization,
    MemberEndpoint, MembershipStatus, OptIn, OptionsRoot, Paging, SpecVersion, TimeoutPolicy,
};

use crate::federation::Federation;
use crate::localization::LocalizationPolicy;
use crate::metrics::latency::Percentiles;
use crate::support;

/// The `paging` member that carries the `bounded` strategy's bound on
/// `k + n`.
// NOTE: §11.6.2 bounds the second strategy, and the schema's `paging` object
// is open; it names no member for the bound, so the name is our own design.
pub const MAX_WINDOW: &str = "max_window";

/// The `paging` member that carries the most rows the gateway reads of one
/// node for a query whose AQL carries no `LIMIT`.
// NOTE: §11.5 forbids reading a share a node's default page may have cut as
// complete, and the schema's `paging` object is open; the name is our own design.
pub const MAX_NODE_ROWS: &str = "max_node_rows";

/// The endpoint member that carries the 95th percentile latency over the
/// requests `latency_ms_p50` is the median of, in milliseconds.
// NOTE: §7a.2 has a gateway name the statistic in the field, and options-root.schema.json leaves
// the endpoint object open (`additionalProperties: true`) for it.
pub const LATENCY_MS_P95: &str = "latency_ms_p95";

/// The `localization` member that names the configured localizer's binding.
pub const LOCALIZATION_MODE: &str = "mode";

/// The `localization` member that names where the localizer's audit messages
/// go: `log`, or `off` in a development deployment.
pub const LOCALIZATION_AUDIT: &str = "audit";

/// The `timeout` member that carries the localizer's budget, in
/// milliseconds.
pub const LOCALIZATION_MS: &str = "localization_ms";

/// The `timeout` member that carries the demographics step's budget, in
/// milliseconds.
pub const DEMOGRAPHICS_MS: &str = "demographics_ms";

/// The `timeout` member that carries the consent pre-filter's budget, in
/// milliseconds.
pub const CONSENT_MS: &str = "consent_ms";

/// The `federation` member that declares the Step-1 consent pre-filter.
pub const CONSENT: &str = "consent";

/// The `federation` member that declares the national-contact-point
/// callers (`crate::auth::contact_point::ContactPoints`).
pub const NATIONAL_CONTACT_POINT: &str = "national_contact_point";

/// The top-level member that names the manufacturer, beside `federation`
/// and `endpoints` (Regulation (EU) 2025/327 Art 30(1)(g)).
// NOTE: §7a.2 and options-root.schema.json leave the top level open
// (`additionalProperties: true`): our own design, the member names the manufacturer.
pub const MANUFACTURER: &str = "manufacturer";

/// The top-level member that names the last day of the running release's
/// support period, beside the manufacturer, or `null` for a build with none
/// (Regulation (EU) 2024/2847 Art 13(19)).
///
/// Fixed releases are published until that day; the release itself is
/// supported only while it is the newest.
// NOTE: Regulation (EU) 2024/2847 Art 13(19) has the end date specified by digital
// means, and options-root.schema.json leaves the top level open for it.
pub const SUPPORTED_UNTIL: &str = "supported_until";

/// Why the self-description cannot be built from the running federation.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DescribeError {
    /// A value the configuration holds is refused by the wire type.
    #[error("a declared value is refused by the OPTIONS {{base}}/ wire type")]
    Wire(#[from] WireError),
    /// A budget does not fit the integer the body carries it in.
    #[error("the {member} budget does not fit in a whole number of milliseconds")]
    Budget {
        /// The `timeout` member it would fill.
        member: &'static str,
    },
    /// An endpoint names a node the registry does not hold.
    #[error("the endpoint {endpoint} names a node the registry does not hold")]
    UnknownNode {
        /// The endpoint's id.
        endpoint: String,
    },
}

/// Builds the `OPTIONS {base}/` body of `federation` (§7a.2, N30), declaring
/// the stored-query registry offered when `registry` is `true` (§12.7, N44).
///
/// # Errors
///
/// Returns a [`DescribeError`] when a configured value is one the wire type
/// refuses, a budget does not fit in a `u64` of milliseconds, or an endpoint
/// names a node the registry does not hold.
pub fn describe(federation: &Federation, registry: bool) -> Result<OptionsRoot, DescribeError> {
    let gateway = GatewayDescription {
        id: federation.id().clone(),
        spec_version: SpecVersion::of_release(openehr_federation::FEDERATION_SPEC)?,
        aql: AqlBehaviour {
            fan_out: fans_out(federation.context().targeting()),
            extra: Extra::default(),
        },
        dedup: DedupPolicy {
            default: DedupDefault,
            modes: DedupModes::new(
                Federation::dedup_modes()
                    .iter()
                    .map(|mode| mode.name().to_owned())
                    .collect(),
            )?,
            request_header: Some(headers::DEDUP.to_owned()),
            extra: Extra::default(),
        },
        timeout: timeout(federation)?,
        completeness: completeness(federation.best_effort()),
        paging: paging(federation)?,
        aggregates: Aggregates {
            decomposable: federation
                .decomposable_aggregates()
                .iter()
                .map(|function| function.name().to_owned())
                .collect(),
            extra: Extra::default(),
        },
        // NOTE: §12.7 stored-query-fanout, N44: definition fan-out is declared only
        // beside the registry it distributes from.
        definition: DefinitionBehaviour::new(federation.fans_out_template_upload())
            .with_stored_query_registry(registry)?
            .with_stored_query_fan_out(registry && federation.fans_out_stored_queries())?,
        localization: localization(federation.localization())?,
        // NOTE: §13.1 jwks-discovery, N25, N30: the JWK Set location is declared
        // whenever keys are configured, and the member is absent otherwise.
        auth: federation.signing().map(|signing| AuthDescription {
            jwks_uri: Some(signing.jwks_uri.clone()),
            extra: Extra::default(),
        }),
        its_rest: its_rest(federation, registry)?,
        extra: consent(federation)?,
    };
    let mut extra = Extra::default();
    extra.insert_serialized(MANUFACTURER, &manufacturer::MANUFACTURER)?;
    extra.insert_serialized(
        SUPPORTED_UNTIL,
        &support::SUPPORTED_UNTIL.map(|end| end.to_string()),
    )?;
    Ok(OptionsRoot {
        federation: gateway,
        endpoints: members(federation)?,
        extra,
    })
}

/// The `consent` member of `federation`, present only where a Step-1 consent
/// pre-filter is configured: the pre-filter's mode, what a query does when it
/// cannot answer (N27a, §13.2.1), as `disclose` whether an answer names a
/// member it excludes as `consent-denied`, and as `emergency` whether its
/// exclusion applies to a request whose token declares an emergency purpose
/// (`apply`) or every member is asked and its node decides (`pass-to-node`).
///
/// A deployment with no pre-filter declares nothing, and N27 is its sole gate.
/// `disclose: false` tells a client that this deployment reports an excluded
/// member as one without the patient (Regulation (EU) 2025/327 Art 8), so a
/// `not-resolved` member there may be one the pre-filter excluded.
// NOTE: §7a.2 leaves the `federation` object open and names no consent member (N30
// requires none), so the member is our own design.
fn consent(federation: &Federation) -> Result<Extra, DescribeError> {
    #[derive(serde::Serialize)]
    struct Consent {
        prefilter: &'static str,
        on_unavailable: &'static str,
        disclose: bool,
        emergency: &'static str,
    }
    let mut extra = Extra::default();
    if let Some(prefilter) = federation.consent_prefilter() {
        let declared = Consent {
            prefilter: prefilter.mode(),
            on_unavailable: ON_UNAVAILABLE,
            disclose: federation.deployment_discloses_consent(),
            emergency: federation.consent_emergency().as_str(),
        };
        extra.insert_serialized(CONSENT, &declared)?;
    }
    Ok(extra)
}

/// Whether an undirected query fans out under `targeting`, the node
/// selection the deployment declared (§4.3, N4).
fn fans_out(targeting: Targeting) -> bool {
    matches!(targeting, Targeting::AskAll | Targeting::Localized)
}

/// The `localization` member: what the gateway does when its localizer does
/// not answer, `closed` by default and `ask-all` only where the deployment
/// declared it (§14.1, N4, N30), and, with a localizer configured, which
/// binding it is as `mode`, with where its audit messages go as `audit`.
fn localization(policy: &LocalizationPolicy) -> Result<Localization, DescribeError> {
    let mut extra = Extra::default();
    if let Some(mode) = policy.mode() {
        // NOTE: §14.1 asks only for `on_failure`, and the schema leaves the object
        // open: our own design, `mode` names the binding a client is localized by.
        extra.insert_serialized(LOCALIZATION_MODE, mode)?;
    }
    if let Some(audit) = policy.audit() {
        // NOTE: ITI TF-2 §3.55.5.1: where a localizer records its audit messages is
        // declared, so a deployment that turned them off says so.
        extra.insert_serialized(LOCALIZATION_AUDIT, audit)?;
    }
    Ok(Localization {
        on_failure: policy.on_failure().as_str().to_owned(),
        extra,
    })
}

/// The `timeout` member: the configured budget, under which a node past it
/// is abandoned and marked `time-out` (§11.5, N38), and, with a localizer, a
/// demographics step or a consent pre-filter that asks a service
/// configured, its own part of it.
fn timeout(federation: &Federation) -> Result<TimeoutPolicy, DescribeError> {
    let budget = federation.budget();
    let millis = |member: &'static str, duration: std::time::Duration| {
        u64::try_from(duration.as_millis()).map_err(|_overflow| DescribeError::Budget { member })
    };
    let mut extra = Extra::default();
    if federation.localization().localizer().is_some() {
        let localization = millis(LOCALIZATION_MS, federation.localization().timeout())?;
        // NOTE: §11.5 declares the budgets and its `timeout` object is open: our own
        // design, the localizer's budget is declared beside the two N38 names.
        extra.insert_serialized(LOCALIZATION_MS, &localization)?;
    }
    if let Some(step) = federation.demographics() {
        let demographics = millis(DEMOGRAPHICS_MS, step.timeout())?;
        extra.insert_serialized(DEMOGRAPHICS_MS, &demographics)?;
    }
    if let Some(prefilter) = federation
        .consent_prefilter()
        .and_then(ConsentPrefilter::budget)
    {
        let consent = millis(CONSENT_MS, prefilter)?;
        extra.insert_serialized(CONSENT_MS, &consent)?;
    }
    Ok(TimeoutPolicy {
        per_node_ms: millis("per_node_ms", budget.per_node())?,
        overall_ms: millis("overall_ms", budget.overall())?,
        // NOTE: §11.5, N38: the schema leaves `policy` open, and the value
        // names what the budget does to a late node.
        policy: "abandon-and-mark".to_owned(),
        extra,
    })
}

/// The `completeness` member: all-or-nothing by default, and best-effort
/// when offered, selected by `openEHR-federation-completeness: partial`
/// (§11.4, N37).
fn completeness(best_effort: bool) -> Completeness {
    if best_effort {
        Completeness::with_best_effort(OptIn {
            header: Some(headers::COMPLETENESS.to_owned()),
            value: Some(headers::COMPLETENESS_PARTIAL.to_owned()),
            extra: Extra::default(),
        })
    } else {
        Completeness::all_or_nothing_only()
    }
}

/// The `paging` member: the `OFFSET k > 0` strategy and, when it is
/// bounded, its bound on `k + n` (§11.6.2, N39), and the most rows read of
/// one node for a query with no `LIMIT` (§11.5). No cursor is offered, so
/// the strategy is never `cursor` (§11.6.4).
fn paging(federation: &Federation) -> Result<Paging, DescribeError> {
    let strategy = federation.offset_strategy();
    let mut extra = Extra::default();
    if let Some(window) = strategy.max_window() {
        extra.insert_serialized(MAX_WINDOW, &window.get())?;
    }
    if let Some(ceiling) = federation.max_node_rows() {
        extra.insert_serialized(MAX_NODE_ROWS, &ceiling.get())?;
    }
    Ok(Paging {
        offset_strategy: strategy.name().to_owned(),
        extra,
    })
}

/// The `its_rest` member: how each ITS-REST area of `federation` is served
/// (§7a.1, N30, N32), with stored queries held at the gateway when
/// `registry` is `true` (§12.7), their execution declared `501` when it is
/// not, and every other definition request routed to
/// one explicitly chosen node, or a template upload fanned out where offered
/// (§12.6, N43). The DEMOGRAPHIC area is never federated: it is `501`, or
/// routed to the one `demographic` endpoint the deployment declared, with or
/// without a targeting header naming it (§7a.1, §12.6, N32).
fn its_rest(federation: &Federation, registry: bool) -> Result<ItsRestAreas, DescribeError> {
    let (query, routed) = if registry && federation.fans_out_stored_queries() {
        (
            "federated: GET and POST {base}/v1/query/aql and GET and POST \
             {base}/v1/query/{name}[/{version}] fan out, each stored query served \
             from the gateway registry",
            "routed-single-node: a template request under {base}/v1/definition/template/ goes \
             to the one endpoint the targeting headers name, never merged; \
             stored queries at the gateway registry, which runs its own copy; \
             a stored-query PUT naming * or endpoints in the targeting headers is also \
             distributed to each, reported per node and never rolled back, and a GET of a \
             version naming them reports per node whether its copy matches",
        )
    } else if registry {
        (
            "federated: GET and POST {base}/v1/query/aql and GET and POST \
             {base}/v1/query/{name}[/{version}] fan out, each stored query served \
             from the gateway registry",
            "routed-single-node: a template request under {base}/v1/definition/template/ goes \
             to the one endpoint the targeting headers name, never merged; \
             stored queries at the gateway registry",
        )
    } else {
        (
            // NOTE: N32 has an unsupported area answered `501` and declared under N30;
            // routing would carry query_parameters the gateway cannot check (§5.4.2, N33).
            "federated: GET and POST {base}/v1/query/aql fan out; GET and POST \
             {base}/v1/query/{name}[/{version}] answer 501 Not Implemented, since no \
             stored-query registry is offered",
            "routed-single-node: a request under {base}/v1/definition/ goes to the one \
             endpoint the targeting headers name, never merged",
        )
    };
    Ok(ItsRestAreas {
        query: query.to_owned(),
        // NOTE: §7a.2 describes each area's behaviour in `its_rest`, so the
        // §12.5.1 step 2 binding is named with whose it is and how long it lives.
        ehr: "routed: a new EHR, POST {base}/v1/ehr or PUT {base}/v1/ehr/{ehr_id}, goes \
              only to the one endpoint the targeting headers name, and a PUT is refused \
              when another member holds its ehr_id; every other request under \
              {base}/v1/ehr/{ehr_id} goes to the one node that owns the ehr_id, found by \
              the targeting headers, the resolution bindings the same verified caller's \
              queries recorded, held in memory for federation.binding_ttl, the ehr_id \
              index, then for a read an ask-all probe; \
              a versioned write only when that node controls the version it amends; \
              GET {base}/v1/ehr?subject_id= resolves the subject and goes to the one \
              member that holds it, by its ehr_id"
            .to_owned(),
        definition: if federation.fans_out_template_upload() {
            format!(
                "{routed}; a template upload naming * or several endpoints in the \
                 targeting headers fans out to each, reported per node and never rolled back"
            )
        } else {
            routed.to_owned()
        },
        demographic: DemographicSupport::new(match federation.demographic_endpoint() {
            Some(endpoint) => format!(
                "routed-single-node: a request under {{base}}/v1/demographic/ goes to the \
                 declared endpoint {endpoint} alone, never federated, whether or not the \
                 targeting headers name it; a header naming another endpoint is refused \
                 with targeting-conflict"
            ),
            None => "unsupported: 501".to_owned(),
        })?,
        extra: Extra::default(),
    })
}

/// The member endpoints of `snapshot`, in endpoint-id order (§7a.2, N30).
fn members(federation: &Federation) -> Result<Vec<MemberEndpoint>, DescribeError> {
    let snapshot = federation.snapshot();
    snapshot
        .endpoints()
        .map(|endpoint| member(snapshot, endpoint, federation.latency(endpoint.id())))
        .collect()
}

/// One member endpoint: its id, managing organisation (N20) and membership
/// status, its node, the node's `system_id`, product and version where the
/// registry holds them, and the `latency` of its recent requests where the
/// gateway sent it any.
fn member(
    snapshot: &RegistrySnapshot,
    endpoint: &Endpoint,
    latency: Option<Percentiles>,
) -> Result<MemberEndpoint, DescribeError> {
    let node = snapshot
        .node(endpoint.node())
        .ok_or_else(|| DescribeError::UnknownNode {
            endpoint: endpoint.id().as_str().to_owned(),
        })?;
    // NOTE: §7a.2 leaves the membership vocabulary open; `active` is its
    // conventional value and `suspended` the registry's own.
    let mut extra = Extra::default();
    if let Some(latency) = latency {
        extra.insert_serialized(LATENCY_MS_P95, &latency.p95)?;
    }
    let status = match endpoint.status() {
        EndpointStatus::Active => "active",
        EndpointStatus::Suspended => "suspended",
    };
    Ok(MemberEndpoint {
        id: EndpointId::new(endpoint.id().as_str())?,
        organisation: endpoint.managing_organisation().as_str().to_owned(),
        status: MembershipStatus::new(status)?,
        node_id: Some(endpoint.node().as_str().to_owned()),
        system_id: Some(node.system_id().as_str().to_owned()),
        product: node.product().map(str::to_owned),
        version: node.version().map(str::to_owned),
        latency_ms_p50: latency.map(|latency| latency.p50),
        url: None,
        extra,
    })
}
