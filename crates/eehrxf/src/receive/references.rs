// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every reference of a received document, held to the one patient the
//! document is about.
//!
//! The walk follows the R4 element table (`fhir_types::r4::schema`) through
//! each entry's resource, its backbone elements, its contained resources,
//! its extensions and the `_` siblings of its primitives, so every element
//! of type `Reference` is found, at any depth, by its type and never by its
//! name alone (<https://hl7.org/fhir/R4/references.html>). An element the
//! table does not know is refused: what the walk cannot read it does not
//! pass.
//!
//! - A subject element (`subject`, `patient`, `beneficiary`, `for`) must
//!   name the document's `Patient` entry by reference.
//! - Any other reference that resolves to the patient entry is fine; one
//!   that resolves to no entry is refused when it could name a patient: its
//!   path or its `type` says `Patient`, or nothing says what it names.
//! - A reference's `type` must agree with the resource it resolves to.
//! - A local `#id` must name a resource contained in the same resource.

use std::collections::BTreeMap;

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;

use crate::receive::ReceiveError;
use crate::receive::entries::Entry;
use crate::receive::entries::resolve;
use crate::receive::entries::restful;

/// The elements R4 names the subject of care with.
// NOTE: R4 `subject` (Observation, Condition), `patient` (AllergyIntolerance),
// `beneficiary` (Coverage) and `for` (Task) each name the person a resource is about
// (<https://hl7.org/fhir/R4/references.html>); no specification lists them: our own design.
const SUBJECT_ELEMENTS: &[&str] = &["subject", "patient", "beneficiary", "for"];

/// What one walk over one entry holds the document's references to.
struct Walk<'a, 'd> {
    entries: &'a [Entry<'d>],
    holder: usize,
    patient: usize,
}

