// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The adapters of the Dutch binding (Annex B) over `nl_generic_functions`:
//! the NVI localizer ([`nvi`]) and the Mitz consent pre-filter ([`mitz`]),
//! and the custodian map both read from the registry.
//!
//! No specification governs the grouping: our own design.

pub mod mitz;
pub mod nvi;

use std::collections::{BTreeMap, BTreeSet};

use ferrofed_registry::id::{NodeId, OrganisationId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use nl_generic_functions::identification::Ura;
use nl_generic_functions::lrza::{self, LrzaError};
use thiserror::Error;

/// A member organisation the registry read from a directory carries URA
/// identifiers the LRZa rules refuse (Annex B §B.2).
///
/// It names the organisation by its registry id, never a patient value.
#[derive(Debug, Error)]
#[error("organisation {organisation} carries URA identifiers the LRZa rules refuse")]
pub struct DirectoryUraError {
    /// The organisation's registry id.
    pub organisation: OrganisationId,
    /// What the LRZa rules reported.
    #[source]
    pub source: LrzaError,
}

/// The custodian map the registry gives: each URA its member organisations
/// carry, by the LRZa rules (Annex B §B.2), mapped to the members those
/// organisations operate. It is empty for a registry no directory gave.
pub(crate) fn directory_custodians(
    registry: &RegistrySnapshot,
) -> Result<BTreeMap<Ura, BTreeSet<NodeId>>, DirectoryUraError> {
    let mut derived: BTreeMap<Ura, BTreeSet<NodeId>> = BTreeMap::new();
    for node in registry.nodes() {
        let Some(organisation) = registry.organisation(node.organisation()) else {
            continue;
        };
        let identifiers = organisation
            .identifiers()
            .iter()
            .map(|identifier| (Some(identifier.system()), Some(identifier.value())));
        let ura = lrza::ura_in(identifiers).map_err(|source| DirectoryUraError {
            organisation: organisation.id().clone(),
            source,
        })?;
        // NOTE: Annex B §B.2: an organisation with no URA is not a top-level care
        // provider, so it names no custodian and its members need one elsewhere.
        if let Some(ura) = ura {
            derived.entry(ura).or_default().insert(node.id().clone());
        }
    }
    Ok(derived)
}
