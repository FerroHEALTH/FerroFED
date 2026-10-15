// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every coded element of a resource, found by its R4 type.
//!
//! The walk follows the R4 element table (`fhir_types::r4::schema`) through
//! a resource, its backbone elements, its data types, the `_` siblings of
//! its primitives and, when asked, the resources it contains, so a `code`
//! primitive, a `Coding` and a `CodeableConcept` are found by their type and
//! never by their name alone (<https://hl7.org/fhir/R4/datatypes.html>). An
//! element the table does not know is refused: what the walk cannot read it
//! does not pass.

use fhir_types::codec::Object;
use fhir_types::codec::Value;
use fhir_types::r4::schema::SCHEMAS;
use fhir_types::schema::Kind;
use fhir_types::schema::TypeSchema;

/// One step from a resource to an element: a member name or an array index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// A member of an object.
    Key(String),
    /// An item of an array.
    Index(usize),
}

/// One coded value the walk found.
#[derive(Debug, Clone)]
pub(crate) enum Coded<'d> {
    /// A `code` primitive's value.
    Code(&'d str),
    /// A `Coding`.
    Coding(&'d Object),
    /// A `CodeableConcept`.
    Concept(&'d Object),
}

/// One coded element of a resource.
#[derive(Debug, Clone)]
pub(crate) struct Occurrence<'d> {
    /// Where it is, as an element path with indexes, such as
    /// `Bundle.entry[1].resource.code.coding[0]`.
    pub(crate) location: String,
    /// The steps from the walked object to it.
    pub(crate) steps: Vec<Step>,
    /// The R4 element path of its definition, such as `Bundle.type` or
    /// `Observation.value[x]`.
    pub(crate) element: &'static str,
    /// The value.
    pub(crate) value: Coded<'d>,
}

/// An element the R4 table does not describe, at the location it names.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{location} is an element the R4 element table does not describe")]
pub(crate) struct Unwalkable {
    /// The element path.
    pub(crate) location: String,
}

/// Returns every coded element of `resource`, found at `location`, and of
/// the resources it carries when `nested`.
pub(crate) fn coded<'d>(
    resource: &'d Object,
    location: &str,
    nested: bool,
) -> Result<Vec<Occurrence<'d>>, Unwalkable> {
    let mut walk = Walk {
        nested,
        found: Vec::new(),
    };
    walk.resource(resource, location, &[])?;
    Ok(walk.found)
}

/// One walk and what it found.
struct Walk<'d> {
    nested: bool,
    found: Vec<Occurrence<'d>>,
}

impl<'d> Walk<'d> {
    /// Walks one resource.
    fn resource(
        &mut self,
        resource: &'d Object,
        location: &str,
        steps: &[Step],
    ) -> Result<(), Unwalkable> {
        let kind = resource
            .get("resourceType")
            .and_then(Value::as_str)
            .ok_or_else(|| unwalkable(location))?;
        let schema = SCHEMAS
            .type_named(kind)
            .filter(|schema| schema.path == kind)
            .ok_or_else(|| unwalkable(location))?;
        self.object(schema, resource, location, steps, true)
    }

    /// Walks the members of one object of type `schema`.
    fn object(
        &mut self,
        schema: &TypeSchema,
        node: &'d Object,
        location: &str,
        steps: &[Step],
        is_resource: bool,
    ) -> Result<(), Unwalkable> {
        for (key, value) in node {
            if is_resource && key == "resourceType" {
                continue;
            }
            let at = format!("{location}.{key}");
            let mut here = steps.to_vec();
            here.push(Step::Key(key.clone()));
            if let Some(primitive) = key.strip_prefix('_') {
                field(schema, primitive).ok_or_else(|| unwalkable(&at))?;
                let element = SCHEMAS
                    .type_named("Element")
                    .ok_or_else(|| unwalkable(&at))?;
                for (place, path, item) in items(&at, &here, value) {
                    if let Some(object) = item.as_object() {
                        self.object(element, object, &place, &path, false)?;
                    }
                }
                continue;
            }
            let (kind, element, type_code) = field(schema, key).ok_or_else(|| unwalkable(&at))?;
            for (place, path, item) in items(&at, &here, value) {
                match kind {
                    Kind::Complex(name) => {
                        let object = item.as_object().ok_or_else(|| unwalkable(&place))?;
                        let coded = match name {
                            "Coding" => Some(Coded::Coding(object)),
                            "CodeableConcept" => Some(Coded::Concept(object)),
                            _ => None,
                        };
                        if let Some(value) = coded {
                            self.found.push(Occurrence {
                                location: place.clone(),
                                steps: path.clone(),
                                element,
                                value,
                            });
                        }
                        let inner = SCHEMAS.type_named(name).ok_or_else(|| unwalkable(&place))?;
                        self.object(inner, object, &place, &path, false)?;
                    }
                    Kind::Resource if self.nested => {
                        let object = item.as_object().ok_or_else(|| unwalkable(&place))?;
                        self.resource(object, &place, &path)?;
                    }
                    Kind::Primitive(_) if type_code == "code" => {
                        if let Some(code) = item.as_str() {
                            self.found.push(Occurrence {
                                location: place,
                                steps: path,
                                element,
                                value: Coded::Code(code),
                            });
                        }
                    }
                    Kind::Resource | Kind::Primitive(_) | Kind::Attribute | Kind::Xhtml => {}
                    Kind::Choice(_) => return Err(unwalkable(&place)),
                }
            }
        }
        Ok(())
    }
}

/// Returns the kind, the element path and the type code of the element
/// `key` of `schema`, a choice element resolved to the alternative its key
/// names.
fn field(schema: &TypeSchema, key: &str) -> Option<(Kind, &'static str, String)> {
    schema.fields.iter().find_map(|field| {
        if field.name == key {
            return match field.kind {
                Kind::Choice(_) => None,
                kind => Some((
                    kind,
                    field.path,
                    field.types.first().copied().unwrap_or_default().to_owned(),
                )),
            };
        }
        let Kind::Choice(variants) = field.kind else {
            return None;
        };
        let suffix = key.strip_prefix(field.name)?;
        variants
            .iter()
            .find(|(candidate, _)| *candidate == suffix)
            .map(|(candidate, kind)| (*kind, field.path, uncapitalized(candidate)))
    })
}

/// Returns `name` with its first letter lower case, the type code a choice
/// suffix names (`Code` is `code`).
fn uncapitalized(name: &str) -> String {
    let mut characters = name.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(characters).collect()
    })
}

/// Returns the items of a member with their locations and steps: an
/// array's items, or the one value.
fn items<'v>(at: &str, steps: &[Step], value: &'v Value) -> Vec<(String, Vec<Step>, &'v Value)> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let mut path = steps.to_vec();
                path.push(Step::Index(index));
                (format!("{at}[{index}]"), path, item)
            })
            .collect(),
        other => vec![(at.to_owned(), steps.to_vec(), other)],
    }
}

/// Returns the refusal of an element the walk cannot read.
fn unwalkable(location: &str) -> Unwalkable {
    Unwalkable {
        location: location.to_owned(),
    }
}
