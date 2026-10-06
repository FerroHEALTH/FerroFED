// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The entries of a received document: their `fullUrl`s, and the one
//! patient every subject in the document names.
//!
//! Every rule here reads the JSON the R4 model encodes the decoded document
//! as, so it sees exactly the document the check and the mapping read.

use std::collections::BTreeSet;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;

use crate::receive::ReceiveError;
use crate::receive::references;

/// One entry of the document: its `fullUrl` and its resource.
pub(super) struct Entry<'d> {
    pub(super) full_url: Option<&'d str>,
    pub(super) resource: Option<&'d Object>,
}

impl Entry<'_> {
    /// Returns the type of the entry's resource.
    pub(super) fn resource_type(&self) -> Option<&str> {
        self.resource?.get("resourceType")?.as_str()
    }
}

/// Returns the entries of `tree`, the encoded document, in order.
pub(super) fn entries(tree: &Object) -> Vec<Entry<'_>> {
    tree.get("entry")
        .and_then(Value::as_array)
        .unwrap_or_default()
        .iter()
        .map(|entry| Entry {
            full_url: entry.get("fullUrl").and_then(Value::as_str),
            resource: entry.get("resource").and_then(Value::as_object),
        })
        .collect()
}

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
/// A `fullUrl` is not version specific (`bdl-8`), a `fullUrl` with one
/// `meta.versionId` is given once (`bdl-7`), and a `fullUrl` that looks like
/// a REST-style server URL ends with its resource's type and id: "the fullUrl
/// SHALL NOT disagree with the id in the resource"
/// (`Bundle.entry.fullUrl`, <https://hl7.org/fhir/R4/bundle-definitions.html#Bundle.entry.fullUrl>).
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
        if let Some((kind, id)) = restful(url) {
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

/// Returns the resource type and id a REST-style `fullUrl` ends with, or `None`
/// for one that does not look like a REST-style server URL.
// NOTE: a fullUrl that is a URN or another absolute URL is legitimately not
// RESTful and carries no type or id to agree with (R4 `Bundle.entry.fullUrl`).
pub(super) fn restful(url: &str) -> Option<(&str, &str)> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let mut segments = url.rsplit('/');
    let id = segments.next()?;
    let kind = segments.next()?;
    SCHEMAS.is_resource(kind).then_some((kind, id))
}

/// Returns the index of the entry `reference` names from the entry whose
/// `fullUrl` is `from`, when exactly one entry's `fullUrl` resolves it.
///
/// An absolute reference equals a `fullUrl`. A relative `Type/id` resolves
/// against the server base of the REST-style `fullUrl` of the entry that
/// holds it, and from any other entry to nothing
/// (<https://hl7.org/fhir/R4/bundle.html#references>). A local `#id` names a
/// contained resource, never an entry.
pub(super) fn resolve(entries: &[Entry<'_>], from: Option<&str>, reference: &str) -> Option<usize> {
    let target = if reference.contains(':') {
        reference.to_owned()
    } else if reference.starts_with('#') {
        return None;
    } else {
        let url = from?;
        let (kind, id) = restful(url)?;
        let base = url.strip_suffix(&format!("/{kind}/{id}"))?;
        format!("{base}/{reference}")
    };
    let mut named = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.full_url == Some(target.as_str()));
    match (named.next(), named.next()) {
        (Some((index, _)), None) => Some(index),
        _ => None,
    }
}

/// Returns the index of the `Patient` entry the document is about, after
/// holding every subject in the document to it.
///
/// The `Composition.subject` names the patient. No other entry and no
/// contained resource may be a `Patient`, and every reference of every
/// entry is held to the patient (`references`).
///
/// # Errors
///
/// Returns [`ReceiveError::NoSubject`] when the `Composition` names no
/// subject, [`ReceiveError::SubjectUnresolved`] when it names no single
/// `Patient` entry, [`ReceiveError::SeveralPatients`], and the refusals of
/// the reference walk.
// NOTE: Regulation (EU) 2025/327 Art 13(3) registers data under the identification
// data of the one person concerned, so a document that names a second subject cannot
// be registered whole (no specification states this refusal: our own design).
pub(super) fn patient(tree: &Object) -> Result<usize, ReceiveError> {
    let entries = entries(tree);
    let reference = entries
        .first()
        .and_then(|entry| entry.resource)
        .and_then(|composition| composition.get("subject"))
        .and_then(|subject| subject.get("reference"))
        .and_then(Value::as_str)
        .ok_or(ReceiveError::NoSubject)?;
    let composition = entries.first().and_then(|entry| entry.full_url);
    let patient = resolve(&entries, composition, reference)
        .filter(|index| {
            entries
                .get(*index)
                .is_some_and(|entry| entry.resource_type() == Some("Patient"))
        })
        .ok_or(ReceiveError::SubjectUnresolved)?;
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
