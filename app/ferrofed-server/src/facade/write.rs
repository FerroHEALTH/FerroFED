// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Writes routed to one node: a versioned write only to the CDR that controls
//! the version it amends, and a new EHR only to an explicitly chosen node
//! (§12.4, §12a.1, N23).
//!
//! [`Write::of`] names what an ITS-REST operation writes, from its
//! `operationId`:
//!
//! - a **versioned write** amends existing versions: the one it names in
//!   `If-Match` (`composition_update`, `ehr_status_update`,
//!   `directory_update`, `directory_delete`) or in its path
//!   (`composition_delete`), or those a `CONTRIBUTION` names in its body
//!   (`contribution_create`);
//! - a **new EHR** (`ehr_create`, `ehr_create_with_id`) has no owner yet, so
//!   only the targeting headers name its node (§12.4, §8.4);
//! - every other operation, a read or a create inside an existing EHR
//!   included, goes where its path `ehr_id` routes it (§12.5.1, N41).
//!
//! A versioned write is EHR-scoped, so it goes to the node its path `ehr_id`
//! routes to, by the explicit target, a held binding or the `ehr_id` index,
//! and never by the ask-all probe (§12a.1 `route-ehr`, N41). [`controlled`]
//! then holds that node to `route-write`: the registry must route each
//! preceding version's `creating_system_id` to it, as the member's own
//! `system_id` or a registered `[[creating_system]]` mapping (§12a.1, N21,
//! N23). A learned route never shows control, because a holder of a version
//! need not have created it (§10.2, §10.3). A node that does not control the
//! version is never sent the write, and no other node is either: the path
//! `ehr_id` is that node's own, so no other node can take the request
//! unchanged (N22, N42a), and the write is refused `409` naming the
//! controlling system (§10.3 `copy-write-reject`, N36): its
//! `creating_system_id` as the registry spells it, with the controlling node
//! and its endpoint, or, for a system the registry does not know, the place
//! in the request that names the version. No value of the request is quoted
//! back. The refusal never asks the controlling node anything, so it is the
//! same whether that node is up or down, and a write derived from a
//! de-duplicated row is held to it whichever endpoint the row was read from
//! (§10.3 `dedup-write-target`, CP-29).
//!
//! A `CONTRIBUTION` is read with `openehr-its`'s ITS-REST `NewContribution`
//! for its versions' `preceding_version_uid`s only, in the representation its
//! `Content-Type` selects: canonical JSON, or a canonical envelope whose
//! versions' `data` is FLAT or STRUCTURED (ITS-REST 1.1.0
//! `contribution_create`, §Simplified Formats). The body forwarded is the one
//! received, byte for byte (N22). Every version that names a preceding
//! version must be controlled by the path node, and a version that names none
//! is a creation inside the path EHR, so a `CONTRIBUTION` of creations alone
//! routes by its path `ehr_id` as any other request does.

use std::fmt;

use ferrofed_engine::declared;
use ferrofed_registry::creating_system::CreatingSystemRoute;
use ferrofed_registry::id::{EndpointId, NodeId, SystemId};
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, header};
use openehr_base::prelude::ObjectVersionId;
use openehr_its::json;
use openehr_its::rest::generated::ehr::{NewContribution, Versionable};
use openehr_its::rest::routes::RouteMatch;
use serde::de::{DeserializeOwned, IgnoredAny};

use crate::error::Code;
use crate::facade::owner;
use crate::facade::route::EHR_GROUP;

/// The path parameter a `DELETE` of a composition names the version it
/// amends in (ITS-REST 1.1.0 EHR API, `composition_delete`).
const PRECEDING_PARAM: &str = "uid_based_id";

/// The canonical JSON media type (ITS-REST 1.1.0 overview, §JSON Format).
const CANONICAL_JSON: &str = "application/json";

/// The canonical XML media type (ITS-REST 1.1.0 overview, §XML Format).
const CANONICAL_XML: &str = "application/xml";

/// The Simplified Flat media type (ITS-REST 1.1.0 overview, §Simplified
/// Formats).
const SIMPLIFIED_FLAT: &str = "application/openehr.wt.flat+json";

/// The Simplified Structured media type (ITS-REST 1.1.0 overview,
/// §Simplified Formats).
const SIMPLIFIED_STRUCTURED: &str = "application/openehr.wt.structured+json";

