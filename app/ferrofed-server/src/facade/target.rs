// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node set a `FROM ENDPOINT` or `ORGANISATION` directive selects,
//! through the registry (§8.1, §8.4.1, N11, N19, N20).
//!
//! The directive names stable registry identifiers, never URLs (§8.1, N19).
//! An `ENDPOINT` list is the node set exactly; an `ORGANISATION` list stands
//! for every endpoint each organisation manages (N20). An identifier the
//! registry does not know is a `400` (§8.4.1), whose message points at the
//! identifier by its place in the list and never quotes it, since a client
//! may write anything there (§5.4.3).

use std::collections::BTreeSet;
use std::ops::Range;

use ferrofed_registry::id::{EndpointId, OrganisationId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use openehr_query::federation::{Directive, DirectiveKind};

/// A directive that names what the registry does not know (§8.4.1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TargetError {
    /// An `ENDPOINT` identifier is no registry endpoint.
    #[error(
        "identifier {position} of the endpoint directive{} is not an endpoint the registry knows (§8.4.1, N19)",
        At(.at)
    )]
    UnknownEndpoint {
        /// The identifier's place in the list, from 1.
        position: usize,
        /// Where the directive was written.
        at: Option<Range<usize>>,
    },
    /// An `ORGANISATION` identifier is no registry organisation.
    #[error(
        "identifier {position} of the organisation directive{} is not an organisation the registry knows (§8.1, §8.4.1, N20)",
        At(.at)
    )]
    UnknownOrganisation {
        /// The identifier's place in the list, from 1.
        position: usize,
        /// Where the directive was written.
        at: Option<Range<usize>>,
    },
}

/// The endpoints `directive` selects in `snapshot`, in `endpoint_id` order.
///
/// The set may be empty: a known organisation that manages no endpoint
/// selects none, and the request then has no destination (§11.2).
///
/// # Errors
/// [`TargetError::UnknownEndpoint`] or [`TargetError::UnknownOrganisation`]
/// for the first identifier the registry does not know, an identifier that
/// is not of a registry identifier's form included.
pub fn selected(
    snapshot: &RegistrySnapshot,
    directive: &Directive,
) -> Result<BTreeSet<EndpointId>, TargetError> {
    let at = directive.span.bytes();
    // NOTE: §8.4.1, an identifier not of a registry id's form names nothing the
    // registry knows, so its parse failure is the unknown-identifier answer.
    let mut endpoints = BTreeSet::new();
    for (index, id) in directive.ids.iter().enumerate() {
        let position = index.saturating_add(1);
        match directive.kind {
            DirectiveKind::Endpoint => {
                let endpoint = EndpointId::new(id.as_str())
                    .ok()
                    .filter(|endpoint| snapshot.endpoint(endpoint).is_some())
                    .ok_or_else(|| TargetError::UnknownEndpoint {
                        position,
                        at: at.clone(),
                    })?;
                endpoints.insert(endpoint);
            }
            DirectiveKind::Organisation => {
                let organisation = OrganisationId::new(id.as_str())
                    .ok()
                    .filter(|organisation| snapshot.organisation(organisation).is_some())
                    .ok_or_else(|| TargetError::UnknownOrganisation {
                        position,
                        at: at.clone(),
                    })?;
                endpoints.extend(
                    snapshot
                        .endpoints_managed_by(&organisation)
                        .map(|endpoint| endpoint.id().clone()),
                );
            }
        }
    }
    Ok(endpoints)
}

/// The ` (bytes a..b)` suffix of a message, or nothing for an unknown span.
struct At<'a>(&'a Option<Range<usize>>);

impl std::fmt::Display for At<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(bytes) => write!(f, " (bytes {}..{})", bytes.start, bytes.end),
            None => Ok(()),
        }
    }
}
