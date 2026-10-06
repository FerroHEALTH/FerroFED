// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entries of a received document: their resource types, their
//! `fullUrl`s, and the one patient the document is about.
//!
//! Every rule here reads the JSON the R4 model encodes the decoded document
//! as, so it sees exactly the document the check and the mapping read, and
//! every reference it follows goes through the one resolver
//! (`receive::reference`).

use std::collections::BTreeSet;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;

use crate::receive::ReceiveError;
use crate::receive::reference::ReferenceError;
use crate::receive::reference::Target;
use crate::receive::reference::Url;
use crate::receive::reference::entries;
use crate::receive::reference::resolve_in;
use crate::receive::references;

/// Refuses a resource, anywhere in the document, whose `resourceType` R4
/// does not define.
///
/// The R4 model keeps such a resource as an opaque body it decodes nothing
/// of, so no rule, check or mapping could read what it holds
/// (<https://hl7.org/fhir/R4/resourcelist.html>).
///
/// # Errors
///
/// Returns [`ReceiveError::UnknownResource`] at the first such resource.
pub(super) fn known_resources(node: &Object, location: &str) -> Result<(), ReceiveError> {
    if let Some(kind) = node.get("resourceType").and_then(Value::as_str)
        && !SCHEMAS.is_resource(kind)
    {
        return Err(ReceiveError::UnknownResource {
            location: location.to_owned(),
        });
    }
    for (key, value) in node {
        match value {
            Value::Object(object) => known_resources(object, &format!("{location}.{key}"))?,
            Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    if let Some(object) = item.as_object() {
                        known_resources(object, &format!("{location}.{key}[{index}]"))?;
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Holds every entry's `fullUrl` to the R4 `Bundle` rules.
///
/// A `fullUrl` is spelled in the one canonical form the resolver reads, it
/// is not version specific (`bdl-8`), a `fullUrl` with one `meta.versionId`
/// is given once (`bdl-7`), and a REST-style one ends with its resource's
/// type and id: "the fullUrl SHALL NOT disagree with the id in the
/// resource" (`Bundle.entry.fullUrl`,
/// <https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.fullUrl>).
///
/// # Errors
///
/// Returns the [`ReceiveError`] of the first entry that breaks a rule.
pub(super) fn full_urls(tree: &Object) -> Result<(), ReceiveError> {
    let mut seen = BTreeSet::new();
    for (index, entry) in entries(tree).iter().enumerate() {
        let Some(url) = entry.full_url else {
            continue;
        };
        let parsed = Url::parse(url).ok_or(ReceiveError::NonCanonicalFullUrl { entry: index })?;
        if url.contains("/_history/") {
            return Err(ReceiveError::VersionedFullUrl { entry: index });
        }
        let version = entry
            .resource
            .and_then(|resource| resource.get("meta"))
            .and_then(|meta| meta.get("versionId"))
            .and_then(Value::as_str);
        if !seen.insert((url, version)) {
            return Err(ReceiveError::DuplicateFullUrl { entry: index });
        }
        if let Url::Rest { kind, id, .. } = parsed {
            let resource_id = entry
                .resource
                .and_then(|resource| resource.get("id"))
                .and_then(Value::as_str);
            if entry.resource_type() != Some(kind) || resource_id != Some(id) {
                return Err(ReceiveError::FullUrlMismatch { entry: index });
            }
        }
    }
    Ok(())
}

/// Returns the index of the `Patient` entry the document is about, after
/// holding every reference in the document to it.
///
/// The `Composition.subject` names the patient, resolved from the
/// composition's entry. No other entry and no contained resource may be a
/// `Patient`, and every reference of every entry is held to the patient
/// (`references`).
///
/// # Errors
///
/// Returns [`ReceiveError::NoSubject`] when the `Composition` names no
/// subject, [`ReceiveError::SubjectUnresolved`] when it resolves to no
/// `Patient` entry, [`ReceiveError::SeveralPatients`], and the refusals of
/// the reference walk.
// NOTE: Regulation (EU) 2025/327 Art 13(3) registers data under the identification
// data of the one person concerned, so a document that names a second subject cannot
// be registered whole (no specification states this refusal: our own design).
pub(super) fn patient(tree: &Object) -> Result<usize, ReceiveError> {
    let entries = entries(tree);
    let subject = entries
        .first()
        .and_then(|entry| entry.resource)
        .and_then(|composition| composition.get("subject"))
        .and_then(Value::as_object)
        .ok_or(ReceiveError::NoSubject)?;
    let patient = match resolve_in(&entries, 0, subject) {
        Ok(Target::Entry(index))
            if entries
                .get(index)
                .is_some_and(|entry| entry.resource_type() == Some("Patient")) =>
        {
            index
        }
        Err(ReferenceError::Unreferenced { .. }) => return Err(ReceiveError::NoSubject),
        _ => return Err(ReceiveError::SubjectUnresolved),
    };
    if let Some((other, _)) = entries
        .iter()
        .enumerate()
        .find(|(index, entry)| *index != patient && entry.resource_type() == Some("Patient"))
    {
        return Err(ReceiveError::SeveralPatients { entry: other });
    }
    for (index, entry) in entries.iter().enumerate() {
        if let Some(resource) = entry.resource {
            let location = format!("Bundle.entry[{index}].resource");
            references::hold(&entries, index, patient, resource, &location)?;
        }
    }
    Ok(patient)
}
