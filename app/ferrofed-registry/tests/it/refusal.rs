// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every document that breaks a membership rule refuses to load, and the
//! refusal names the rule (N19, N20, N21, §12b.2).
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

use ferrofed_registry::error::{LoadError, Referrer, UrlFault};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::fixture::{ONE_NODE, endpoint, one_node_with};

fn refusal(document: &str) -> LoadError {
    match RegistrySnapshot::from_toml_str(document) {
        Ok(_) => panic!("the document must refuse to load"),
        Err(error) => error,
    }
}

/// The parser's message for a document of the wrong shape.
fn shape_refusal(document: &str) -> String {
    match refusal(document) {
        LoadError::Parse(source) => source.to_string(),
        other => panic!("expected a shape refusal, got {other:?}"),
    }
}

#[test]
fn the_one_node_fixture_loads() {
    assert!(
        RegistrySnapshot::from_toml_str(ONE_NODE).is_ok(),
        "the base of every refusal below is itself valid"
    );
}

#[test]
fn a_node_naming_an_undeclared_organisation_is_refused() {
    let document = ONE_NODE.replace("organisation = \"org-a\"", "organisation = \"org-x\"");
    assert!(
        matches!(
            refusal(&document),
            LoadError::UnknownOrganisation { referrer: Referrer::Node(node), organisation }
                if node.as_str() == "node-a" && organisation.as_str() == "org-x"
        ),
        "a dangling node.organisation"
    );
}

#[test]
fn an_endpoint_naming_an_undeclared_node_is_refused() {
    let document = one_node_with(&endpoint(
        "ghost-pub",
        "node-ghost",
        "https://ghost.example.org/",
    ));
    assert!(
        matches!(
            refusal(&document),
            LoadError::UnknownNode { endpoint, node }
                if endpoint.as_str() == "ghost-pub" && node.as_str() == "node-ghost"
        ),
        "a dangling endpoint.node"
    );
}

#[test]
fn an_endpoint_naming_an_undeclared_managing_organisation_is_refused() {
    let document = ONE_NODE.replace(
        "managing_organisation = \"org-a\"",
        "managing_organisation = \"org-x\"",
    );
    assert!(
        matches!(
            refusal(&document),
            LoadError::UnknownOrganisation { referrer: Referrer::Endpoint(endpoint), organisation }
                if endpoint.as_str() == "node-a-pub" && organisation.as_str() == "org-x"
        ),
        "a dangling endpoint.managing_organisation"
    );
}

#[test]
fn an_endpoint_with_no_managing_organisation_is_refused() {
    // N20: every endpoint has exactly one managing organisation (1..1).
    let document = ONE_NODE.replace("managing_organisation = \"org-a\"\n", "");
    let message = shape_refusal(&document);
    assert!(
        message.contains("managing_organisation"),
        "the missing field is named: {message}"
    );
}

#[test]
fn an_endpoint_with_two_managing_organisations_is_refused() {
    // N20: a list of two is not one.
    let document = ONE_NODE.replace(
        "managing_organisation = \"org-a\"",
        "managing_organisation = [\"org-a\", \"org-a\"]",
    );
    let message = shape_refusal(&document);
    assert!(
        message.contains("invalid type"),
        "a list is refused: {message}"
    );
}

#[test]
fn an_endpoint_repeating_the_managing_organisation_key_is_refused() {
    let document = ONE_NODE.replace(
        "managing_organisation = \"org-a\"",
        "managing_organisation = \"org-a\"\nmanaging_organisation = \"org-a\"",
    );
    let message = shape_refusal(&document);
    assert!(
        message.contains("duplicate key"),
        "a repeated key is refused: {message}"
    );
}

