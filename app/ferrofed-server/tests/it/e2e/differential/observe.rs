// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The aspects of an answer and of a node journal the differential run
//! compares, and the normalisations it applies.
//!
//! An aspect is a name and a text value; two gateways differ on a step where
//! an aspect's values differ or one of them lacks it. What the specification
//! leaves to an implementation is normalised away before the comparison, and
//! [`NORMALISATIONS`] lists every such rule for the report.
#![expect(
    clippy::disallowed_types,
    reason = "the test seam: the differential run reads both gateways' answers as JSON values"
)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ferrofed_testkit::containers::API_PATH;
use ferrofed_testkit::proxy::Capture;
use serde_json::{Map, Value};
use uuid::Uuid;

use crate::e2e::PATIENT;
use crate::e2e::scenario::Reply;

/// Every normalisation the comparison applies, with the reason the
/// specification leaves the normalised value free.
pub(crate) const NORMALISATIONS: &[(&str, &str)] = &[
    (
        "row order",
        "rows are compared as a sorted multiset unless the query has an ORDER BY (AQL §4.3 leaves the order of an unordered result open)",
    ),
    (
        "latency_ms and the ITS-REST meta members other than federation",
        "timings, `_created`, `_generator`, `_executed_aql`, `_href`, `_type` and `_schema_version` describe one run of one implementation (ITS-REST RESULT_SET meta is informative)",
    ),
    (
        "the error code and message text",
        "no specification fixes an error vocabulary or message text, so only the ITS-REST `Error` shape and any `meta.federation` are compared",
    ),
    (
        "the authority of a Location header",
        "each gateway reaches the node through its own proxy, so only the path is compared",
    ),
    (
        "the text of a dispatched AQL query",
        "§7.1 fixes what a node query scopes and carries, not its spelling, so the report shows it and the comparison reads its ehr_id scope and identifier hygiene",
    ),
    (
        "the request id and the gateway's own trace and conveyance headers",
        "each gateway mints its own; no response header outside the federation headers is compared",
    ),
];

/// How a step's answer is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A query answer, whose rows are compared as a multiset.
    Rows,
    /// A query answer with an `ORDER BY`, whose rows are compared in order.
    OrderedRows,
    /// The `OPTIONS {base}/` self-description.
    Options,
    /// A routed read or write, whose body is the node's.
    Passthrough,
}

/// The aspects of one gateway's side of a step.
#[derive(Debug, Default)]
pub(crate) struct Observed {
    /// The compared aspects.
    pub(crate) aspects: BTreeMap<String, String>,
    /// What the report shows and the comparison does not read.
    pub(crate) info: BTreeMap<String, String>,
}

impl Observed {
    fn set(&mut self, aspect: impl Into<String>, value: impl Into<String>) {
        self.aspects.insert(aspect.into(), value.into());
    }

    fn note(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.info.insert(key.into(), value.into());
    }
}

/// The response header fields the comparison reads, by lower-case name.
const FIELDS: [&str; 4] = [
    "openehr-federation-endpoint",
    "openehr-federation-system-id",
    "preference-applied",
    "etag",
];

/// Reads the aspects of `reply`, as `shape` says.
pub(crate) fn answer(reply: &Reply, shape: Shape) -> Observed {
    let mut observed = Observed::default();
    observed.set("status", reply.status.as_u16().to_string());
    for name in FIELDS {
        if let Some(value) = reply.field(name) {
            observed.set(format!("header.{name}"), value);
        }
    }
    if let Some(location) = reply.field("location") {
        observed.set("header.location", path_of(location));
    }
    if reply.text.is_empty() {
        observed.set("body", "empty");
        return observed;
    }
    let Ok(body) = serde_json::from_str::<Value>(&reply.text) else {
        observed.set("body", "not JSON");
        observed.note("body", reply.text.chars().take(400).collect::<String>());
        return observed;
    };
    match shape {
        Shape::Passthrough => passthrough(&mut observed, reply, &body),
        Shape::Options => options(&mut observed, reply, &body),
        Shape::Rows | Shape::OrderedRows => {
            if is_result_set(&body) {
                result_set(&mut observed, reply, &body, shape == Shape::OrderedRows);
            } else {
                error(&mut observed, &body);
            }
        }
    }
    observed
}

