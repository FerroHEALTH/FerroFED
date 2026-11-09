// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The class a refused reload of the federation is logged under. No
//! specification governs the classes: our own design.

use ferrofed_engine::dispatch::SetupError;
use ferrofed_registry::error::LoadError;

use crate::binding::error::BindingError;
use crate::binding::seam::ProvideError;
use crate::binding::{self, Role, RoleConflict};
use crate::federation::error::FederationError;
use crate::onward::OnwardError;

/// The class a refused federation is logged under: the core's own, or the
/// class the binding whose error it is names.
pub(super) fn federation(error: &FederationError) -> &'static str {
    match error {
        FederationError::Registry { source, .. } => match **source {
            LoadError::Read { .. } => "registry-unreadable",
            _ => "registry-invalid",
        },
        FederationError::Conflict(RoleConflict { role, .. }) => match role {
            Role::Resolver => "resolvers",
            Role::ConsentPrefilter => "consent-prefilter",
            Role::Localizer => "localization",
            _ => "bindings",
        },
        FederationError::Localization(_) => "localization",
        FederationError::NodeSelectionUndeclared | FederationError::IdUndeclared => "federation",
        FederationError::DemographicWithoutRegistry
        | FederationError::DemographicEndpointUnknown { .. } => "demographic-endpoint",
        FederationError::PatientWithoutRegistry { .. }
        | FederationError::PatientEndpointUnknown { .. }
        | FederationError::PatientWithoutResolver { .. } => "patient-binding",
        FederationError::Describe(_) => "self-description",
        FederationError::Clients(SetupError::UnknownEndpoint { .. })
        | FederationError::Binding(BindingError::Grant { .. })
        | FederationError::Onward(
            OnwardError::Grant { .. } | OnwardError::Provide(ProvideError::Unclaimed { .. }),
        ) => "credentials",
        #[cfg(feature = "binding-nl")]
        FederationError::Onward(OnwardError::Provide(ProvideError::NutsClient { .. })) => {
            "http-client"
        }
        FederationError::Clients(_) => "node-clients",
        FederationError::Transport(_) => "http-client",
        FederationError::Unsigned => "signing",
        FederationError::RetentionEndpointUnknown { .. } => "access-log",
        FederationError::Audit(_) | FederationError::Binding(BindingError::Audit(_)) => "audit",
        #[cfg(feature = "binding-ihe")]
        FederationError::FhirRegistry { source, .. } => match **source {
            ferrofed_identity::ihe::mcsd::error::FhirFormError::Read { .. } => {
                "registry-unreadable"
            }
            _ => "registry-invalid",
        },
        FederationError::Binding(error) => binding::class(error).unwrap_or("binding"),
        _ => "binding",
    }
}
