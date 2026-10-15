// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resources the operator console manages, on the admin listener.
//!
//! A member of the federation is owned whole: the registry document (or a
//! care services directory) provides it and it is read-only here, or the
//! console does and it lives in the configuration revisions as one entry of
//! `[[registry.members]]`. A save or a removal writes a new revision on the
//! one the operator read, which the operator previews with the diff and then
//! applies, so a member's change has the history and the one-step rollback
//! of every revision. A change naming a resource the file provides is
//! refused with `409` (`resource-file-owned`), whatever the console shows.
//!
//! A stored-query version is immutable once held (§12.7, N44), so the
//! console stores a new version in the gateway's stored-query store and
//! never edits or removes one; the read-only definition files and the
//! gateway's reserved namespace refuse it. No specification governs the
//! ownership of a resource: our own design.

use std::sync::Arc;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, State};
use axum::response::{IntoResponse, Response};
use ferrofed_eehrxf::reserved;
use ferrofed_registry::console::{
    ConsoleMember, ConsoleState, MemberResource, Owner, QueryRegistry, QueryResource, Resources,
};
use ferrofed_registry::definition::store::Insertion;
use ferrofed_registry::definition::{QueryName, QueryVersion, StoredDefinition};
use ferrofed_registry::id::NodeId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use http::{HeaderMap, StatusCode};
use jiff::Timestamp;
use serde::Deserialize;

use super::{Answer, Managed, answered, blocking, body_refused, held, record, refused};
use crate::admin::Operator;
use crate::config::error::Error;
use crate::error::{self, Code};
use crate::request_id;
use crate::revision::service::{Refused, RevisionError, SAVED, held_table};
use crate::stored::{self, Inadmissible};
use ferrofed_registry::resource::Resource;
use ferrofed_registry::revision::{Author, Origin, Revision, RevisionId};

use super::sections;
use crate::revision::resources::SectionFault;

/// The `event` of the record of a stored-query version the console stored.
pub const STORED: &str = "config-stored-query-stored";

/// Why a change to a console resource is refused.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConsoleError {
    /// The revisions refused it.
    #[error(transparent)]
    Revision(#[from] RevisionError),
    /// The resource is provided by the file or a directory.
    #[error("member {node} is provided by the {}, so only that source changes it", owner.as_str())]
    FileOwned {
        /// The member's `node_id`.
        node: NodeId,
        /// Who provides it.
        owner: Owner,
    },
    /// The newest revision holds no console member of that `node_id`.
    #[error("the revision holds no console member {0}")]
    Unknown(NodeId),
    /// The credential or binding section is provided by the file.
    #[error("{0} is provided by the configuration file, so only the file changes it")]
    ResourceFileOwned(Resource),
    /// The newest revision holds no credential or binding section of that
    /// name, or this gateway is built without the binding.
    #[error("the revision holds no {0}, or this gateway is built without its binding")]
    ResourceUnknown(Resource),
    /// The settings or the secrets of a credential or a binding section are
    /// refused.
    #[error(transparent)]
    Section(#[from] SectionFault),
}

impl From<Refused> for ConsoleError {
    fn from(refused: Refused) -> Self {
        Self::Revision(refused.into())
    }
}

impl<T: serde::Serialize> Answer for Result<T, ConsoleError> {
    fn answer(self, request_id: &str) -> Response {
        match self {
            Ok(body) => Json(body).into_response(),
            Err(ConsoleError::Revision(refusal)) => refused(&refusal, request_id),
            Err(
                refusal @ (ConsoleError::FileOwned { .. } | ConsoleError::ResourceFileOwned(_)),
            ) => error::response(Code::ResourceFileOwned, refusal.to_string(), request_id),
            Err(refusal @ (ConsoleError::Unknown(_) | ConsoleError::ResourceUnknown(_))) => {
                error::response(Code::ResourceUnknown, refusal.to_string(), request_id)
            }
            Err(ConsoleError::Section(fault)) => sections::refused(&fault, request_id),
        }
    }
}

/// The body of a member's save.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MemberRequest {
    /// The revision the save is based on, which must be the newest.
    base: Option<RevisionId>,
    /// Why the change is made.
    reason: String,
    /// The member, whole.
    member: ConsoleMember,
}

/// The members of a member's save.
const MEMBER_SHAPE: &str = r#"{"base": <revision>, "reason": "<text>", "member": {"organisation": {...}, "node": {...}, "endpoints": [...]}}"#;

