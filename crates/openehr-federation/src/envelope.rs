// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The one place `meta.federation` meets the ITS-REST `ResultSetMetadata`.
//!
//! ITS-REST declares `ResultSetMetadata` open (`additionalProperties: true`),
//! and `openehr-its` carries its undeclared members as JSON values in
//! [`ResultSetMetadata::additional_properties`]. The federation uses that
//! extension point once, for the single member `federation` (§9.1). A gateway
//! MUST NOT emit `complete`, `endpoints`, `timeout` or `dedup` directly on
//! `meta`, nor `_complete`, `_endpoints` or `_federation`, and a client MUST
//! NOT read them there (§9.1, N17, CP-35); both directions refuse them here.

use openehr_its::rest::generated::query::ResultSetMetadata;

use crate::error::WireError;
use crate::meta::FederationMeta;

/// The `meta` member that carries the federation additions.
pub const MEMBER: &str = "federation";

/// The `meta` members §9.1 forbids: the federation members flat on `meta`, and
/// the `_`-prefixed names reserved to openEHR.
pub const FORBIDDEN_ON_META: [&str; 7] = [
    "complete",
    "endpoints",
    "timeout",
    "dedup",
    "_complete",
    "_endpoints",
    "_federation",
];

/// Writes `federation` into `metadata` as `meta.federation`.
///
/// # Errors
///
/// Returns [`WireError::FlatFederationMember`] when `metadata` already holds
/// a member §9.1 forbids, [`WireError::DuplicateMember`] when it already holds
/// `federation`, and [`WireError::Envelope`] when the record cannot be
/// encoded.
pub fn attach(
    federation: &FederationMeta,
    metadata: &mut ResultSetMetadata,
) -> Result<(), WireError> {
    refuse_flat_members(metadata)?;
    if metadata.additional_properties.contains_key(MEMBER) {
        return Err(WireError::DuplicateMember {
            object: "meta",
            member: MEMBER.to_owned(),
        });
    }
    let encoded = serde_json::to_value(federation).map_err(WireError::Envelope)?;
    metadata
        .additional_properties
        .insert(MEMBER.to_owned(), encoded);
    Ok(())
}

/// Reads `meta.federation` out of `metadata`.
///
/// # Errors
///
/// Returns [`WireError::FlatFederationMember`] when `metadata` holds a member
/// §9.1 forbids, [`WireError::MissingMember`] when it has no `federation`
/// (the schema requires it of a federated result set), and
/// [`WireError::Envelope`] when the member is not a valid `meta.federation`.
pub fn read(metadata: &ResultSetMetadata) -> Result<FederationMeta, WireError> {
    refuse_flat_members(metadata)?;
    let encoded = metadata
        .additional_properties
        .get(MEMBER)
        .ok_or(WireError::MissingMember {
            object: "meta",
            member: MEMBER,
        })?;
    let text = serde_json::to_string(encoded).map_err(WireError::Envelope)?;
    serde_json::from_str(&text).map_err(WireError::Envelope)
}

fn refuse_flat_members(metadata: &ResultSetMetadata) -> Result<(), WireError> {
    match FORBIDDEN_ON_META
        .iter()
        .find(|member| metadata.additional_properties.contains_key(**member))
    {
        Some(member) => Err(WireError::FlatFederationMember {
            member: (*member).to_owned(),
        }),
        None => Ok(()),
    }
}
