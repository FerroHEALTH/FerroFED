// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What every answer teaches the follow-up routing table: each version an
//! endpoint is seen holding, mapped by its `creating_system_id` (§12.2, N21).
//!
//! Every fan-out answer and every routed answer is observed: each version an
//! endpoint is seen holding is fed to
//! [`ferrofed_registry::creating_system::LearnedMap::observe`], which learns a
//! route for a `creating_system_id` the registry document does not map and
//! raises an integrity incident on a conflict. The map lives in the
//! [`Federation`], behind one lock every request shares, as the `ehr_id`
//! index does; it is held only for the map's own update and never across an
//! `.await` (no specification governs where learned state lives: our own
//! design).
//!
//! A follow-up read of a version under `{base}/v1/ehr/{ehr_id}/…` is routed
//! by its path `ehr_id` in the order of §12.5.1 (N41), never by the version's
//! `creating_system_id`: §12a.1 routes an EHR-scoped request on the `ehr_id`,
//! N22 forbids rewriting that `ehr_id` for another node, and that node never
//! adopted it (N42a). The learned routes serve the versioned writes of §12.4.

use ferrofed_engine::forward::Forwarded;
use ferrofed_registry::creating_system::Sighting;
use ferrofed_registry::id::EndpointId;
use http::{Method, header};
use openehr_base::prelude::ObjectVersionId;
use openehr_its::rest::routes::{IdentifierClass, ParamLocation, RouteMatch};

use crate::federation::Federation;

/// The version a read under `matched` names in its path, or `None` when it
/// is a write or names no full `OBJECT_VERSION_ID`.
///
/// The path parameters read are those `openehr-its`'s table states as an
/// `OBJECT_VERSION_ID` or a `UID_BASED_ID`, in path order. A node that
/// answers such a read with a success holds the version (§12.2). A
/// `UID_BASED_ID` may be a `HIER_OBJECT_ID`, which carries no
/// `creating_system_id`.
#[must_use]
pub fn version_of(method: &Method, matched: &RouteMatch) -> Option<ObjectVersionId> {
    if !method.is_safe() {
        return None;
    }
    matched.path_params.iter().find_map(|segment| {
        let names_version = matched.params.iter().any(|param| {
            param.location == ParamLocation::Path
                && param.name == segment.name
                && matches!(
                    param.identifier,
                    Some(IdentifierClass::ObjectVersion | IdentifierClass::UidBased)
                )
        });
        if !names_version {
            return None;
        }
        let decoded = segment.decoded().ok()?;
        // NOTE: §12.2, a uid that is no OBJECT_VERSION_ID names no version
        // and carries no creating_system_id, so it is legitimately absent here.
        ObjectVersionId::new(decoded.as_str()).ok()
    })
}

/// The versions a successful routed answer shows `endpoint` holding: the one
/// the read named, and the one its `ETag` names.
///
/// ITS-REST names the version a read or a write answered with in the `ETag`
/// as a quoted `OBJECT_VERSION_ID`; a strong or weak tag that is no version
/// uid names none. Nothing in the body is read.
#[must_use]
pub fn answered(read: Option<ObjectVersionId>, answer: &Forwarded) -> Vec<ObjectVersionId> {
    if !answer.status().is_success() {
        return Vec::new();
    }
    let tagged = answer
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .and_then(|tag| {
            let tag = tag.trim();
            let tag = tag.strip_prefix("W/").unwrap_or(tag);
            let tag = tag.strip_prefix('"')?.strip_suffix('"')?;
            // NOTE: §12.2, an entity tag that is no OBJECT_VERSION_ID names
            // no version, which is legitimately absent here.
            ObjectVersionId::new(tag).ok()
        });
    read.into_iter().chain(tagged).collect()
}

/// Teaches the learned map what a routed answer from `endpoint` shows it
/// holding ([`answered`]), the version `read` named among them.
pub fn learn_from(
    federation: &Federation,
    endpoint: &EndpointId,
    read: Option<ObjectVersionId>,
    answer: &Forwarded,
    logged: &str,
) {
    let seen = answered(read, answer);
    observe(
        federation,
        seen.iter().map(|version| (endpoint, version)),
        logged,
    );
}

/// Feeds every version `seen` names, at the endpoint it was seen at, to the
/// federation's learned map (§12.2, N21).
///
/// A new mapping is logged by the `creating_system_id` and the endpoint, both
/// routing ids, under `logged`; a conflict raised its integrity incident in
/// the registry already. A sighting the map refuses (an endpoint the snapshot
/// no longer holds, a `creating_system_id` that is no uid) teaches nothing.
pub fn observe<'v>(
    federation: &Federation,
    seen: impl IntoIterator<Item = (&'v EndpointId, &'v ObjectVersionId)>,
    logged: &str,
) {
    let snapshot = federation.snapshot();
    let mut learned = federation.learned();
    for (endpoint, version) in seen {
        match learned.observe(snapshot, version, endpoint) {
            Ok(Sighting::Learned(route)) => tracing::debug!(
                creating_system_id = version.creating_system_id_str(),
                endpoint = %endpoint,
                node = %route.node(),
                request_id = logged,
                "learned a route for a creating_system_id"
            ),
            Ok(_) => {}
            Err(refused) => tracing::debug!(
                endpoint = %endpoint,
                error = %refused,
                request_id = logged,
                "a sighting taught the follow-up routing table nothing"
            ),
        }
    }
}
