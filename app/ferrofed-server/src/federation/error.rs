// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a federation cannot be built from the settings.

use std::path::PathBuf;

use ferrofed_engine::dispatch::SetupError;
use ferrofed_identity::dev::DevCrossRefError;
#[cfg(feature = "binding-ihe")]
use ferrofed_identity::directory::error::FhirFormError;
#[cfg(feature = "binding-nl")]
use ferrofed_identity::mitz::MitzConfigError;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrofed_identity::patient::PatientRefError;
#[cfg(feature = "binding-ihe")]
use ferrofed_identity::pdqm::PdqmConfigError;
#[cfg(feature = "binding-ihe")]
use ferrofed_identity::pixm::PixmConfigError;
#[cfg(any(feature = "binding-ihe", feature = "binding-nl"))]
use ferrofed_registry::error::IdError;
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::EndpointId;

use crate::binding::RoleConflict;
#[cfg(feature = "binding-ihe")]
use crate::binding::ihe::mcsd::registry::DirectoryFailure;
use crate::facade::options::DescribeError;
use crate::localization;
use crate::service::{GrantRefused, TlsRefused};

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
    /// The registry could not be read from the directory of `[registry.mcsd]`
    /// (§15.1, §15.2, N19, N20).
    #[error("the registry could not be read from the care services directory")]
    #[cfg(feature = "binding-ihe")]
    Directory(#[source] Box<DirectoryFailure>),
    /// The `[dev]` table is set but no registry is, neither
    /// `registry.document` nor `[registry.mcsd]`, so its rows name members
    /// that do not exist.
    #[error(
        "the [dev] cross-reference needs a registry, registry.document or [registry.mcsd], whose members its rows name"
    )]
    DevWithoutRegistry,
    /// The `[dev]` table does not read as the static cross-reference.
    #[error("the [dev] cross-reference is not valid")]
    DevTable(#[source] crate::config::error::Error),
    /// The static cross-reference refuses its rows or the profile.
    #[error("the [dev] cross-reference cannot be enabled")]
    DevCrossRef(#[source] DevCrossRefError),
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
    /// `[pixm]` is set but no registry is, neither `registry.document` nor
    /// `[registry.mcsd]`, so it names members that do not exist.
    #[error(
        "the [pixm] resolver needs a registry, registry.document or [registry.mcsd], whose members it names"
    )]
    #[cfg(feature = "binding-ihe")]
    PixmWithoutRegistry,
    /// More than one configured section fills a role exactly one may: at
    /// most one resolver and one localizer of a binding's own are active (no
    /// specification governs this: our own design), and at most one consent
    /// pre-filter (N27a).
    #[error(transparent)]
    Conflict(#[from] RoleConflict),
    /// A key of `[nl_gf.mitz.holders]` or `[nl_gf.nvi.custodians]` is not a
    /// node id.
    #[error("{key} is not a node id")]
    #[cfg(feature = "binding-nl")]
    MitzMember {
        /// The key, a member or a URA and never a patient value.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// A `nl_gf.mitz.namespaces` entry is empty.
    #[error("nl_gf.mitz.namespaces has an empty namespace")]
    #[cfg(feature = "binding-nl")]
    MitzNamespace(#[source] PatientRefError),
    /// The Mitz consent pre-filter refuses its configuration.
    #[error("the [nl_gf.mitz] consent pre-filter cannot be enabled")]
    #[cfg(feature = "binding-nl")]
    Mitz(#[source] MitzConfigError),
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
    /// A `[pixm]` member key is not a node id.
    #[error("pixm.manager[{manager}].members.{key:?} is not a node id")]
    #[cfg(feature = "binding-ihe")]
    PixmMember {
        /// The Manager's index.
        manager: usize,
        /// The key that was given.
        key: String,
        /// What the id rules reported.
        #[source]
        source: IdError,
    },
    /// A `[pixm.namespaces]` key is not a namespace.
    #[error("pixm.namespaces has an empty namespace")]
    #[cfg(feature = "binding-ihe")]
    PixmNamespace(#[source] PatientRefError),
    /// The PIXm resolver refuses its Managers or members.
    #[error("the [pixm] resolver cannot be enabled")]
    #[cfg(feature = "binding-ihe")]
    Pixm(#[source] PixmConfigError),
    /// `[pdqm]` is set but no cross-reference resolver is, so the master
    /// identity it finds could never be resolved (Annex A §A.2, §5.2).
    #[error(
        "the [pdqm] demographics step needs a cross-reference resolver, {}, to resolve the master identity it finds (Annex A §A.2, §5.2)",
        crate::binding::resolver_list()
    )]
    #[cfg(feature = "binding-ihe")]
    PdqmWithoutResolver,
    /// A `[pdqm.namespaces]` key is not a namespace.
    #[error("pdqm.namespaces has an empty namespace")]
    #[cfg(feature = "binding-ihe")]
    PdqmNamespace(#[source] PatientRefError),
    /// The PDQm demographics step refuses its Supplier or its domains.
    #[error("the [pdqm] demographics step cannot be enabled")]
    #[cfg(feature = "binding-ihe")]
    Pdqm(#[source] PdqmConfigError),
    /// The audit trail of the PIXm, mCSD and PMIR transactions cannot start
    /// (`[audit]`).
    #[error("the [audit] trail cannot start")]
    #[cfg(feature = "binding-ihe")]
    Audit(#[source] crate::binding::ihe::audit::AuditTrailError),
    /// The localizer of `node_selection = "localized"` cannot be set up
    /// (§14.1, N4).
    #[error("the localizer cannot be set up")]
    Localization(#[source] localization::LocalizationError),
    /// An OAuth 2.0 or FAPI 2.0 grant that cannot be used: one in a PIX
    /// Manager's credentials, or a node's with no `[signing]` key (§13.1,
    /// N25).
    #[error("{section} names a grant it cannot use: only a node takes one, with [signing]")]
    Grant {
        /// The credentials section.
        section: String,
    },
    /// The TLS material of an identity, localization, consent or audit
    /// service does not read.
    #[error("the TLS material of a service cannot be used")]
    Tls(#[source] TlsRefused),
    /// The HTTP client of an endpoint's Nuts grant could not be built.
    #[error("the HTTP client of the Nuts grant of {section} could not be built")]
    #[cfg(feature = "binding-nl")]
    NutsClient {
        /// The credentials section.
        section: String,
        /// Why the client could not be built.
        #[source]
        source: reqwest::Error,
    },
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
    /// A node the gateway presents its TLS client certificate to is not
    /// reached over `https` (RFC 8705).
    #[error(
        "endpoint {endpoint} is not an https URL, and credentials.{endpoint} presents a TLS client certificate to it, which needs TLS under every profile"
    )]
    ClientCertificateOverHttp {
        /// The endpoint.
        endpoint: EndpointId,
    },
    /// The HTTP client of a node reached with its own TLS material could
    /// not be built.
    #[error("the HTTP client of endpoint {endpoint} could not be built")]
    NodeTransport {
        /// The endpoint.
        endpoint: EndpointId,
        /// Why it could not be built; it carries no key.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl From<GrantRefused> for FederationError {
    fn from(refused: GrantRefused) -> Self {
        Self::Grant {
            section: refused.section,
        }
    }
}