#[test]
fn a_shared_system_id_is_refused() {
    // §12b.2: a node's system_id is unique across the federation.
    let document = one_node_with(
        "[[node]]\nid = \"node-b\"\norganisation = \"org-a\"\nsystem_id = \"cdr-a.example.org\"\n",
    );
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateSystemId { system_id, first, second }
                if system_id.as_str() == "cdr-a.example.org"
                    && first.as_str() == "node-a"
                    && second.as_str() == "node-b"
        ),
        "two nodes, one system_id"
    );
}

#[test]
fn a_system_id_shared_up_to_ascii_case_is_refused() {
    // master05 §"Composite Identifiers and Case": the two are one identifier.
    let document = one_node_with(
        "[[node]]\nid = \"node-b\"\norganisation = \"org-a\"\nsystem_id = \"CDR-A.example.org\"\n",
    );
    assert!(
        matches!(refusal(&document), LoadError::DuplicateSystemId { .. }),
        "the same system_id in another case"
    );
}

#[test]
fn a_repeated_organisation_id_is_refused() {
    let document = one_node_with("[[organisation]]\nid = \"org-a\"\n");
    assert!(
        matches!(refusal(&document), LoadError::DuplicateOrganisation(id) if id.as_str() == "org-a"),
        "two organisations, one id"
    );
}

#[test]
fn a_repeated_node_id_is_refused() {
    let document = one_node_with(
        "[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\nsystem_id = \"cdr-b.example.org\"\n",
    );
    assert!(
        matches!(refusal(&document), LoadError::DuplicateNode(id) if id.as_str() == "node-a"),
        "two nodes, one node_id"
    );
}

#[test]
fn a_repeated_endpoint_id_is_refused() {
    let document = one_node_with(&endpoint(
        "node-a-pub",
        "node-a",
        "https://other.example.org/",
    ));
    assert!(
        matches!(refusal(&document), LoadError::DuplicateEndpoint(id) if id.as_str() == "node-a-pub"),
        "two endpoints, one endpoint_id"
    );
}

#[test]
fn a_node_identifier_on_two_nodes_is_refused() {
    let document = format!(
        "{ONE_NODE}\n[[node.identifier]]\nsystem = \"urn:oid:2.999.1\"\nvalue = \"x\"\n\n\
         [[node]]\nid = \"node-b\"\norganisation = \"org-a\"\nsystem_id = \"cdr-b.example.org\"\n\n\
         [[node.identifier]]\nsystem = \"urn:oid:2.999.1\"\nvalue = \"x\"\n\n{}",
        endpoint("node-b-pub", "node-b", "https://cdr-b.example.org/openehr")
    );
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateNodeIdentifier { first, second, .. }
                if first.as_str() == "node-a" && second.as_str() == "node-b"
        ),
        "one identifier names two nodes"
    );
}

#[test]
fn an_empty_node_identifier_is_refused() {
    let document =
        one_node_with("[[node.identifier]]\nsystem = \"urn:oid:2.999.1\"\nvalue = \"\"\n");
    assert!(
        matches!(refusal(&document), LoadError::EmptyNodeIdentifier(id) if id.as_str() == "node-a"),
        "an empty value"
    );
}

#[test]
fn a_fhir_rest_connection_type_is_refused() {
    // §15.2: an openEHR endpoint MUST NOT rely on hl7-fhir-rest.
    let document = ONE_NODE.replace("openehr-rest-query", "hl7-fhir-rest");
    let message = shape_refusal(&document);
    assert!(
        message.contains("hl7-fhir-rest"),
        "the code is named: {message}"
    );
}