/// Reads what `journal` shows reached the node `name`, whose own `ehr_id`
/// is the first of `scope` and whose other node's is the second.
pub(crate) fn node(
    observed: &mut Observed,
    name: &str,
    journal: &[Capture],
    (own, other): (Uuid, Uuid),
    authorization: &str,
) {
    let requests: Vec<String> = journal
        .iter()
        .map(|capture| {
            let path = capture.path.strip_prefix(API_PATH).unwrap_or(&capture.path);
            match &capture.query {
                Some(query) => format!("{} {path}?{query}", capture.method),
                None => format!("{} {path}", capture.method),
            }
        })
        .collect();
    observed.set(format!("{name}.requests"), list(&requests));
    let carries = |needle: &str| {
        journal
            .iter()
            .any(|capture| capture.contains(needle.as_bytes()))
    };
    let identifier = carries(&PATIENT.value()) || carries(&PATIENT.namespace());
    observed.set(
        format!("{name}.patient-identifier"),
        if identifier { "present" } else { "absent" },
    );
    let token = authorization
        .strip_prefix("Bearer ")
        .unwrap_or(authorization);
    observed.set(
        format!("{name}.caller-credential"),
        if carries(token) { "present" } else { "absent" },
    );
    let queries: Vec<String> = journal
        .iter()
        .filter(|capture| capture.method == "POST" && capture.path.ends_with("/query/aql"))
        .map(|capture| aql_of(&capture.body))
        .collect();
    if !queries.is_empty() {
        let scopes: Vec<String> = queries
            .iter()
            .map(|aql| {
                match (
                    aql.contains(&own.to_string()),
                    aql.contains(&other.to_string()),
                ) {
                    (true, false) => "own ehr_id",
                    (false, true) => "other node's ehr_id",
                    (true, true) => "both ehr_ids",
                    (false, false) => "no ehr_id",
                }
                .to_owned()
            })
            .collect();
        observed.set(format!("{name}.query-scope"), list(&scopes));
        observed.note(format!("{name}.aql"), queries.join("\n"));
    }
}

/// Returns `items` joined, or `none` when there is none.
fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_owned()
    } else {
        items.join("; ")
    }
}

/// Returns the `q` of an ITS-REST ad-hoc query body, or the body as text.
fn aql_of(body: &[u8]) -> String {
    // NOTE: a body that is not an ad-hoc query is shown as text; no specification governs this: our own design.
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("q").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_else(|| String::from_utf8_lossy(body).into_owned())
}

/// Returns the path, query included, of an absolute or relative URI.
fn path_of(location: &str) -> String {
    match location.split_once("://") {
        Some((_, rest)) => rest
            .find('/')
            .and_then(|start| rest.get(start..))
            .unwrap_or("/")
            .to_owned(),
        None => location.to_owned(),
    }
}

/// Whether `body` is an ITS-REST `RESULT_SET`.
fn is_result_set(body: &Value) -> bool {
    body.get("columns").is_some() || body.get("rows").is_some()
}

/// Reads a `RESULT_SET`.
fn result_set(observed: &mut Observed, reply: &Reply, body: &Value, ordered: bool) {
    observed.set("body", "result set");
    schema(observed, crate::facade::schema::validate(&reply.text));
    let columns: Vec<String> = body
        .get("columns")
        .and_then(Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .map(|column| {
                    let name = column.get("name").and_then(Value::as_str).unwrap_or("");
                    let path = column.get("path").and_then(Value::as_str).unwrap_or("");
                    format!("{name}({path})")
                })
                .collect()
        })
        .unwrap_or_default();
    observed.set("columns", list(&columns));
    let mut rows: Vec<String> = body
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().map(Value::to_string).collect())
        .unwrap_or_default();
    if !ordered {
        rows.sort();
    }
    observed.set("rows.count", rows.len().to_string());
    observed.set("rows", list(&rows));
    if let Some(name) = body.get("name").and_then(Value::as_str) {
        observed.set("name", name);
    }
    meta(observed, body);
}

/// Reads an error answer: its ITS-REST `Error` shape and any
/// `meta.federation` it carries.
fn error(observed: &mut Observed, body: &Value) {
    observed.set("body", "error");
    let message = body.get("message").is_some_and(Value::is_string);
    let validation = body.get("validationErrors").is_some_and(Value::is_array);
    observed.set(
        "error.its-rest-shape",
        match (message, validation) {
            (true, true) => "message and validationErrors",
            (true, false) => "message without validationErrors",
            (false, true) => "validationErrors without message",
            (false, false) => "neither message nor validationErrors",
        },
    );
    observed.note(
        "error",
        body.to_string().chars().take(600).collect::<String>(),
    );
    meta(observed, body);
}

