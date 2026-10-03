// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Schema validation of the answer bodies against the vendored specification.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: schema validation reads JSON as values, in tests only"
)]

use std::error::Error;

use serde_json::Value;

/// The vendored result-envelope schema.
const RESULT_SET_SCHEMA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/attachments/federated-result-set.schema.json"
);

/// The vendored `OPTIONS {base}/` schema.
const OPTIONS_SCHEMA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-spec/modules/ROOT/attachments/options-root.schema.json"
);

/// Validates the JSON `text` against the result-set schema, formats
/// included.
pub(crate) fn validate(text: &str) -> Result<(), Box<dyn Error>> {
    validate_against(RESULT_SET_SCHEMA, text)
}

/// Validates the JSON `text` against the `OPTIONS {base}/` schema,
/// formats included.
pub(crate) fn validate_options(text: &str) -> Result<(), Box<dyn Error>> {
    validate_against(OPTIONS_SCHEMA, text)
}

/// Validates the JSON `text` against the schema at `path`.
fn validate_against(path: &str, text: &str) -> Result<(), Box<dyn Error>> {
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)?;
    let instance: Value = serde_json::from_str(text)?;
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|error| format!("{} at {}", error, error.instance_path()))
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{path}: {}", errors.join("; ")).into())
    }
}
