// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What the walk reads from the document: the occurrences of an element, a
//! value at a path, whether a value carries a pattern, and the one invariant
//! form it evaluates.

use fhir_types::codec::Object;
use fhir_types::codec::Value;

use crate::dataset::constraint::CodingPattern;
use crate::dataset::constraint::Pattern;
use crate::receive::conform::Unread;

/// One occurrence of an element in the document: its location and its
/// value, absent when only its `_` sibling carries it.
pub(super) type Occurrence<'d> = (String, Option<&'d Value>);

/// The R4 `Bundle` invariants `ReceivedDocument::read` holds a document to.
pub(super) const READ: &[&str] = &["bdl-7", "bdl-8", "bdl-9", "bdl-10", "bdl-11"];

/// Returns the resource types and the element of an invariant of the
/// entry-reference form, or `None` for any other expression.
///
/// The types and the element are read from the expression, the form is
/// rebuilt from them, and the expression is admitted only when it equals the
/// rebuilt form once whitespace is set aside, so no other expression is read
/// as this one.
pub(super) fn entry_reference_rule(expression: &str) -> Option<(Vec<String>, String)> {
    let compact: String = expression.chars().filter(|c| !c.is_whitespace()).collect();
    let mut types = Vec::new();
    for part in compact.split("resource.is(").skip(1) {
        let (kind, _) = part.split_once(')')?;
        let kind = kind.to_owned();
        if !types.contains(&kind) {
            types.push(kind);
        }
    }
    let field = compact
        .strip_suffix(".reference.exists())")?
        .rsplit_once(".all(resource.")?
        .1
        .to_owned();
    if types.is_empty() || !field.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let alternatives = types
        .iter()
        .map(|kind| format!("resource.is({kind})"))
        .collect::<Vec<_>>()
        .join("or");
    let rebuilt = format!(
        "entry.where({alternatives}).empty()orentry.where({alternatives}).all(resource.{field}.reference.exists())"
    );
    (rebuilt == compact).then_some((types, field))
}

/// Returns the occurrences whose value is an object, the ones an element's
/// children are checked over.
pub(super) fn objects<'d>(found: &[Occurrence<'d>]) -> Vec<(String, &'d Object)> {
    found
        .iter()
        .filter_map(|(at, value)| {
            value
                .and_then(Value::as_object)
                .map(|object| (at.clone(), object))
        })
        .collect()
}

/// Returns the last segment of an element id, a slice name included.
pub(super) fn last_segment(id: &str) -> &str {
    id.rsplit_once('.').map_or(id, |(_, last)| last)
}

/// Returns the occurrences of the element `name` in `node`.
///
/// A choice element `value[x]` occurs as each key `value<Type>` whose type
/// the element admits; a primitive occurs where its value or its `_` sibling
/// does (<https://hl7.org/fhir/R4/json.html#primitive>).
pub(super) fn occurrences<'d>(
    node: &'d Object,
    location: &str,
    name: &str,
    types: &[String],
) -> Vec<Occurrence<'d>> {
    let keys: Vec<String> = match name.strip_suffix("[x]") {
        Some(stem) => types
            .iter()
            .map(|code| format!("{stem}{}", capitalized(code)))
            .collect(),
        None => vec![name.to_owned()],
    };
    let mut found = Vec::new();
    for key in keys {
        let at = format!("{location}.{key}");
        let values = node.get(&key);
        let siblings = node.get(&format!("_{key}"));
        match (values, siblings) {
            (Some(Value::Array(items)), _) => {
                found.extend(items.iter().enumerate().map(|(index, item)| {
                    let item = (!item.is_null()).then_some(item);
                    (format!("{at}[{index}]"), item)
                }));
            }
            (Some(value), _) => found.push((at, Some(value))),
            (None, Some(Value::Array(items))) => {
                found.extend((0..items.len()).map(|index| (format!("{at}[{index}]"), None)));
            }
            (None, Some(_)) => found.push((at, None)),
            (None, None) => {}
        }
    }
    found
}

/// Returns `code` with its first letter upper case, the form a choice key
/// takes (`valueString`).
fn capitalized(code: &str) -> String {
    let mut characters = code.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// Returns the value at a dotted `path` below `value`, the first item of an
/// array on the way.
pub(super) fn follow<'d>(value: &'d Value, path: &str) -> Option<&'d Value> {
    path.split('.').try_fold(value, |at, key| {
        let next = at.get(key)?;
        match next {
            Value::Array(items) => items.first(),
            other => Some(other),
        }
    })
}

/// Returns whether `value` carries `pattern`, or `None` when the model does
/// not read the pattern's form.
pub(super) fn matches(pattern: &Pattern, value: Option<&Value>) -> Option<bool> {
    match pattern {
        Pattern::Primitive(text) => Some(value.and_then(Value::as_str) == Some(text.as_str())),
        Pattern::Concept { codings, text } => {
            let Some(value) = value else {
                return Some(false);
            };
            let carried: &[Value] = value
                .get("coding")
                .and_then(Value::as_array)
                .unwrap_or_default();
            let codes = codings
                .iter()
                .all(|wanted| carried.iter().any(|coding| coding_matches(wanted, coding)));
            let texts = text
                .as_deref()
                .is_none_or(|wanted| value.get("text").and_then(Value::as_str) == Some(wanted));
            Some(codes && texts)
        }
        Pattern::Coding(wanted) => Some(value.is_some_and(|coding| coding_matches(wanted, coding))),
        _ => None,
    }
}

/// Returns whether `coding` carries every member `wanted` requires.
fn coding_matches(wanted: &CodingPattern, coding: &Value) -> bool {
    [
        ("system", wanted.system()),
        ("version", wanted.version()),
        ("code", wanted.code()),
        ("display", wanted.display()),
    ]
    .into_iter()
    .all(|(key, required)| {
        required.is_none_or(|required| coding.get(key).and_then(Value::as_str) == Some(required))
    })
}

/// Returns the reason a pattern the model does not read is listed under.
pub(super) fn unread_pattern(pattern: &Pattern) -> Unread {
    match pattern {
        Pattern::Unread { key } => Unread::Pattern(key.clone()),
        _ => Unread::Pattern(String::from("pattern")),
    }
}
