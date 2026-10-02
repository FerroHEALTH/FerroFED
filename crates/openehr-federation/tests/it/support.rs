// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The vendored specification as test input: the two schemas, the examples
//! its pages carry, and schema validation of what the crate emits.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: schema validation and semantic JSON comparison read JSON as values, in tests only"
)]

use std::error::Error;

use serde::Serialize;
use serde_json::Value;

/// The vendored specification pages and attachments, from this crate.
const SPEC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT"
);

/// The result-envelope schema file name.
pub(crate) const RESULT_SET_SCHEMA: &str = "federated-result-set.schema.json";

/// The `OPTIONS {base}/` schema file name.
pub(crate) const OPTIONS_SCHEMA: &str = "options-root.schema.json";

/// Reads a vendored schema as JSON.
pub(crate) fn schema(name: &str) -> Result<Value, Box<dyn Error>> {
    let text = std::fs::read_to_string(format!("{SPEC}/attachments/{name}"))?;
    Ok(serde_json::from_str(&text)?)
}

/// Reads a vendored specification page.
pub(crate) fn page(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!("{SPEC}/pages/{name}"))?)
}

/// Every vendored specification page, concatenated.
pub(crate) fn all_pages() -> Result<String, Box<dyn Error>> {
    let mut pages = Vec::new();
    for entry in std::fs::read_dir(format!("{SPEC}/pages"))? {
        pages.push(entry?.path());
    }
    pages.sort();
    let mut text = String::new();
    for path in pages {
        text.push_str(&std::fs::read_to_string(path)?);
    }
    Ok(text)
}

/// The `[source,json]` blocks of a page, in order.
pub(crate) fn json_examples(page_text: &str) -> Vec<String> {
    let mut examples = Vec::new();
    let mut lines = page_text.lines();
    while let Some(line) = lines.next() {
        if line.trim() != "[source,json]" {
            continue;
        }
        if lines.next().map(str::trim) != Some("----") {
            continue;
        }
        let body: Vec<&str> = lines.by_ref().take_while(|l| l.trim() != "----").collect();
        examples.push(body.join("\n"));
    }
    examples
}

/// The one `[source,json]` example of a page.
pub(crate) fn only_example(name: &str) -> Result<String, Box<dyn Error>> {
    let mut examples = json_examples(&page(name)?);
    match (examples.pop(), examples.is_empty()) {
        (Some(example), true) => Ok(example),
        _ => Err(format!("{name} does not carry exactly one JSON example").into()),
    }
}

/// Validates `instance` against the named vendored schema, formats included.
pub(crate) fn validate(schema_name: &str, instance: &impl Serialize) -> Result<(), Box<dyn Error>> {
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema(schema_name)?)?;
    let instance = serde_json::to_value(instance)?;
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{schema_name}: {}", errors.join("; ")).into())
    }
}

/// Validates JSON text against the named vendored schema.
pub(crate) fn validate_text(schema_name: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let instance: Value = serde_json::from_str(text)?;
    validate(schema_name, &instance)
}

/// Whether two JSON texts are the same JSON value, member order aside.
pub(crate) fn same_json(left: &str, right: &str) -> Result<bool, Box<dyn Error>> {
    let left: Value = serde_json::from_str(left)?;
    let right: Value = serde_json::from_str(right)?;
    Ok(left == right)
}

/// The JSON text of `value` with the member at `pointer` removed.
pub(crate) fn without(text: &str, pointer: &str) -> Result<String, Box<dyn Error>> {
    let mut value: Value = serde_json::from_str(text)?;
    let (parent, member) = pointer
        .rsplit_once('/')
        .ok_or_else(|| format!("{pointer} is not a JSON pointer"))?;
    value
        .pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .and_then(|object| object.remove(member))
        .ok_or_else(|| format!("{pointer} is not in the example"))?;
    Ok(serde_json::to_string(&value)?)
}

/// The JSON text of `value` with the member at `pointer` set to `json`.
pub(crate) fn with(text: &str, pointer: &str, json: &str) -> Result<String, Box<dyn Error>> {
    let mut value: Value = serde_json::from_str(text)?;
    let (parent, member) = pointer
        .rsplit_once('/')
        .ok_or_else(|| format!("{pointer} is not a JSON pointer"))?;
    let replacement: Value = serde_json::from_str(json)?;
    value
        .pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("{parent} is not an object in the example"))?
        .insert(member.to_owned(), replacement);
    Ok(serde_json::to_string(&value)?)
}

/// The JSON text at `pointer` inside `text`.
pub(crate) fn at(text: &str, pointer: &str) -> Result<String, Box<dyn Error>> {
    let value: Value = serde_json::from_str(text)?;
    let found = value
        .pointer(pointer)
        .ok_or_else(|| format!("{pointer} is not in the document"))?;
    Ok(serde_json::to_string(found)?)
}

/// The member names of the schema object at `pointer`: its `properties`
/// keys and its `required` list.
pub(crate) fn schema_members(
    schema_name: &str,
    pointer: &str,
) -> Result<(Vec<String>, Vec<String>), Box<dyn Error>> {
    let schema = schema(schema_name)?;
    let object = schema
        .pointer(pointer)
        .ok_or_else(|| format!("{schema_name} has no {pointer}"))?;
    let mut properties: Vec<String> = object
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().cloned().collect())
        .unwrap_or_default();
    properties.sort();
    let mut required: Vec<String> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    required.sort();
    Ok((properties, required))
}

/// The string values of the schema `enum` at `pointer`.
pub(crate) fn schema_enum(schema_name: &str, pointer: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let schema = schema(schema_name)?;
    let values = schema
        .pointer(pointer)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{schema_name} has no enum at {pointer}"))?;
    Ok(values
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect())
}

/// The top-level member names of the JSON object `text`.
pub(crate) fn member_names(text: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let value: Value = serde_json::from_str(text)?;
    let object = value.as_object().ok_or("not a JSON object")?;
    Ok(object.keys().cloned().collect())
}