/// What an ITS-REST operation routed to one node writes (§12.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Write {
    /// A write that can amend existing versions, named where [`Preceding`]
    /// says.
    Versioned(Preceding),
    /// The creation of an EHR, which only the targeting headers route.
    NewEhr,
    /// A read, or a write that is neither: it goes where its path `ehr_id`
    /// routes it.
    Routed,
}

/// Where a versioned write names the version it amends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preceding {
    /// In `If-Match`, as one quoted `OBJECT_VERSION_ID`.
    IfMatch,
    /// In the path, as the `uid_based_id` segment.
    Path,
    /// In the body of a `CONTRIBUTION`, as each version's
    /// `preceding_version_uid`, which a version that creates an object omits.
    Contribution,
}

impl Write {
    /// What the operation `matched` writes, by its ITS-REST `operationId`.
    ///
    /// ITS-REST 1.1.0 names the preceding version of a `PUT` and of a
    /// directory `DELETE` in `If-Match`, of a composition `DELETE` in the
    /// path, and of each version a `CONTRIBUTION` to an EHR commits in its
    /// body. An `operationId` is unique only within its API group, so every
    /// classified operation is matched in the EHR group alone.
    #[must_use]
    pub fn of(matched: &RouteMatch) -> Self {
        if matched.group != EHR_GROUP {
            return Self::Routed;
        }
        match matched.operation_id {
            "composition_update" | "ehr_status_update" | "directory_update"
            | "directory_delete" => Self::Versioned(Preceding::IfMatch),
            "composition_delete" => Self::Versioned(Preceding::Path),
            "contribution_create" => Self::Versioned(Preceding::Contribution),
            "ehr_create" | "ehr_create_with_id" => Self::NewEhr,
            _ => Self::Routed,
        }
    }
}

/// Whether `matched` is the creation of an EHR with no `ehr_id` in its path,
/// `POST {base}/v1/ehr`, routed only by the targeting headers (§12.4).
#[must_use]
pub fn creates_ehr(matched: &RouteMatch) -> bool {
    matched.group == EHR_GROUP && matched.operation_id == "ehr_create"
}

/// Why a versioned write routed to `at` is refused before anything is sent.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refused {
    /// The write names no single preceding version.
    #[error(transparent)]
    Preceding(#[from] PrecedingInvalid),
    /// The node the write routes to does not control the preceding version.
    #[error(transparent)]
    NotControlling(#[from] NotControlling),
}

impl Refused {
    /// The stable code the error body names.
    #[must_use]
    pub fn code(&self) -> Code {
        match self {
            Self::Preceding(_) => Code::PrecedingVersionInvalid,
            Self::NotControlling(_) => Code::ControllingSystemUnreachable,
        }
    }
}

/// Why a versioned write names no single preceding version (§12.4, N23).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PrecedingInvalid {
    /// `If-Match` is absent.
    #[error(
        "the versioned write carries no If-Match, so the version it amends, and its controlling CDR, are unknown (§12.4, N23)"
    )]
    Missing,
    /// `If-Match` names more than one entity tag, or is repeated.
    #[error(
        "If-Match names more than one version, and a versioned write amends exactly one (§12.4, N23)"
    )]
    Several,
    /// The version named is no quoted `OBJECT_VERSION_ID`.
    #[error(
        "the version the write amends is not one OBJECT_VERSION_ID, quoted in If-Match or written in the path of a DELETE, so its controlling CDR cannot be found (§12.4, N23)"
    )]
    Malformed,
    /// The body is no `CONTRIBUTION` in the JSON representation its
    /// `Content-Type` selects, a version's `preceding_version_uid` included,
    /// so the versions it amends are unknown.
    #[error(
        "the body is not a CONTRIBUTION in the representation its Content-Type selects, canonical JSON or a canonical envelope whose data is FLAT or STRUCTURED, whose every preceding_version_uid is one OBJECT_VERSION_ID, so the versions it amends, and their controlling CDR, are unknown (ITS-REST 1.1.0 contribution_create; §12.4, N23)"
    )]
    Contribution,
    /// The body is a `CONTRIBUTION` in canonical XML, which the gateway does
    /// not read, so the versions it amends are unknown.
    #[error(
        "a CONTRIBUTION in canonical XML is not read, so the versions it amends, and their controlling CDR, are unknown; send it in canonical JSON or a Simplified Format (ITS-REST 1.1.0 contribution_create; §12.4, N23)"
    )]
    XmlContribution,
}

/// Where a versioned write names a version it amends, which a refusal points
/// at in place of quoting the version (§5.4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    /// `If-Match`.
    IfMatch,
    /// The `uid_based_id` segment of the request path.
    Path,
    /// The `preceding_version_uid` of the version at this position of a
    /// `CONTRIBUTION`'s `versions`, counted from 1 in body order.
    Contribution(usize),
}

