// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The refusals of `[federation]`, `[stored_queries]` and `[registry]`.

use ferrofed_registry::error::IdError;

use crate::config::stored_queries::Backend;

/// A `[federation]`, `[stored_queries]` or `[registry]` value the server
/// refuses to start on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FederationFault {
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
    /// `federation.demographic_endpoint` is not an endpoint id.
    #[error("federation.demographic_endpoint is not an endpoint id")]
    DemographicEndpoint {
        /// What the registry's endpoint id rule reported.
        #[source]
        source: IdError,
    },
    /// Both a registry document and a directory are configured, and the
    /// registry has one source.
    #[error("set registry.document or [registry.mcsd], not both: the registry has one source")]
    #[cfg(feature = "binding-ihe")]
    TwoRegistrySources,
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
    /// `[federation.consent] emergency = "pass-to-node"` with no emergency purpose declared.
    #[error("[federation.consent] pass-to-node needs an [[access_log.emergency_purpose]]")]
    EmergencyWithoutPurpose,
}
