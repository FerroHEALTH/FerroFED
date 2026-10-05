// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A synthetic two-node federation: example domains and OIDs under the
//! `2.999` example arc, nothing real.

/// Two organisations, two nodes, three endpoints; `node-a` exposes two
/// endpoints, and `org-region` manages one endpoint of `org-a`'s node (N20
/// allows a managing organisation other than the operator).
pub(crate) const TWO_NODES: &str = r#"
[[organisation]]
id = "org-a"
name = "Hospital A"

[[organisation]]
id = "org-region"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[node.identifier]]
system = "urn:oid:2.999.1"
value = "node-a"

[[node]]
id = "node-b"
organisation = "org-region"
system_id = "2.999.20.1"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"

[[endpoint]]
id = "node-a-region"
node = "node-a"
url = "https://internal.cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-region"
status = "suspended"

[[endpoint]]
id = "node-b-pub"
node = "node-b"
url = "http://cdr-b.example.org:8080/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-region"
"#;

/// One organisation with one node and one endpoint, for refusal tests that
/// append a broken declaration.
pub(crate) const ONE_NODE: &str = r#"
[[organisation]]
id = "org-a"

[[node]]
id = "node-a"
organisation = "org-a"
system_id = "cdr-a.example.org"

[[endpoint]]
id = "node-a-pub"
node = "node-a"
url = "https://cdr-a.example.org/openehr"
connection_type = "openehr-rest-query"
managing_organisation = "org-a"
"#;

/// The two-node document with `extra` appended.
pub(crate) fn two_nodes_with(extra: &str) -> String {
    format!("{TWO_NODES}\n{extra}")
}

/// A `[[creating_system]]` mapping of `creating_system_id` to `endpoint`.
pub(crate) fn creating_system(creating_system_id: &str, endpoint: &str) -> String {
    format!(
        "[[creating_system]]\ncreating_system_id = \"{creating_system_id}\"\nendpoint = \"{endpoint}\"\n"
    )
}

/// The one-node document with `extra` appended.
pub(crate) fn one_node_with(extra: &str) -> String {
    format!("{ONE_NODE}\n{extra}")
}

/// An endpoint declaration for `node`, with `url` as its base URL.
pub(crate) fn endpoint(id: &str, node: &str, url: &str) -> String {
    format!(
        "[[endpoint]]\nid = \"{id}\"\nnode = \"{node}\"\nurl = \"{url}\"\nconnection_type = \"openehr-rest-query\"\nmanaging_organisation = \"org-a\"\n"
    )
}