#[test]
fn an_unusable_base_url_is_refused() {
    let cases = [
        ("not a url", "parse"),
        ("ftp://cdr.example.org/openehr", "scheme"),
        ("https://user:secret@cdr.example.org/openehr", "credentials"),
        ("https://cdr.example.org/openehr?x=1", "query"),
        ("https://cdr.example.org/openehr#top", "fragment"),
    ];
    for (url, case) in cases {
        let document = ONE_NODE.replace("https://cdr-a.example.org/openehr", url);
        let fault = match refusal(&document) {
            LoadError::EndpointUrl { endpoint, fault } => {
                assert_eq!(
                    endpoint.as_str(),
                    "node-a-pub",
                    "the endpoint is named ({case})"
                );
                fault
            }
            other => panic!("expected an URL refusal for {case}, got {other:?}"),
        };
        let expected = match case {
            "parse" => matches!(fault, UrlFault::Parse(_)),
            "scheme" => matches!(fault, UrlFault::Scheme(ref s) if s == "ftp"),
            "credentials" => matches!(fault, UrlFault::Credentials),
            _ => matches!(fault, UrlFault::NotABase),
        };
        assert!(expected, "the {case} fault, got {fault:?}");
    }
}

#[test]
fn the_url_refusal_never_echoes_a_credential() {
    let document = ONE_NODE.replace(
        "https://cdr-a.example.org/openehr",
        "https://user:hunter2@cdr.example.org/openehr",
    );
    let error = refusal(&document);
    let rendered = format!("{error} {error:?}");
    assert!(
        !rendered.contains("hunter2"),
        "no password in the error: {rendered}"
    );
}

#[test]
fn a_base_url_shared_by_two_endpoints_is_refused() {
    let document = one_node_with(&endpoint(
        "node-a-alt",
        "node-a",
        "https://cdr-a.example.org/openehr",
    ));
    assert!(
        matches!(
            refusal(&document),
            LoadError::DuplicateEndpointUrl { first, second, .. }
                if first.as_str() == "node-a-pub" && second.as_str() == "node-a-alt"
        ),
        "one interface under two ids"
    );
}

#[test]
fn a_node_without_an_endpoint_is_refused() {
    let document = one_node_with(
        "[[node]]\nid = \"node-b\"\norganisation = \"org-a\"\nsystem_id = \"cdr-b.example.org\"\n",
    );
    assert!(
        matches!(refusal(&document), LoadError::NodeWithoutEndpoint(id) if id.as_str() == "node-b"),
        "a node nothing could reach"
    );
}

#[test]
fn a_document_with_no_node_is_refused() {
    assert!(
        matches!(
            refusal("[[organisation]]\nid = \"org-a\"\n"),
            LoadError::NoNode
        ),
        "an empty federation"
    );
    assert!(
        matches!(refusal(""), LoadError::NoNode),
        "an empty document"
    );
}

#[test]
fn an_unknown_field_is_refused_at_every_level() {
    let cases = [
        format!("{ONE_NODE}\nmembers = 3\n"),
        ONE_NODE.replace("id = \"org-a\"", "id = \"org-a\"\nwebsite = \"x\""),
        ONE_NODE.replace(
            "system_id = \"cdr-a.example.org\"",
            "system_id = \"cdr-a.example.org\"\nsystemid = \"x\"",
        ),
        one_node_with(
            "[[node.identifier]]\nsystem = \"urn:oid:2.999.1\"\nvalue = \"x\"\nuse = \"official\"\n",
        ),
        ONE_NODE.replace(
            "connection_type = \"openehr-rest-query\"",
            "connection_type = \"openehr-rest-query\"\ntimeout = 5",
        ),
    ];
    for document in &cases {
        let message = shape_refusal(document);
        assert!(
            message.contains("unknown field"),
            "deny_unknown_fields: {message}"
        );
    }
}

#[test]
fn a_malformed_identifier_is_refused_at_load() {
    let cases = [
        ONE_NODE.replace("id = \"node-a\"", "id = \"node a\""),
        ONE_NODE.replace("id = \"node-a-pub\"", "id = \"-node-a-pub\""),
        ONE_NODE.replace(
            "system_id = \"cdr-a.example.org\"",
            "system_id = \"not a uid\"",
        ),
    ];
    for document in &cases {
        let message = shape_refusal(document);
        assert!(!message.is_empty(), "a refusal with a message");
    }
}