impl fmt::Display for Named {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IfMatch => f.write_str("the version If-Match names"),
            Self::Path => f.write_str("the version the request path names"),
            Self::Contribution(position) => write!(
                f,
                "the preceding_version_uid of version {position} of the CONTRIBUTION"
            ),
        }
    }
}

/// Why the node a versioned write routes to may not take it (§10.3, §12a.1,
/// N23, N36).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NotControlling {
    /// The registry routes the preceding version's `creating_system_id` to
    /// another node.
    #[error(
        "a version this write amends was created by system {creating_system_id}, which node {controller}{} controls, and the request's ehr_id routes the write to node {at}, which does not control it; a versioned write is never committed at another node than its controlling CDR (§10.3, §12a.1, N23, N36)",
        Through(.endpoint.as_ref())
    )]
    Elsewhere {
        /// The version's `creating_system_id`, as the registry spells it.
        creating_system_id: SystemId,
        /// The controlling node.
        controller: NodeId,
        /// The endpoint the controlling node is reached through, where it
        /// has an active one.
        endpoint: Option<EndpointId>,
        /// The node the path `ehr_id` routes to.
        at: NodeId,
    },
    /// The registry routes the preceding version's `creating_system_id` to no
    /// node: it is no member's `system_id` and no registered mapping's.
    #[error(
        "the controlling system of {named} is the creating_system_id in it, which is no member's system_id and no registered creating_system mapping, so no member is known to control the version, and the request's ehr_id routes the write to node {at}, which does not; a versioned write is never committed at another node than its controlling CDR (§10.3, §12a.1, N23, N36)"
    )]
    Unregistered {
        /// Where the request names the version.
        named: Named,
        /// The node the path `ehr_id` routes to.
        at: NodeId,
    },
}

/// Holds a versioned write that its path `ehr_id` routes to `at` to the rule
/// of §12a.1 `route-write`: `at` controls every version it amends (N23).
///
/// `(matched, headers, body)` is the request as it arrived; the body is read
/// only for a `CONTRIBUTION`, and never changed.
///
/// # Errors
///
/// [`Refused::Preceding`] when the write names no single preceding version,
/// or is a `CONTRIBUTION` that does not parse, and [`Refused::NotControlling`]
/// when the registry routes a preceding version's `creating_system_id` to
/// another node, or to none.
pub fn controlled(
    snapshot: &RegistrySnapshot,
    preceding: Preceding,
    (matched, headers, body): (&RouteMatch, &HeaderMap, &[u8]),
    at: &NodeId,
) -> Result<(), Refused> {
    let versions = match preceding {
        Preceding::IfMatch => vec![(Named::IfMatch, if_match(headers)?)],
        Preceding::Path => vec![(Named::Path, in_path(matched)?)],
        Preceding::Contribution => amended(declared::content_type(matched, headers), body)?,
    };
    versions
        .iter()
        .try_for_each(|(named, version)| controlled_at(snapshot, (*named, version), at))
}

/// Whether the registry routes the `creating_system_id` of `version`, which
/// the request names where `named` says, to `at` (§12a.1 `route-write`).
fn controlled_at(
    snapshot: &RegistrySnapshot,
    (named, version): (Named, &ObjectVersionId),
    at: &NodeId,
) -> Result<(), Refused> {
    // NOTE: §12a.1, a creating_system_id that is no openEHR uid is no member's
    // system_id, so it names no controlling CDR, the unregistered answer.
    let registered = SystemId::creating_system_id_of(version)
        .ok()
        .and_then(|creating_system_id| {
            snapshot
                .registered_creating_system(&creating_system_id)
                .map(|(spelled, route)| (spelled.clone(), route))
        });
    match registered {
        Some((_, route)) if route.node() == at => Ok(()),
        Some((creating_system_id, route)) => Err(NotControlling::Elsewhere {
            creating_system_id,
            endpoint: through(snapshot, &route),
            controller: route.node().clone(),
            at: at.clone(),
        }
        .into()),
        // NOTE: §10.3 copy-write-reject with §5.4.3: the unregistered system is the
        // request's own value, so the refusal identifies it by where the request names it.
        None => Err(NotControlling::Unregistered {
            named,
            at: at.clone(),
        }
        .into()),
    }
}

