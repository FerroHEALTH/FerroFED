// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Why a federation cannot be built from the settings.

use std::path::PathBuf;

use ferrofed_engine::dispatch::SetupError;
use ferrofed_identity::dev::DevCrossRefError;
use ferrofed_identity::directory::error::FhirFormError;
use ferrofed_identity::patient::PatientRefError;
use ferrofed_identity::pixm::PixmConfigError;
use ferrofed_registry::error::{IdError, LoadError};
use ferrofed_registry::id::EndpointId;

use crate::directory::DirectoryFailure;
use crate::facade::options::DescribeError;
use crate::localization;

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
    /// `[pixm]` is set but no registry is, neither `registry.document` nor
    /// `[registry.mcsd]`, so it names members that do not exist.
    #[error(
        "the [pixm] resolver needs a registry, registry.document or [registry.mcsd], whose members it names"
    )]
    PixmWithoutRegistry,
    /// Both `[dev]` and `[pixm]` are set, and exactly one resolver is active
    /// (no specification governs this: our own design).
    #[error("set one resolver: [dev] and [pixm] are both configured")]
    TwoResolvers,
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
    PixmNamespace(#[source] PatientRefError),
    /// The PIXm resolver refuses its Managers or members.
    #[error("the [pixm] resolver cannot be enabled")]
    Pixm(#[source] PixmConfigError),
    /// The localizer of `node_selection = "localized"` cannot be set up
    /// (§14.1, N4).
    #[error("the localizer cannot be set up")]
    Localization(#[source] localization::LocalizationError),
    /// An OAuth 2.0 grant that cannot be used: one in a PIX Manager's
    /// credentials, or a node's with no `[signing]` key (§13.1, N25).
    #[error("{section} names an oauth2 grant it cannot use: only a node takes one, with [signing]")]
    Grant {
        /// The credentials section.
        section: String,
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
}
