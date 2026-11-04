// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a federation cannot be built from the settings.

use std::path::PathBuf;

use ferrofed_engine::dispatch::SetupError;
#[cfg(feature = "binding-ihe")]
use ferrofed_identity::ihe::mcsd::error::FhirFormError;
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::EndpointId;

use crate::binding::RoleConflict;
use crate::federation::describe::DescribeError;

/// A federation that cannot be built from the settings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FederationError {
    /// The registry document could not be read or refused to load.
    #[error("the registry document {} could not be loaded", path.display())]
    Registry {
        /// The document named by `registry.document`.
        path: PathBuf,
        /// What the registry reported.
        #[source]
        source: Box<LoadError>,
    },
    /// The registry document in FHIR form could not be read or refused to
    /// load (N19, N20, §15.2).
    #[error("the registry document {} could not be loaded", path.display())]
    #[cfg(feature = "binding-ihe")]
    FhirRegistry {
        /// The document named by `registry.document`.
        path: PathBuf,
        /// What the FHIR form reported.
        #[source]
        source: Box<FhirFormError>,
    },
    /// The members the operator console manages could not be joined to the
    /// registry document (`[[registry.members]]`).
    #[error("the members the operator console manages could not be joined to the registry")]
    ConsoleMembers(#[source] Box<LoadError>),
    /// `federation.demographic_endpoint` is set but no registry is, neither
    /// `registry.document` nor `[registry.mcsd]`, so it names an endpoint
    /// that does not exist.
    #[error(
        "federation.demographic_endpoint needs a registry, registry.document or [registry.mcsd], whose endpoint it names"
    )]
    DemographicWithoutRegistry,
    /// `federation.demographic_endpoint` names no endpoint of the registry
    /// (§7a.1, §12.6, N32).
    #[error(
        "federation.demographic_endpoint names {endpoint}, which is no endpoint of the registry"
    )]
    DemographicEndpointUnknown {
        /// The endpoint id that was given.
        endpoint: EndpointId,
    },
    /// An `[auth.issuer.patient]` binding is set but no registry is, neither
    /// `registry.document` nor `[registry.mcsd]`, so it names an endpoint
    /// that does not exist.
    #[error(
        "{key} needs a registry, registry.document or [registry.mcsd], whose endpoint it names"
    )]
    PatientWithoutRegistry {
        /// The binding's key.
        key: String,
    },
    /// An `[auth.issuer.patient]` binding names no endpoint of the registry.
    #[error("{key} names {endpoint}, which is no endpoint of the registry")]
    PatientEndpointUnknown {
        /// The binding's key.
        key: String,
        /// The endpoint id that was given.
        endpoint: EndpointId,
    },
    /// A `[[fhir.receive]]` names a member the registry does not hold.
    #[error(
        "fhir.receive names {member} as the member of the category {category}, which is no node of the registry"
    )]
    ReceivingMemberUnknown {
        /// The category's code.
        category: &'static str,
        /// The member as given.
        member: String,
    },
    /// A member's own exchange-format endpoint names a code that is no
    /// priority category of Regulation (EU) 2025/327 Art 14(1).
    #[error(
        "endpoint {endpoint} serves the category {category} natively, which is no priority category code"
    )]
    NativeCategoryUnknown {
        /// The native endpoint.
        endpoint: EndpointId,
        /// The code as given.
        category: String,
    },
    /// A member's own endpoint serves a category in a profile other than
    /// the one the face asks every document of it in.
    #[error(
        "endpoint {endpoint} serves the category {category} natively and does not declare the profile {profile}"
    )]
    NativeProfileMissing {
        /// The native endpoint.
        endpoint: EndpointId,
        /// The category's code.
        category: String,
        /// The profile the face asks for.
        profile: &'static str,
    },
    /// A `[[fhir.serve]]` names a member the registry does not hold.
    #[error(
        "fhir.serve names {member} as a member that serves the category {category}, which is no node of the registry"
    )]
    ServingMemberUnknown {
        /// The category's code.
        category: &'static str,
        /// The member as given.
        member: String,
    },
    /// A `[[fhir.serve]]` names a member with no endpoint of its own that
    /// serves the category, so the face could not ask it.
    #[error(
        "fhir.serve names {member} for the category {category}, and the registry declares no hl7-fhir-rest endpoint of it serving that category"
    )]
    ServingWithoutEndpoint {
        /// The category's code.
        category: &'static str,
        /// The member.
        member: String,
    },
    /// A category is served but no resolver is configured, so a patient
    /// cannot be resolved to the serving members' `ehr_id`s (§5.2).
    #[error(
        "[[fhir.serve]] needs a cross-reference resolver, {}, to resolve a patient at the members that serve prescriptions (§5.2)",
        crate::binding::resolver_list()
    )]
    ServingWithoutResolver,
    /// A category is received but no resolver is configured, so a received
    /// document's patient cannot be resolved to its member's `ehr_id` (§5.2).
    #[error(
        "[[fhir.receive]] needs a cross-reference resolver, {}, to resolve a received document's patient at its member (§5.2)",
        crate::binding::resolver_list()
    )]
    ReceivingWithoutResolver,
    /// An `[auth.issuer.patient]` binding is set but no resolver is, so the
    /// token's `ehrId` cannot be resolved to the members' `ehr_id`s (§5.2).
    #[error(
        "{key} needs a cross-reference resolver, {}, to resolve the token's ehrId at every member (§5.2)",
        crate::binding::resolver_list()
    )]
    PatientWithoutResolver {
        /// The binding's key.
        key: String,
    },
    /// More than one configured section fills a role exactly one may: at
    /// most one resolver and one localizer of a binding's own are active (no
    /// specification governs this: our own design), and at most one consent
    /// pre-filter (N27a).
    #[error(transparent)]
    Conflict(#[from] RoleConflict),
    /// A registry is configured, but `federation.node_selection` is not: how
    /// an undirected patient query finds its nodes is a deployment decision,
    /// declared and never defaulted (§4.3, N4).
    #[error(
        "set federation.node_selection when a registry, registry.document or [registry.mcsd], is set: \"ask-all\" asks every member's cross-reference (§4.3, N4)"
    )]
    NodeSelectionUndeclared,
    /// A registry is configured, but `federation.id` is not: the
    /// `OPTIONS {base}/` body names the federation (§7a.2, N30), and the
    /// identifier is the deployment's to choose, never defaulted.
    #[error(
        "set federation.id when a registry, registry.document or [registry.mcsd], is set: the OPTIONS {{base}}/ self-description names the federation (§7a.2, N30)"
    )]
    IdUndeclared,
    /// The `OPTIONS {base}/` self-description cannot be built from the
    /// configuration (§7a.2, N30).
    #[error("the OPTIONS {{base}}/ self-description cannot be built")]
    Describe(#[source] DescribeError),
    /// The `[audit]` trail the access records and the IHE binding's BALP
    /// records go through cannot start.
    #[error("the [audit] trail cannot start")]
    Audit(#[source] crate::audit::AuditTrailError),
    /// `[audit]` sends the access records nowhere, which every profile but
    /// development refuses (Regulation (EU) 2025/327 Annex II 3.2).
    #[error(
        "no access log: every access to patient data is recorded outside the development profile; set [audit] destination = \"repository\""
    )]
    AccessLogUnavailable,
    /// `[access_log.retention.origins]` names an endpoint the registry does
    /// not hold, so no record would ever be kept for it.
    #[error("access_log.retention.origins names {endpoint}, which is no endpoint of the registry")]
    RetentionEndpointUnknown {
        /// The endpoint id that was given.
        endpoint: String,
    },
    /// `[access_log.labels.origins]` names an endpoint the registry does not
    /// hold, so no record would ever carry its labels.
    #[error("access_log.labels.origins names {endpoint}, which is no endpoint of the registry")]
    LabelEndpointUnknown {
        /// The endpoint id that was given.
        endpoint: String,
    },
    /// The localizer of `node_selection = "localized"` cannot be set up
    /// (§14.1, N4).
    #[error("the localizer cannot be set up")]
    Localization(#[source] crate::binding::seam::LocalizationError),
    /// A registry is configured, from `registry.document` or from
    /// `[registry.mcsd]`, but `[signing]` is not: every request to a node
    /// conveys the caller's identity, signed with that key (§13.1, N24, N25).
    #[error(
        "set [signing] when a registry is configured, by registry.document or [registry.mcsd]: every request to a node carries the caller's identity, signed with that key (§13.1, N24)"
    )]
    Unsigned,
    /// The node clients could not be built.
    #[error("the node clients could not be built")]
    Clients(#[source] SetupError),
    /// The HTTP client every node client shares could not be built.
    #[error("the HTTP client for the nodes could not be built")]
    Transport(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// The onward clients of the nodes cannot be built.
    #[error(transparent)]
    Onward(#[from] crate::onward::OnwardError),
    /// A public document a binding serves would sit inside the ITS-REST
    /// surface, `{base}/v1/`, where the client authentication gate reads
    /// every request.
    #[error("the public document at {path} would sit inside the ITS-REST surface {surface}")]
    DocumentInSurface {
        /// The document's path.
        path: String,
        /// The surface's path prefix.
        surface: String,
    },
    /// Two public documents would be served at one path.
    #[error("two public documents would be served at {path}")]
    DocumentTwice {
        /// The path both claim.
        path: String,
    },
    /// A binding cannot build what it adds to the federation.
    #[error(transparent)]
    Binding(#[from] crate::binding::error::BindingError),
}