/// The body of a member's removal.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RemoveRequest {
    /// The revision the removal is based on, which must be the newest.
    base: Option<RevisionId>,
    /// Why the change is made.
    reason: String,
}

/// The members of a member's removal.
const REMOVE_SHAPE: &str = r#"{"base": <revision>, "reason": "<text>"}"#;

/// The body of a stored-query version's store.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct QueryRequest {
    /// The AQL text.
    aql: String,
}

/// The members of a stored-query version's store.
const QUERY_SHAPE: &str = r#"{"aql": "<text>"}"#;

/// The members `table`, a revision's configuration, holds under
/// `[[registry.members]]`.
fn members_of(table: &toml::Table) -> Result<Vec<ConsoleMember>, RevisionError> {
    let Some(members) = table
        .get("registry")
        .and_then(toml::Value::as_table)
        .and_then(|registry| registry.get("members"))
    else {
        return Ok(Vec::new());
    };
    members.clone().try_into().map_err(|error| {
        RevisionError::Store(crate::revision::StoreError::Corrupt(Box::new(error)))
    })
}

/// The members revision `id` holds, none for no revision.
fn members_at(
    managed: &Managed,
    id: Option<RevisionId>,
) -> Result<Vec<ConsoleMember>, RevisionError> {
    match id {
        Some(id) => members_of(&held_table(&held(&managed.revisions, id)?)?),
        None => Ok(Vec::new()),
    }
}

/// Who provides the registry's members that are not the console's.
fn source(managed: &Managed) -> Owner {
    if managed.reloader.reads_directory() {
        Owner::Directory
    } else {
        Owner::File
    }
}

/// Whether the registry's own part, the members not the console's, holds
/// an identity `member` names: its organisation id, its `node_id`, its
/// `system_id`, an `endpoint_id` or an endpoint's base URL.
fn file_holds(snapshot: &RegistrySnapshot, member: &ConsoleMember) -> bool {
    let theirs = |node: &NodeId| !snapshot.is_console(node);
    let organisation = &member.organisation.id;
    let node = snapshot.node(&member.node.id).is_some() && theirs(&member.node.id);
    let system_id = snapshot
        .node_for_system_id(&member.node.system_id)
        .is_some_and(|node| theirs(node.id()));
    let operated = snapshot.organisation(organisation).is_some()
        && !snapshot
            .nodes()
            .any(|node| node.organisation() == organisation && snapshot.is_console(node.id()));
    let endpoints = member.endpoints.iter().any(|endpoint| {
        // NOTE: no specification governs this: our own design; text that is no URL
        // addresses no endpoint, and the registry then refuses it as a base URL.
        let url = url::Url::parse(&endpoint.url).ok();
        snapshot
            .endpoints()
            .filter(|held| theirs(held.node()))
            .any(|held| *held.id() == endpoint.id || url.as_ref() == Some(held.url()))
            || snapshot
                .native_endpoints()
                .filter(|held| theirs(held.node()))
                .any(|held| *held.id() == endpoint.id || url.as_ref() == Some(held.url()))
    });
    node || system_id || operated || endpoints
}

/// The running registry, empty when the gateway federates nothing.
fn snapshot(managed: &Managed) -> Option<RegistrySnapshot> {
    managed
        .reloader
        .state()
        .federation()
        .map(|federation| federation.snapshot().clone())
}

/// `GET` [`super::RESOURCES`]: every member and stored query, with its owner.
pub(super) async fn resources(State(managed): State<Arc<Managed>>, headers: HeaderMap) -> Response {
    let managed = Arc::clone(&managed);
    blocking(&headers, move || report(&managed)).await
}

