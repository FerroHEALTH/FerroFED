// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node set a request's targeting selects, through the registry (§8.1,
//! §8.4, §8.4.1, N11, N19, N20, N35).
//!
//! A request targets nodes in the AQL, with a `FROM ENDPOINT` or
//! `ORGANISATION` directive, or beside it, with the
//! `openEHR-federation-endpoint` or `openEHR-federation-organisation` request
//! header, whose value is a comma-separated list (§8.4). Both mechanisms are
//! accepted and are equivalent. Each names stable registry identifiers, never
//! URLs (§8.1, N19): an endpoint list is the node set exactly, and an
//! organisation list stands for every endpoint each organisation manages
//! (N20). No query parameter targets anything (§8.4).
//!
//! An identifier the registry does not know is a `400` under either
//! mechanism (§8.4.1), whose message points at the identifier by its place in
//! its list and never quotes it, since a client may write anything there
//! (§5.4.3). When more than one mechanism appears, the node sets they select
//! must be identical: the request then proceeds, and otherwise it is a `400`
//! naming both sets, never a merge and never a silent preference (§8.4.1,
//! N35).

use std::collections::BTreeSet;
use std::fmt;
use std::ops::Range;

use ferrofed_registry::id::{EndpointId, OrganisationId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::HeaderMap;
use openehr_federation::headers;
use openehr_query::federation::{Directive, DirectiveKind};

use crate::error::Code;

/// One of the four ways a request names its node set (§8.1, §8.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Mechanism {
    /// The AQL `FROM ENDPOINT` directive (§8.1).
    EndpointDirective,
    /// The AQL `ORGANISATION` directive (§8.1).
    OrganisationDirective,
    /// The `openEHR-federation-endpoint` request header (§8.4).
    EndpointHeader,
    /// The `openEHR-federation-organisation` request header (§8.4).
    OrganisationHeader,
}

impl fmt::Display for Mechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EndpointDirective => f.write_str("the endpoint directive"),
            Self::OrganisationDirective => f.write_str("the organisation directive"),
            Self::EndpointHeader => write!(f, "the {} header", headers::ENDPOINT),
            Self::OrganisationHeader => write!(f, "the {} header", headers::ORGANISATION),
        }
    }
}

/// The node set one mechanism of a request selects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// The mechanism that named the set.
    pub by: Mechanism,
    /// The endpoints it selects, in `endpoint_id` order.
    pub endpoints: BTreeSet<EndpointId>,
}

/// A request whose targeting cannot be answered (§8.4, §8.4.1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TargetError {
    /// An endpoint identifier is no registry endpoint.
    #[error(
        "identifier {position} of {by}{} is not an endpoint the registry knows (§8.4.1, N19)",
        At(.at)
    )]
    UnknownEndpoint {
        /// The directive or the header that named it.
        by: Mechanism,
        /// The identifier's place in the list, from 1.
        position: usize,
        /// Where the directive was written; a header has no place in the
        /// query.
        at: Option<Range<usize>>,
    },
    /// An organisation identifier is no registry organisation.
    #[error(
        "identifier {position} of {by}{} is not an organisation the registry knows (§8.1, §8.4.1, N20)",
        At(.at)
    )]
    UnknownOrganisation {
        /// The directive or the header that named it.
        by: Mechanism,
        /// The identifier's place in the list, from 1.
        position: usize,
        /// Where the directive was written; a header has no place in the
        /// query.
        at: Option<Range<usize>>,
    },
    /// A targeting header is given with no identifier in it.
    #[error("{0} is given and names no identifier (§8.4)")]
    Empty(Mechanism),
    /// Two mechanisms of one request select different node sets (§8.4.1,
    /// N35).
    #[error(
        "{} selects {} and {} selects {}, so the request names two node sets and neither is chosen (§8.4.1, N35)",
        .first.by,
        Listed(&.first.endpoints),
        .second.by,
        Listed(&.second.endpoints)
    )]
    Conflict {
        /// The set the first mechanism selects, in the order directive,
        /// endpoint header, organisation header.
        first: Selected,
        /// The first set that differs from it.
        second: Selected,
    },
}

