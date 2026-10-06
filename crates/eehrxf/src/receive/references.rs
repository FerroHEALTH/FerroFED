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
//! pass. Each reference is resolved by the one resolver
//! (`receive::reference`), and the outcome is held to these rules:
//!
//! - A subject element (`subject`, `patient`, `beneficiary`, `for`) must
//!   resolve to the document's `Patient` entry.
//! - Any other reference resolves to an entry or a contained resource; or
//!   it names, by its canonical path, a resource outside the document whose
//!   type is not `Patient` and agrees with its `type`; or it names its target
//!   by `display` alone, or by `identifier` under a declared `type` other
//!   than `Patient`. Every other outcome of the resolver is refused.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;

use crate::receive::ReceiveError;
use crate::receive::reference::Entry;
use crate::receive::reference::ReferenceError;
use crate::receive::reference::Target;
use crate::receive::reference::resolve_in;

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
    walk.resource(resource, location, false)
}

impl Walk<'_, '_> {
    /// Walks one resource, the entry's own or one it contains.
    fn resource(
        &self,
        resource: &Object,
        location: &str,
        contained: bool,
    ) -> Result<(), ReceiveError> {
        let kind = resource
            .get("resourceType")
            .and_then(Value::as_str)
            .ok_or_else(|| unread(location))?;
        if kind == "Patient" && contained {
            return Err(ReceiveError::ContainedPatient {
                location: location.to_owned(),
            });
        }
        let schema = SCHEMAS
            .type_named(kind)
            .filter(|schema| schema.path == kind)
            .ok_or_else(|| unread(location))?;
        self.object(schema, resource, location, true)
    }

    /// Walks the members of one object of type `schema`.
    fn object(
        &self,
        schema: &TypeSchema,
        node: &Object,
        location: &str,
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
                        self.object(element, object, &place, false)?;
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
                            self.reference(key, object, &place)?;
                        }
                        let inner = SCHEMAS.type_named(name).ok_or_else(|| unread(&place))?;
                        self.object(inner, object, &place, false)?;
                    }
                    Kind::Resource => {
                        let object = item.as_object().ok_or_else(|| unread(&place))?;
                        self.resource(object, &place, true)?;
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
    ) -> Result<(), ReceiveError> {
        let subject = SUBJECT_ELEMENTS.contains(&element);
        let declared = reference.get("type").and_then(Value::as_str);
        let resolved = resolve_in(self.entries, self.holder, reference);
        let admitted = match resolved {
            Ok(Target::Entry(index)) => {
                let patient =
                    self.entries.get(index).and_then(Entry::resource_type) == Some("Patient");
                index == self.patient || (!subject && !patient)
            }
            Ok(Target::Contained { .. }) => !subject,
            Err(ReferenceError::Unreferenced { identifier }) => {
                !subject && declared != Some("Patient") && (!identifier || declared.is_some())
            }
            Err(ReferenceError::NotInBundle {
                kind: Some(ref kind),
            }) => !subject && kind != "Patient" && declared.is_none_or(|declared| declared == kind),
            Err(_) => false,
        };
        if admitted {
            return Ok(());
        }
        Err(match resolved {
            Err(source) if !subject => ReceiveError::Reference {
                location: location.to_owned(),
                source,
            },
            _ => ReceiveError::SubjectMismatch {
                location: location.to_owned(),
            },
        })
    }
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