/// The members and stored queries `managed` serves, with their owners.
fn report(managed: &Managed) -> Result<Resources, RevisionError> {
    let store = managed.revisions.store();
    let running = store.current()?.map(|mark| mark.revision);
    let head = store.head()?;
    let running_members = members_at(managed, running)?;
    let head_members = members_at(managed, head)?;
    let snapshot = snapshot(managed);
    let mut members = Vec::new();
    if let Some(snapshot) = &snapshot {
        let owner = source(managed);
        for node in snapshot
            .nodes()
            .filter(|node| !snapshot.is_console(node.id()))
        {
            let mut endpoints: Vec<_> = snapshot
                .endpoints_of(node.id())
                .map(|endpoint| endpoint.id().clone())
                .chain(
                    snapshot
                        .native_endpoints()
                        .filter(|native| native.node() == node.id())
                        .map(|native| native.id().clone()),
                )
                .collect();
            endpoints.sort();
            members.push(MemberResource {
                node: node.id().clone(),
                organisation: node.organisation().clone(),
                system_id: node.system_id().clone(),
                endpoints,
                owner,
                state: None,
                member: None,
            });
        }
    }
    let running_has = |member: &ConsoleMember| running_members.contains(member);
    for member in &head_members {
        let state = match &snapshot {
            Some(snapshot) if file_holds(snapshot, member) => ConsoleState::Shadowed,
            Some(snapshot) if snapshot.is_console(&member.node.id) && running_has(member) => {
                ConsoleState::Running
            }
            _ => ConsoleState::Pending,
        };
        members.push(console_resource(member, state));
    }
    for member in &running_members {
        let kept = head_members
            .iter()
            .any(|held| held.node.id == member.node.id);
        if !kept {
            members.push(console_resource(member, ConsoleState::Removing));
        }
    }
    members.sort_by(|a, b| a.node.cmp(&b.node));
    Ok(Resources {
        running,
        head,
        members,
        stored_query_registry: registry(managed),
        stored_queries: queries(managed)?,
        sections: sections::report(managed)?,
        bindings: sections::available(),
    })
}

/// The report of `member`, which the console owns, standing at `state`.
fn console_resource(member: &ConsoleMember, state: ConsoleState) -> MemberResource {
    MemberResource {
        node: member.node.id.clone(),
        organisation: member.organisation.id.clone(),
        system_id: member.node.system_id.clone(),
        endpoints: member
            .endpoints
            .iter()
            .map(|endpoint| endpoint.id.clone())
            .collect(),
        owner: Owner::Console,
        state: Some(state),
        member: Some(member.clone()),
    }
}

/// The stored-query registry `managed` serves, and whether it takes a new
/// version.
fn registry(managed: &Managed) -> QueryRegistry {
    match managed.reloader.state().definitions() {
        None => QueryRegistry::Absent,
        Some(definitions) if definitions.is_read_only() => QueryRegistry::ReadOnly,
        Some(_) => QueryRegistry::Writable,
    }
}

/// Every stored-query version the gateway holds, with its owner: the
/// gateway's reserved namespace, the read-only definition files, or the
/// store.
fn queries(managed: &Managed) -> Result<Vec<QueryResource>, RevisionError> {
    let Some(definitions) = managed.reloader.state().definitions() else {
        return Ok(Vec::new());
    };
    definitions.refresh().map_err(|error| {
        RevisionError::Store(crate::revision::StoreError::Backend(Box::new(error)))
    })?;
    let read_only = definitions.is_read_only();
    Ok(definitions
        .list("")
        .iter()
        .map(|definition| {
            let owner = if reserved::reserves(definition.name()) {
                Owner::Gateway
            } else if read_only {
                Owner::File
            } else {
                Owner::Store
            };
            QueryResource {
                name: definition.name().as_str().to_owned(),
                version: definition.version().to_string(),
                owner,
            }
        })
        .collect())
}

/// The configuration of revision `base`, which a change is written on top
/// of; an empty one for none.
pub(super) fn base_table(
    managed: &Managed,
    base: Option<RevisionId>,
) -> Result<toml::Table, RevisionError> {
    match base {
        Some(id) => held_table(&held(&managed.revisions, id)?),
        None => Ok(toml::Table::new()),
    }
}

/// `table` with `members` as its `[[registry.members]]`, the key removed
/// when there is none.
fn with_members(mut table: toml::Table, members: &[ConsoleMember]) -> Result<toml::Table, Refused> {
    let registry = table
        .entry("registry")
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let toml::Value::Table(registry) = registry {
        if members.is_empty() {
            registry.remove("members");
        } else {
            let value = toml::Value::try_from(members)
                .map_err(|source| Refused::from(Error::Assemble { source }))?;
            registry.insert(String::from("members"), value);
        }
    }
    Ok(table)
}