impl TargetError {
    /// The stable code the error body names for this error.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::UnknownEndpoint { .. }
            | Self::Empty(Mechanism::EndpointDirective | Mechanism::EndpointHeader) => {
                Code::EndpointUnknown
            }
            Self::UnknownOrganisation { .. }
            | Self::Empty(Mechanism::OrganisationDirective | Mechanism::OrganisationHeader) => {
                Code::OrganisationUnknown
            }
            Self::Conflict { .. } => Code::TargetingConflict,
        }
    }
}

/// The node set the request's targeting selects, or `None` when neither
/// `directive` nor a targeting header in `headers` names one.
///
/// Every mechanism present is read through `snapshot`: the directive first,
/// then the endpoint header, then the organisation header. The set may be
/// empty: a known organisation that manages no endpoint selects none, and the
/// request then has no destination (§11.2).
///
/// # Errors
///
/// The first [`TargetError::UnknownEndpoint`] or
/// [`TargetError::UnknownOrganisation`] of any mechanism, an identifier that
/// is not of a registry identifier's form included; [`TargetError::Empty`]
/// for a header with no identifier; and [`TargetError::Conflict`] when two
/// mechanisms select different sets.
pub fn requested(
    snapshot: &RegistrySnapshot,
    directive: Option<&Directive>,
    headers: &HeaderMap,
) -> Result<Option<BTreeSet<EndpointId>>, TargetError> {
    let mut named = Vec::new();
    if let Some(directive) = directive {
        let by = match directive.kind {
            DirectiveKind::Endpoint => Mechanism::EndpointDirective,
            DirectiveKind::Organisation => Mechanism::OrganisationDirective,
        };
        let ids: Vec<Option<&str>> = directive.ids.iter().map(|id| Some(id.as_str())).collect();
        named.push(Selected {
            by,
            endpoints: expanded(snapshot, by, &ids, directive.span.bytes().as_ref())?,
        });
    }
    for (name, by) in [
        (headers::ENDPOINT, Mechanism::EndpointHeader),
        (headers::ORGANISATION, Mechanism::OrganisationHeader),
    ] {
        if let Some(ids) = listed(headers, name) {
            named.push(Selected {
                by,
                endpoints: expanded(snapshot, by, &ids, None)?,
            });
        }
    }
    let mut named = named.into_iter();
    let Some(first) = named.next() else {
        return Ok(None);
    };
    if let Some(second) = named.find(|other| other.endpoints != first.endpoints) {
        return Err(TargetError::Conflict { first, second });
    }
    Ok(Some(first.endpoints))
}

/// The identifiers of every field line of the header `name` in `headers`, in
/// order, or `None` when the header is absent.
///
/// Each line is a comma-separated list whose empty elements are ignored (RFC
/// 9110 §5.6.1). A line that is not visible ASCII stands as one identifier,
/// `None`, since it can name no registry identifier.
fn listed<'h>(headers: &'h HeaderMap, name: &str) -> Option<Vec<Option<&'h str>>> {
    let mut lines = headers.get_all(name).iter().peekable();
    lines.peek()?;
    let mut ids = Vec::new();
    for line in lines {
        match line.to_str() {
            Ok(text) => ids.extend(
                text.split(',')
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map(Some),
            ),
            Err(_opaque) => ids.push(None),
        }
    }
    Some(ids)
}