/// The versions a `CONTRIBUTION` body amends, in body order: each version's
/// `preceding_version_uid` (ITS-REST 1.1.0 EHR API, `contribution_create`,
/// `NewContribution`), with its place in the body.
///
/// `media` is the listed media type the request's `Content-Type` names, and
/// `None` when it sends none, which reads as canonical JSON. Under a
/// Simplified Format the envelope stays canonical and only each version's
/// `data` is FLAT or STRUCTURED (ITS-REST 1.1.0 `contribution_create`), so
/// the envelope is read as in canonical JSON and `data` is left to the node.
/// A version with no `preceding_version_uid` creates an object, and amends
/// none.
fn amended(
    media: Option<&str>,
    body: &[u8],
) -> Result<Vec<(Named, ObjectVersionId)>, PrecedingInvalid> {
    let text = std::str::from_utf8(body).map_err(|_not_utf8| PrecedingInvalid::Contribution)?;
    let preceding = match media {
        None | Some(CANONICAL_JSON) => preceding_of::<Versionable>(text)?,
        Some(SIMPLIFIED_FLAT | SIMPLIFIED_STRUCTURED) => preceding_of::<IgnoredAny>(text)?,
        // TODO(#308): a CONTRIBUTION in canonical XML is refused 400 until openehr-its reads it.
        Some(CANONICAL_XML) => return Err(PrecedingInvalid::XmlContribution),
        Some(_unlisted) => return Err(PrecedingInvalid::Contribution),
    };
    Ok(preceding
        .into_iter()
        .zip(1_usize..)
        .filter_map(|(version, position)| {
            version.map(|preceding| (Named::Contribution(position), preceding))
        })
        .collect())
}

/// Each version's `preceding_version_uid` of the `CONTRIBUTION` `text`, in
/// body order, read with each version's `data` as a `T`.
fn preceding_of<T: DeserializeOwned>(
    text: &str,
) -> Result<Vec<Option<ObjectVersionId>>, PrecedingInvalid> {
    let contribution: NewContribution<T> =
        json::from_canonical_json(text).map_err(|_quoted| PrecedingInvalid::Contribution)?;
    Ok(contribution
        .versions
        .into_iter()
        .map(|version| version.preceding_version_uid)
        .collect())
}

/// The endpoint the controlling node of `route` is named by: the mapped
/// endpoint, or for a member, the endpoint it is asked through.
fn through(snapshot: &RegistrySnapshot, route: &CreatingSystemRoute) -> Option<EndpointId> {
    route.endpoint().cloned().or_else(|| {
        owner::reached_through(snapshot, route.node()).map(|endpoint| endpoint.id().clone())
    })
}

/// The version `If-Match` names: exactly one field line holding one quoted
/// `OBJECT_VERSION_ID` (ITS-REST 1.1.0 overview, `If-Match`).
///
/// ITS-REST gives the value as a `version_uid` enclosed in double quotes, so
/// a weak tag, `*`, and an unquoted value name no version.
fn if_match(headers: &HeaderMap) -> Result<ObjectVersionId, PrecedingInvalid> {
    let mut lines = headers.get_all(header::IF_MATCH).iter();
    let (Some(line), None) = (lines.next(), lines.next()) else {
        return Err(if headers.contains_key(header::IF_MATCH) {
            PrecedingInvalid::Several
        } else {
            PrecedingInvalid::Missing
        });
    };
    let text = line
        .to_str()
        .map_err(|_opaque| PrecedingInvalid::Malformed)?;
    let tag = text
        .trim()
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or(PrecedingInvalid::Malformed)?;
    if tag.contains('"') {
        return Err(PrecedingInvalid::Several);
    }
    ObjectVersionId::new(tag).map_err(|_malformed| PrecedingInvalid::Malformed)
}

/// The version the path of a composition `DELETE` names (ITS-REST 1.1.0 EHR
/// API: "the `uid_based_id` MUST be in a form of an `OBJECT_VERSION_ID`").
fn in_path(matched: &RouteMatch) -> Result<ObjectVersionId, PrecedingInvalid> {
    let decoded = matched
        .path_param(PRECEDING_PARAM)
        .and_then(|param| param.decoded().ok())
        .ok_or(PrecedingInvalid::Malformed)?;
    ObjectVersionId::new(decoded.as_str()).map_err(|_malformed| PrecedingInvalid::Malformed)
}

/// ` (endpoint e)` for a named endpoint, nothing otherwise.
struct Through<'a>(Option<&'a EndpointId>);