/// Saves `table` as the revision after `base`, made by `operator` for
/// `reason`, and records it.
pub(super) fn saved_as(
    managed: &Managed,
    (base, table): (Option<RevisionId>, &toml::Table),
    (operator, reason): (&Operator, String),
) -> Result<Revision, ConsoleError> {
    let text =
        toml::to_string(table).map_err(|source| Refused::from(Error::Assemble { source }))?;
    let saved = guarded(
        managed,
        (Origin::Operator, base),
        (Author::from(operator), reason),
        &text,
    )?;
    record(SAVED, saved.id, operator);
    Ok(saved)
}

/// Saves `text` as the revision after `base`, from `origin`, by `author`
/// for `reason`: the one path every save on the admin listener takes.
///
/// A member the registry document or a directory provides cannot be set
/// unless `base` holds it as it is, and [`Revisions::save_from`] refuses a
/// change to a credential or a binding section the file provides.
///
/// [`Revisions::save_from`]: crate::revision::service::Revisions::save_from
pub(super) fn guarded(
    managed: &Managed,
    (origin, base): (Origin, Option<RevisionId>),
    (author, reason): (Author, String),
    text: &str,
) -> Result<Revision, ConsoleError> {
    // NOTE: no specification governs this: our own design; a TOML or members that do
    // not read are refused by the save with the errors `config check` reports.
    if let Ok(table) = toml::from_str::<toml::Table>(text)
        && let Ok(members) = members_of(&table)
        && let Some(snapshot) = snapshot(managed)
    {
        let held = members_at(managed, base)?;
        if let Some(member) = members
            .into_iter()
            .find(|member| !held.contains(member) && file_holds(&snapshot, member))
        {
            return Err(ConsoleError::FileOwned {
                node: member.node.id,
                owner: source(managed),
            });
        }
    }
    Ok(managed
        .revisions
        .save_from(origin, base, (author, reason), text)?)
}

/// `201` for an answer that is `200`, as a save answers.
pub(super) fn created(answer: Response) -> Response {
    if answer.status() == StatusCode::OK {
        (StatusCode::CREATED, answer).into_response()
    } else {
        answer
    }
}

/// `PUT` [`super::MEMBER`]: saves a revision on the base the body names
/// with the member in place of the console's member of that `node_id`, or
/// beside the others, `201` with the revision; it is not applied.
///
/// A member that shares an identity with one the registry document or a
/// directory provides is refused `409` (`resource-file-owned`).
pub(super) async fn save_member(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(node): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(request) = serde_json::from_slice::<MemberRequest>(&body) else {
        return body_refused(&headers, MEMBER_SHAPE);
    };
    if request.member.node.id.as_str() != node {
        return error::response(
            Code::BodyInvalid,
            "the member's node.id is not the node_id the path names",
            request_id::of(&headers).unwrap_or_default(),
        );
    }
    let managed = Arc::clone(&managed);
    created(
        answered(&headers, move || {
            let MemberRequest {
                base,
                reason,
                member,
            } = request;
            if snapshot(&managed).is_some_and(|snapshot| file_holds(&snapshot, &member)) {
                return Err(ConsoleError::FileOwned {
                    node: member.node.id,
                    owner: source(&managed),
                });
            }
            let table = base_table(&managed, base)?;
            let mut members = members_of(&table)?;
            match members
                .iter_mut()
                .find(|held| held.node.id == member.node.id)
            {
                Some(held) => *held = member,
                None => members.push(member),
            }
            let table = with_members(table, &members)?;
            saved_as(&managed, (base, &table), (&operator, reason))
        })
        .await,
    )
}

/// `POST` [`super::MEMBER_REMOVE`]: saves a revision on the base the body
/// names without the console's member of that `node_id`, `201` with the
/// revision; it is not applied.
///
/// A member the registry document or a directory provides is refused `409`
/// (`resource-file-owned`), and one the base holds no console member of
/// `404` (`resource-unknown`).
pub(super) async fn remove_member(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path(node): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let Ok(request) = serde_json::from_slice::<RemoveRequest>(&body) else {
        return body_refused(&headers, REMOVE_SHAPE);
    };
    let Ok(node) = NodeId::new(node) else {
        return error::response(
            Code::BodyInvalid,
            "the path names no node_id",
            request_id::of(&headers).unwrap_or_default(),
        );
    };
    let managed = Arc::clone(&managed);
    created(
        answered(&headers, move || {
            let table = base_table(&managed, request.base)?;
            let mut members = members_of(&table)?;
            let before = members.len();
            members.retain(|held| held.node.id != node);
            if members.len() == before {
                let theirs = snapshot(&managed).is_some_and(|snapshot| {
                    snapshot.node(&node).is_some() && !snapshot.is_console(&node)
                });
                return Err(if theirs {
                    ConsoleError::FileOwned {
                        node,
                        owner: source(&managed),
                    }
                } else {
                    ConsoleError::Unknown(node)
                });
            }
            let table = with_members(table, &members)?;
            saved_as(
                &managed,
                (request.base, &table),
                (&operator, request.reason),
            )
        })
        .await,
    )
}

