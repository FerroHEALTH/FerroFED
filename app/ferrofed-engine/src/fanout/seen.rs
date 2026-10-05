// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The versions a node's rows show it holding, read from the result cells
//! (§12.2, N21).
//!
//! A row that carries a `COMPOSITION` or `VERSION` uid already names the
//! system that created the version, and the registry maps every observed
//! `creating_system_id` (§12.2, N21). A cell names a version the endpoint
//! holds when it is an `OBJECT_VERSION_ID` as text (a selected `uid/value`),
//! an `OBJECT_VERSION_ID` object (a selected `uid`), or an RM object whose own
//! `uid` is one (a selected `COMPOSITION` or `VERSION`). Nothing deeper is
//! read: a reference inside an RM object names a version held elsewhere (no
//! specification governs which cells are read: our own design).

use std::collections::BTreeMap;

use ferrofed_registry::id::{EndpointId, SystemId};
use openehr_base::prelude::ObjectVersionId;
use openehr_its::rest::generated::query::ResultSetRow;

/// The RM type name of an `OBJECT_VERSION_ID` in canonical JSON.
const OBJECT_VERSION_ID: &str = "OBJECT_VERSION_ID";

/// One version seen per endpoint and `creating_system_id`, the unit the
/// learned map learns by.
pub(super) type Seen = BTreeMap<(EndpointId, SystemId), ObjectVersionId>;

/// Records in `seen` every version a cell of `rows` shows `endpoint` holding.
pub(super) fn record(seen: &mut Seen, endpoint: &EndpointId, rows: &[ResultSetRow]) {
    for version in rows.iter().flatten().filter_map(version) {
        // NOTE: §12.2, a creating_system_id that is no openEHR uid names no
        // system the registry can map, so the version teaches nothing.
        let Ok(system) = SystemId::creating_system_id_of(&version) else {
            continue;
        };
        seen.entry((endpoint.clone(), system)).or_insert(version);
    }
}

/// The version `cell` names, or `None` when it names none.
#[expect(
    clippy::disallowed_types,
    reason = "the result-cell seam: ITS-REST types a RESULT_SET cell as a JSON value"
)]
fn version(cell: &serde_json::Value) -> Option<ObjectVersionId> {
    match cell {
        // NOTE: §12.2, a text cell that is no OBJECT_VERSION_ID is a value of
        // another kind, legitimately not a version uid.
        serde_json::Value::String(text) => ObjectVersionId::new(text.as_str()).ok(),
        serde_json::Value::Object(object) => typed(object).or_else(|| {
            object
                .get("uid")
                .and_then(serde_json::Value::as_object)
                .and_then(typed)
        }),
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::Array(_) => None,
    }
}

/// The `OBJECT_VERSION_ID` `object` is, or `None` when it is another object.
#[expect(
    clippy::disallowed_types,
    reason = "the result-cell seam: ITS-REST types a RESULT_SET cell as a JSON value"
)]
fn typed(object: &serde_json::Map<String, serde_json::Value>) -> Option<ObjectVersionId> {
    if object.get("_type")?.as_str()? != OBJECT_VERSION_ID {
        return None;
    }
    // NOTE: §12.2, an OBJECT_VERSION_ID whose value does not parse names no
    // version, the same as a cell of another kind.
    ObjectVersionId::new(object.get("value")?.as_str()?).ok()
}

#[cfg(test)]
mod tests {
    use super::{Seen, record};
    use ferrofed_registry::id::EndpointId;
    use serde_json::json;

    const CREATED_AT_A: &str = "8849182c-82ad-4088-a07f-48ead4180515::cdr-a.example.org::1";
    const CREATED_ELSEWHERE: &str = "5c3e9b1a-7d2f-4e8a-9b6c-1f0e2d3c4b5a::legacy.example.org::2";

    #[expect(
        clippy::disallowed_types,
        reason = "a test builds RESULT_SET cells as JSON values"
    )]
    fn seen_at(rows: &[Vec<serde_json::Value>]) -> Vec<(String, String)> {
        let endpoint: EndpointId = "node-b-pub".parse().expect("a valid endpoint id");
        let mut seen = Seen::new();
        record(&mut seen, &endpoint, rows);
        seen.into_keys()
            .map(|(endpoint, system)| (endpoint.as_str().to_owned(), system.as_str().to_owned()))
            .collect()
    }

    #[test]
    fn a_uid_as_text_as_an_object_and_as_an_rm_objects_uid_are_each_seen() {
        let rows = vec![
            vec![json!(CREATED_AT_A)],
            vec![json!({"_type": "OBJECT_VERSION_ID", "value": CREATED_ELSEWHERE})],
            vec![
                json!({"_type": "COMPOSITION", "uid": {"_type": "OBJECT_VERSION_ID", "value": CREATED_AT_A}}),
            ],
        ];
        assert_eq!(
            vec![
                ("node-b-pub".to_owned(), "cdr-a.example.org".to_owned()),
                ("node-b-pub".to_owned(), "legacy.example.org".to_owned()),
            ],
            seen_at(&rows),
            "one sighting per endpoint and creating_system_id"
        );
    }

    #[test]
    fn other_cells_and_nested_references_are_not_seen() {
        let rows = vec![vec![
            json!(null),
            json!(7),
            json!("a free-text value"),
            json!("8849182c-82ad-4088-a07f-48ead4180515"),
            json!({"_type": "HIER_OBJECT_ID", "value": CREATED_AT_A}),
            json!({"_type": "COMPOSITION", "uid": {"_type": "HIER_OBJECT_ID", "value": "8849182c-82ad-4088-a07f-48ead4180515"}}),
            json!({"_type": "COMPOSITION", "links": [{"target": {"_type": "OBJECT_VERSION_ID", "value": CREATED_ELSEWHERE}}]}),
            json!([CREATED_AT_A]),
        ]];
        assert!(
            seen_at(&rows).is_empty(),
            "no cell names a version the endpoint holds"
        );
    }
}