/// Reads `meta.federation` of `body`, when present.
fn meta(observed: &mut Observed, body: &Value) {
    let Some(meta) = body.get("meta").and_then(Value::as_object) else {
        observed.set("meta", "absent");
        return;
    };
    let others: Vec<String> = meta
        .keys()
        .filter(|key| *key != "federation")
        .cloned()
        .collect();
    observed.note("meta members", list(&others));
    let Some(federation) = meta.get("federation").and_then(Value::as_object) else {
        observed.set("meta.federation", "absent");
        return;
    };
    observed.set("meta.federation.members", keys(federation));
    if let Some(complete) = federation.get("complete") {
        observed.set("meta.federation.complete", complete.to_string());
    }
    let mut endpoints: Vec<String> = federation
        .get("endpoints")
        .and_then(Value::as_array)
        .map(|endpoints| endpoints.iter().map(endpoint).collect())
        .unwrap_or_default();
    endpoints.sort();
    observed.set("meta.federation.endpoints", list(&endpoints));
}

/// Renders one `meta.federation.endpoints[]` record without its timing.
fn endpoint(record: &Value) -> String {
    let id = record.get("id").and_then(Value::as_str).unwrap_or("?");
    let status = record.get("status").and_then(Value::as_str).unwrap_or("?");
    let mut text = format!("{id}={status}");
    if let Some(rows) = record.get("row_count") {
        // NOTE: writing into a `String` cannot fail (`std::fmt::Write` for `String`).
        let _written: std::fmt::Result = write!(text, " rows={rows}");
    }
    if record.get("error").is_some() {
        text.push_str(" error");
    }
    if let Some(object) = record.as_object() {
        let extra: Vec<&str> = object
            .keys()
            .map(String::as_str)
            .filter(|key| !matches!(*key, "id" | "status" | "row_count" | "error" | "latency_ms"))
            .collect();
        if !extra.is_empty() {
            text.push_str(" +");
            text.push_str(&extra.join(","));
        }
    }
    text
}

/// Returns the sorted member names of `object`.
fn keys(object: &Map<String, Value>) -> String {
    let mut names: Vec<String> = object.keys().cloned().collect();
    names.sort();
    names.join(",")
}

/// Records a schema validation outcome.
fn schema(observed: &mut Observed, outcome: Result<(), Box<dyn std::error::Error>>) {
    match outcome {
        Ok(()) => observed.set("schema", "valid"),
        Err(error) => {
            observed.set("schema", "invalid");
            observed.note("schema", error.to_string());
        }
    }
}

/// Reads the `OPTIONS {base}/` self-description, every leaf an aspect.
fn options(observed: &mut Observed, reply: &Reply, body: &Value) {
    observed.set("body", "self-description");
    schema(
        observed,
        crate::facade::schema::validate_options(&reply.text),
    );
    let mut leaves = BTreeMap::new();
    flatten("options", body, &mut leaves);
    for (path, value) in leaves {
        observed.set(path, value);
    }
}

/// Collects every leaf of `value` under `path`, an array's items by index.
fn flatten(path: &str, value: &Value, leaves: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(object) => {
            for (key, member) in object {
                flatten(&format!("{path}.{key}"), member, leaves);
            }
        }
        Value::Array(items) if !items.is_empty() => {
            for (index, item) in items.iter().enumerate() {
                flatten(&format!("{path}[{index}]"), item, leaves);
            }
        }
        other => {
            leaves.insert(path.to_owned(), other.to_string());
        }
    }
}

/// Reads a routed answer, whose body is the node's.
fn passthrough(observed: &mut Observed, reply: &Reply, body: &Value) {
    observed.set(
        "body",
        if body.get("message").is_some() && body.get("_type").is_none() {
            "error"
        } else {
            "node content"
        },
    );
    observed.set("body.checksum", checksum(reply.text.as_bytes()));
    observed.note("body", reply.text.chars().take(400).collect::<String>());
}

/// Returns the FNV-1a 64-bit checksum of `bytes` in hexadecimal, enough to
/// tell whether two bodies are byte-identical.
fn checksum(bytes: &[u8]) -> String {
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{hash:016x}")
}