/// Holds every reference of the resource of entry `holder` to the entry
/// `patient`.
///
/// # Errors
///
/// Returns the [`ReceiveError`] of the first reference that breaks a rule of
/// the module, or of the first element the R4 table does not know.
pub(super) fn hold(
    entries: &[Entry<'_>],
    holder: usize,
    patient: usize,
    resource: &Object,
    location: &str,
) -> Result<(), ReceiveError> {
    let walk = Walk {
        entries,
        holder,
        patient,
    };
    walk.resource(resource, location, None)
}

impl Walk<'_, '_> {
    /// Walks one resource, an entry's or a contained one.
    ///
    /// `holder` is, for a contained resource, the contained resources of
    /// the resource that holds it, which a local reference inside it names
    /// (<https://hl7.org/fhir/R4/references.html#contained>).
    fn resource(
        &self,
        resource: &Object,
        location: &str,
        holder: Option<&BTreeMap<String, String>>,
    ) -> Result<(), ReceiveError> {
        let kind = resource
            .get("resourceType")
            .and_then(Value::as_str)
            .ok_or_else(|| unread(location))?;
        if kind == "Patient" && holder.is_some() {
            return Err(ReceiveError::ContainedPatient {
                location: location.to_owned(),
            });
        }
        let schema = SCHEMAS
            .type_named(kind)
            .filter(|schema| schema.path == kind)
            .ok_or_else(|| unread(location))?;
        let contained = match holder {
            Some(siblings) => siblings.clone(),
            None => resource
                .get("contained")
                .and_then(Value::as_array)
                .unwrap_or_default()
                .iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?;
                    let inner = item.get("resourceType").and_then(Value::as_str)?;
                    Some((id.to_owned(), inner.to_owned()))
                })
                .collect(),
        };
        self.object(schema, resource, location, &contained, true)
    }

    /// Walks the members of one object of type `schema`.
    fn object(
        &self,
        schema: &TypeSchema,
        node: &Object,
        location: &str,
        contained: &BTreeMap<String, String>,
        is_resource: bool,
    ) -> Result<(), ReceiveError> {
        for (key, value) in node {
            if is_resource && key == "resourceType" {
                continue;
            }
            let at = format!("{location}.{key}");
            if let Some(primitive) = key.strip_prefix('_') {
                field(schema, primitive).ok_or_else(|| unread(&at))?;
                let element = SCHEMAS.type_named("Element").ok_or_else(|| unread(&at))?;
                for (place, item) in items(&at, value) {
                    if let Some(object) = item.as_object() {
                        self.object(element, object, &place, contained, false)?;
                    }
                }
                continue;
            }
            let kind = field(schema, key).ok_or_else(|| unread(&at))?;
            for (place, item) in items(&at, value) {
                match kind {
                    Kind::Complex(name) => {
                        let object = item.as_object().ok_or_else(|| unread(&place))?;
                        if name == "Reference" {
                            self.reference(key, object, &place, contained)?;
                        }
                        let inner = SCHEMAS.type_named(name).ok_or_else(|| unread(&place))?;
                        self.object(inner, object, &place, contained, false)?;
                    }
                    Kind::Resource => {
                        let object = item.as_object().ok_or_else(|| unread(&place))?;
                        self.resource(object, &place, Some(contained))?;
                    }
                    Kind::Primitive(_) | Kind::Attribute | Kind::Xhtml => {}
                    Kind::Choice(_) => return Err(unread(&place)),
                }
            }
        }
        Ok(())
    }

    /// Holds one reference, found at the element `element`, to the patient.
    fn reference(
        &self,
        element: &str,
        reference: &Object,
        location: &str,
        contained: &BTreeMap<String, String>,
    ) -> Result<(), ReceiveError> {
        let subject = SUBJECT_ELEMENTS.contains(&element);
        let declared = reference.get("type").and_then(Value::as_str);
        let refused = || {
            if subject {
                ReceiveError::SubjectMismatch {
                    location: location.to_owned(),
                }
            } else {
                ReceiveError::PatientReference {
                    location: location.to_owned(),
                }
            }
        };
        let Some(text) = reference.get("reference").and_then(Value::as_str) else {
            return if subject || declared == Some("Patient") {
                Err(refused())
            } else {
                Ok(())
            };
        };
        if let Some(id) = text.strip_prefix('#') {
            let target = contained
                .get(id)
                .ok_or_else(|| ReceiveError::LocalUnresolved {
                    location: location.to_owned(),
                })?;
            if subject || target == "Patient" || declared.is_some_and(|kind| kind != target) {
                return Err(refused());
            }
            return Ok(());
        }
        let from = self
            .entries
            .get(self.holder)
            .and_then(|entry| entry.full_url);
        if let Some(index) = resolve(self.entries, from, text) {
            let target = self
                .entries
                .get(index)
                .and_then(Entry::resource_type)
                .ok_or_else(|| unread(location))?;
            if declared.is_some_and(|kind| kind != target) {
                return Err(ReceiveError::ReferenceType {
                    location: location.to_owned(),
                });
            }
            if (subject || target == "Patient") && index != self.patient {
                return Err(refused());
            }
            return Ok(());
        }
        let named = named_type(text);
        let may_be_patient = match (named, declared) {
            (Some(named), Some(declared)) => named != declared || named == "Patient",
            (Some(named), None) => named == "Patient",
            (None, _) => true,
        };
        if subject || may_be_patient {
            return Err(refused());
        }
        Ok(())
    }
}

/// Returns the resource type a reference's path names, `Patient` for
/// `Patient/1` or `https://example.org/fhir/Patient/1/_history/2`, or `None`
/// when its path names none, as a URN does.
fn named_type(reference: &str) -> Option<&str> {
    let path = reference
        .split_once("/_history/")
        .map_or(reference, |(path, _)| path);
    if path.starts_with("http://") || path.starts_with("https://") {
        return restful(path).map(|(kind, _)| kind);
    }
    let (kind, id) = path.split_once('/')?;
    (!id.is_empty() && !id.contains('/') && SCHEMAS.is_resource(kind)).then_some(kind)
}

/// Returns the kind the element `key` of `schema` reads, a choice element
/// resolved to the alternative its key names.
fn field(schema: &TypeSchema, key: &str) -> Option<Kind> {
    schema.fields.iter().find_map(|field| {
        if field.name == key {
            return match field.kind {
                Kind::Choice(_) => None,
                kind => Some(kind),
            };
        }
        let Kind::Choice(variants) = field.kind else {
            return None;
        };
        let suffix = key.strip_prefix(field.name)?;
        variants
            .iter()
            .find(|(candidate, _)| *candidate == suffix)
            .map(|(_, kind)| *kind)
    })
}

/// Returns the items of a member with their locations: an array's items, or
/// the one value.
fn items<'v>(at: &str, value: &'v Value) -> Vec<(String, &'v Value)> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (format!("{at}[{index}]"), item))
            .collect(),
        other => vec![(at.to_owned(), other)],
    }
}

/// Returns the refusal of an element the walk cannot read.
fn unread(location: &str) -> ReceiveError {
    ReceiveError::Unreadable {
        location: location.to_owned(),
    }
}