/// `PUT` [`super::STORED_QUERY`]: stores the body's AQL as the version the
/// path names in the gateway's stored-query store, `201` with the ITS-REST
/// `StoredQuery` (§12.7, N44).
///
/// It is admitted as a `PUT {base}/v1/definition/query/{name}/{version}`
/// is: a held version is `409` (`stored-query-held`), the reserved
/// namespace `409` (`stored-query-reserved`), a text the rewrite refuses a
/// `400` with its refusal's code, and a patient named by a literal `400`
/// (`subject-literal`). The read-only definition files are the file's, so
/// a store there is `409` (`resource-file-owned`), and a gateway with no
/// stored-query registry answers `501`.
pub(super) async fn store_query(
    State(managed): State<Arc<Managed>>,
    Extension(operator): Extension<Operator>,
    Path((name, version)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let request_id = request_id::of(&headers).unwrap_or_default().to_owned();
    let Ok(request) = serde_json::from_slice::<QueryRequest>(&body) else {
        return body_refused(&headers, QUERY_SHAPE);
    };
    let state = managed.reloader.state();
    let (Some(definitions), Some(federation)) = (state.definitions(), state.federation()) else {
        return error::fixed(Code::NotImplemented, &request_id);
    };
    let name = match QueryName::new(&name) {
        Ok(name) => name,
        Err(refused) => {
            return error::response(Code::QueryNameInvalid, refused.to_string(), &request_id);
        }
    };
    let version = match version.parse::<QueryVersion>() {
        Ok(version) => version,
        Err(refused) => {
            return error::response(Code::QueryVersionInvalid, refused.to_string(), &request_id);
        }
    };
    if reserved::reserves(&name) {
        return error::fixed(Code::StoredQueryReserved, &request_id);
    }
    if definitions.is_read_only() {
        return error::response(
            Code::ResourceFileOwned,
            "the stored queries are read from the read-only definition files, so only those files change them",
            &request_id,
        );
    }
    let admitted = match stored::admit(&request.aql, federation.context()) {
        Ok(admitted) => admitted,
        Err(Inadmissible::Refused(refusal)) => {
            crate::facade::security::refused(&refusal, &request_id);
            return error::response(
                Code::Refused((&refusal).into()),
                refusal.to_string(),
                &request_id,
            );
        }
        Err(Inadmissible::SubjectLiteral { at }) => {
            crate::facade::security::definition_refused(at.as_ref(), &request_id);
            return error::fixed(Code::SubjectLiteral, &request_id);
        }
    };
    let definition =
        StoredDefinition::new(name, version, admitted.aql().to_owned(), Timestamp::now());
    let held = Arc::clone(definitions);
    let inserted = {
        let definition = definition.clone();
        tokio::task::spawn_blocking(move || held.insert(definition)).await
    };
    match inserted {
        Ok(Ok(Insertion::Stored)) => {
            tracing::info!(
                target: crate::facade::security::TARGET,
                event = STORED,
                name = definition.name().as_str(),
                version = %definition.version(),
                issuer = operator.issuer(),
                subject = operator.subject(),
                "an operator stored a stored-query version"
            );
            (
                StatusCode::CREATED,
                Json(crate::facade::stored::its_rest(&definition)),
            )
                .into_response()
        }
        Ok(Ok(Insertion::Held)) => error::fixed(Code::StoredQueryHeld, &request_id),
        Ok(Err(failure)) => {
            tracing::error!(
                error = crate::chain(&failure),
                "a stored query could not be stored for the operator"
            );
            error::fixed(Code::Internal, &request_id)
        }
        Err(failure) => {
            tracing::error!(error = %failure, "the stored-query store for the operator did not complete");
            error::fixed(Code::Internal, &request_id)
        }
    }
}