impl fmt::Display for Through<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(endpoint) => write!(f, " (endpoint {endpoint})"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Preceding, PrecedingInvalid, Write, creates_ehr, if_match};
    use http::{HeaderMap, HeaderValue, Method, header};
    use openehr_its::rest::routes::{Lookup, RouteMatch, lookup};

    const VERSION: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";

    fn write(method: &Method, path: &str) -> Option<Write> {
        match lookup(method, path) {
            Lookup::Matched(matched) => Some(Write::of(&matched)),
            Lookup::MethodNotAllowed { .. } | Lookup::NotFound => None,
        }
    }

    fn with_if_match(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(header::IF_MATCH, HeaderValue::from_static(value));
        }
        headers
    }

    #[test]
    fn each_versioned_write_names_where_it_carries_its_preceding_version() {
        let if_match = Some(Write::Versioned(Preceding::IfMatch));
        assert_eq!(if_match, write(&Method::PUT, "/ehr/7d44/composition/u"));
        assert_eq!(if_match, write(&Method::PUT, "/ehr/7d44/ehr_status"));
        assert_eq!(if_match, write(&Method::PUT, "/ehr/7d44/directory"));
        assert_eq!(if_match, write(&Method::DELETE, "/ehr/7d44/directory"));
        assert_eq!(
            Some(Write::Versioned(Preceding::Path)),
            write(&Method::DELETE, "/ehr/7d44/composition/u::s::1")
        );
        assert_eq!(
            Some(Write::Versioned(Preceding::Contribution)),
            write(&Method::POST, "/ehr/7d44/contribution")
        );
    }

    #[test]
    fn creating_an_ehr_is_a_new_ehr_and_everything_else_is_routed() {
        assert_eq!(Some(Write::NewEhr), write(&Method::POST, "/ehr"));
        assert_eq!(Some(Write::NewEhr), write(&Method::PUT, "/ehr/7d44"));
        for (method, path) in [
            (Method::POST, "/ehr/7d44/composition"),
            (Method::POST, "/ehr/7d44/directory"),
            (Method::POST, "/demographic/contribution"),
            (Method::GET, "/ehr/7d44/composition/u::s::1"),
            (Method::PUT, "/ehr/7d44/composition/u::s::1/tags"),
        ] {
            assert_eq!(Some(Write::Routed), write(&method, path), "{method} {path}");
        }
    }

    #[test]
    fn a_classified_operation_id_in_another_group_is_routed() {
        for operation_id in [
            "composition_update",
            "ehr_status_update",
            "directory_update",
            "directory_delete",
            "composition_delete",
            "contribution_create",
            "ehr_create",
            "ehr_create_with_id",
        ] {
            let elsewhere = RouteMatch {
                group: "demographic",
                operation_id,
                template: "/demographic/x",
                method: Method::POST,
                path_params: Vec::new(),
                params: &[],
                request_media: &[],
            };
            assert_eq!(Write::Routed, Write::of(&elsewhere), "{operation_id}");
            assert!(!creates_ehr(&elsewhere), "{operation_id}");
        }
        let Lookup::Matched(demographic) = lookup(&Method::POST, "/demographic/contribution")
        else {
            panic!("ITS-REST defines POST /demographic/contribution");
        };
        assert_eq!("contribution_create", demographic.operation_id);
        assert_eq!(Write::Routed, Write::of(&demographic));
    }

    #[test]
    fn if_match_names_one_quoted_object_version_id() {
        let quoted = format!("\"{VERSION}\"");
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_MATCH, HeaderValue::from_str(&quoted).unwrap());
        assert_eq!(VERSION, if_match(&headers).unwrap().value());
    }

    #[test]
    fn anything_but_one_quoted_object_version_id_names_no_preceding_version() {
        assert_eq!(Err(PrecedingInvalid::Missing), if_match(&HeaderMap::new()));
        for (values, refused) in [
            (
                vec!["\"a::b::1\"", "\"a::b::2\""],
                PrecedingInvalid::Several,
            ),
            (vec!["\"a::b::1\", \"a::b::2\""], PrecedingInvalid::Several),
            (vec!["*"], PrecedingInvalid::Malformed),
            (vec!["W/\"a::b::1\""], PrecedingInvalid::Malformed),
            (vec!["a::b::1"], PrecedingInvalid::Malformed),
            (vec!["\"not a version\""], PrecedingInvalid::Malformed),
        ] {
            assert_eq!(
                Err(refused),
                if_match(&with_if_match(&values)).map(|_| ()),
                "{values:?}"
            );
        }
    }
}