/// The endpoints the identifiers `ids` of the mechanism `by` select in
/// `snapshot`, the directive written at `at`.
///
/// An endpoint mechanism selects each endpoint it names; an organisation
/// mechanism selects every endpoint each organisation manages (N20).
fn expanded(
    snapshot: &RegistrySnapshot,
    by: Mechanism,
    ids: &[Option<&str>],
    at: Option<&Range<usize>>,
) -> Result<BTreeSet<EndpointId>, TargetError> {
    if ids.is_empty() {
        return Err(TargetError::Empty(by));
    }
    let organisations = matches!(
        by,
        Mechanism::OrganisationDirective | Mechanism::OrganisationHeader
    );
    // NOTE: §8.4.1, an identifier not of a registry id's form names nothing the
    // registry knows, so its parse failure is the unknown-identifier answer.
    let mut endpoints = BTreeSet::new();
    for (index, id) in ids.iter().enumerate() {
        let position = index.saturating_add(1);
        if organisations {
            let organisation = id
                .and_then(|id| OrganisationId::new(id).ok())
                .filter(|organisation| snapshot.organisation(organisation).is_some())
                .ok_or_else(|| TargetError::UnknownOrganisation {
                    by,
                    position,
                    at: at.cloned(),
                })?;
            endpoints.extend(
                snapshot
                    .endpoints_managed_by(&organisation)
                    .map(|endpoint| endpoint.id().clone()),
            );
        } else {
            let endpoint = id
                .and_then(|id| EndpointId::new(id).ok())
                .and_then(|endpoint| snapshot.endpoint(&endpoint))
                .ok_or_else(|| TargetError::UnknownEndpoint {
                    by,
                    position,
                    at: at.cloned(),
                })?;
            endpoints.insert(endpoint.id().clone());
        }
    }
    Ok(endpoints)
}

/// The ` (bytes a..b)` suffix of a message, or nothing for an unknown span.
struct At<'a>(&'a Option<Range<usize>>);

impl fmt::Display for At<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(bytes) => write!(f, " (bytes {}..{})", bytes.start, bytes.end),
            None => Ok(()),
        }
    }
}

/// A node set as a message names it: `[a, b]`, or `no endpoint`.
///
/// Every identifier in it is the registry's own, read back from the snapshot
/// after the request's identifiers were found there, so the message quotes
/// no client text (§5.4.3); §8.4.1 requires the error to name both sets.
struct Listed<'a>(&'a BTreeSet<EndpointId>);

impl fmt::Display for Listed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            return f.write_str("no endpoint");
        }
        f.write_str("[")?;
        for (index, endpoint) in self.0.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            f.write_str(endpoint.as_str())?;
        }
        f.write_str("]")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use ferrofed_registry::id::EndpointId;
    use http::{HeaderMap, HeaderValue};
    use openehr_federation::headers;

    use super::{Mechanism, Selected, TargetError};
    use crate::error::Code;

    fn set(ids: &[&str]) -> BTreeSet<EndpointId> {
        ids.iter().map(|id| EndpointId::new(*id).unwrap()).collect()
    }

    #[test]
    fn a_header_list_ignores_empty_elements_and_counts_every_line() {
        let mut map = HeaderMap::new();
        map.append(headers::ENDPOINT, HeaderValue::from_static(" a ,, b,"));
        map.append(headers::ENDPOINT, HeaderValue::from_static("c"));
        assert_eq!(
            Some(vec![Some("a"), Some("b"), Some("c")]),
            super::listed(&map, headers::ENDPOINT)
        );
        assert_eq!(None, super::listed(&map, headers::ORGANISATION));
        let mut opaque = HeaderMap::new();
        opaque.append(
            headers::ENDPOINT,
            HeaderValue::from_bytes(b"node-\xe9").unwrap(),
        );
        assert_eq!(Some(vec![None]), super::listed(&opaque, headers::ENDPOINT));
    }

    // conformance: CP-28
    #[test]
    fn a_conflict_names_both_sets_and_answers_its_own_code() {
        let conflict = TargetError::Conflict {
            first: Selected {
                by: Mechanism::EndpointDirective,
                endpoints: set(&["node-a-pub", "node-b-pub"]),
            },
            second: Selected {
                by: Mechanism::EndpointHeader,
                endpoints: set(&[]),
            },
        };
        assert_eq!(
            "the endpoint directive selects [node-a-pub, node-b-pub] and the openEHR-federation-endpoint header selects no endpoint, so the request names two node sets and neither is chosen (§8.4.1, N35)",
            conflict.to_string()
        );
        assert_eq!(Code::TargetingConflict, conflict.code());
    }

    #[test]
    fn an_empty_header_answers_the_code_of_its_kind() {
        assert_eq!(
            Code::EndpointUnknown,
            TargetError::Empty(Mechanism::EndpointHeader).code()
        );
        assert_eq!(
            Code::OrganisationUnknown,
            TargetError::Empty(Mechanism::OrganisationHeader).code()
        );
    }
}
