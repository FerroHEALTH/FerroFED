// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A member's own exchange-format endpoint, the `native` mode: a second
//! `hl7-fhir-rest` endpoint of the node, held apart from every Query API
//! endpoint, so no query reaches it and no directive names it (N19, §7a.2,
//! §8), and refused without the identifier system the member's `ehr_id` is
//! written in (N33).
#![allow(clippy::panic, reason = "the helpers fail the test they serve")]

use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;

use crate::fixture::{ONE_NODE, one_node_with};

/// The patient summary category's code.
const SUMMARIES: &str = "Patient-Summaries";

/// The EPS composition profile.
const EPS: &str = "http://hl7.eu/fhir/eps/StructureDefinition/composition-eu-eps";

/// A native endpoint of `node-a` named `id`, with the keys `keys`.
fn native(id: &str, keys: &str) -> String {
    format!(
        "[[endpoint]]\nid = \"{id}\"\nnode = \"node-a\"\nurl = \"https://cdr-a.example.org/fhir/{id}\"\n\
         connection_type = \"hl7-fhir-rest\"\nmanaging_organisation = \"org-a\"\n{keys}\n"
    )
}

/// The keys of a native endpoint serving patient summaries.
fn serving() -> String {
    format!(
        "categories = [\"{SUMMARIES}\"]\nprofiles = [\"{EPS}\"]\nehr_id_system = \"urn:oid:2.999.4.1\""
    )
}

fn refusal(document: &str) -> LoadError {
    match RegistrySnapshot::from_toml_str(document) {
        Ok(_) => panic!("the document must refuse to load"),
        Err(error) => error,
    }
}

#[test]
fn a_native_endpoint_loads_apart_from_every_query_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot =
        RegistrySnapshot::from_toml_str(&one_node_with(&native("node-a-eps", &serving())))?;
    let id: EndpointId = "node-a-eps".parse()?;
    let node: NodeId = "node-a".parse()?;
    assert!(
        snapshot.endpoint(&id).is_none(),
        "N19, §8: a native endpoint is no Query API endpoint and no directive names it"
    );
    assert_eq!(
        snapshot
            .endpoints()
            .map(|endpoint| endpoint.id().as_str())
            .collect::<Vec<_>>(),
        ["node-a-pub"],
        "§7a.2: the endpoints a directive may name"
    );
    let held = snapshot.native_endpoint(&id).ok_or("the native endpoint")?;
    assert_eq!(held.ehr_id_system(), "urn:oid:2.999.4.1");
    assert_eq!(held.profiles(), [EPS]);
    assert_eq!(
        snapshot
            .native_for(&node, SUMMARIES)
            .map(ferrofed_registry::native::NativeEndpoint::id),
        Some(&id),
        "node-a is native for patient summaries"
    );
    assert!(
        snapshot.native_for(&node, "Laboratory-Reports").is_none(),
        "every other category stays mapped"
    );
    Ok(())
}

#[test]
fn a_suspended_native_endpoint_serves_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let keys = format!("{}\nstatus = \"suspended\"", serving());
    let snapshot = RegistrySnapshot::from_toml_str(&one_node_with(&native("node-a-eps", &keys)))?;
    assert!(snapshot.native_for(&"node-a".parse()?, SUMMARIES).is_none());
    Ok(())
}

#[test]
fn a_native_endpoint_without_an_identifier_system_is_refused() {
    let keys = format!("categories = [\"{SUMMARIES}\"]\nprofiles = [\"{EPS}\"]");
    assert!(
        matches!(
            refusal(&one_node_with(&native("node-a-eps", &keys))),
            LoadError::NativeWithoutIdentifierSystem(endpoint) if endpoint.as_str() == "node-a-eps"
        ),
        "N33: the member is located by its ehr_id alone, in a declared system"
    );
}

#[test]
fn a_native_endpoint_with_an_empty_identifier_system_is_refused() {
    let keys =
        format!("categories = [\"{SUMMARIES}\"]\nprofiles = [\"{EPS}\"]\nehr_id_system = \"\"");
    assert!(matches!(
        refusal(&one_node_with(&native("node-a-eps", &keys))),
        LoadError::NativeWithoutIdentifierSystem(_)
    ));
}

#[test]
fn a_native_endpoint_without_a_category_or_a_profile_is_refused() {
    for (keys, missing) in [
        (
            format!("profiles = [\"{EPS}\"]\nehr_id_system = \"urn:oid:2.999.4.1\""),
            "categories",
        ),
        (
            format!("categories = [\"{SUMMARIES}\"]\nehr_id_system = \"urn:oid:2.999.4.1\""),
            "profiles",
        ),
    ] {
        assert!(
            matches!(
                refusal(&one_node_with(&native("node-a-eps", &keys))),
                LoadError::NativeWithout { key, .. } if key == missing
            ),
            "{missing}"
        );
    }
}

#[test]
fn a_query_endpoint_setting_a_native_key_is_refused() {
    let document = ONE_NODE.replace(
        "managing_organisation = \"org-a\"\n",
        "managing_organisation = \"org-a\"\nehr_id_system = \"urn:oid:2.999.4.1\"\n",
    );
    assert!(matches!(
        refusal(&document),
        LoadError::MisplacedKey { endpoint, key }
            if endpoint.as_str() == "node-a-pub" && key == "ehr_id_system"
    ));
}

#[test]
fn a_native_endpoint_with_consent_refusal_codes_is_refused() {
    let keys = format!("{}\nconsent_refusal_codes = [\"CONSENT\"]", serving());
    assert!(matches!(
        refusal(&one_node_with(&native("node-a-eps", &keys))),
        LoadError::MisplacedKey {
            key: "consent_refusal_codes",
            ..
        }
    ));
}

#[test]
fn two_native_endpoints_of_one_node_for_one_category_are_refused() {
    let document = one_node_with(&format!(
        "{}\n{}",
        native("node-a-eps", &serving()),
        native("node-a-eps-2", &serving())
    ));
    assert!(matches!(
        refusal(&document),
        LoadError::NativeCategoryTwice { category, .. } if category == SUMMARIES
    ));
}

#[test]
fn a_native_endpoint_shares_no_id_with_a_query_endpoint() {
    assert!(matches!(
        refusal(&one_node_with(&native("node-a-pub", &serving()))),
        LoadError::DuplicateEndpoint(endpoint) if endpoint.as_str() == "node-a-pub"
    ));
}

#[test]
fn a_node_with_only_a_native_endpoint_has_no_endpoint() {
    let document = format!(
        "[[organisation]]\nid = \"org-a\"\n\n[[node]]\nid = \"node-a\"\norganisation = \"org-a\"\n\
         system_id = \"cdr-a.example.org\"\n\n{}",
        native("node-a-eps", &serving())
    );
    assert!(
        matches!(refusal(&document), LoadError::NodeWithoutEndpoint(node) if node.as_str() == "node-a"),
        "a native endpoint is a second endpoint, beside one a query reaches"
    );
}
