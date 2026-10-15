// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The members the operator console holds, joined to the document's: each
//! is owned whole, one that shares an identity with the document's is taken
//! by the document and named, and two console members cannot share one.

use std::error::Error;

use ferrofed_registry::console::ConsoleMember;
use ferrofed_registry::error::LoadError;
use ferrofed_registry::id::{EndpointId, NodeId};
use ferrofed_registry::snapshot::RegistrySnapshot;

type TestResult = Result<(), Box<dyn Error>>;

/// The document's one member, A.
const DOCUMENT: &str = r#"
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

/// A console member named `name` at the base URL `url`, its organisation
/// `organisation`.
fn member(name: &str, organisation: &str, url: &str) -> Result<ConsoleMember, Box<dyn Error>> {
    Ok(toml::from_str(&format!(
        r#"
organisation = {{ id = "{organisation}", name = "Organisation <{name}>" }}
node = {{ id = "node-{name}", system_id = "cdr-{name}.example.org" }}
endpoints = [{{ id = "node-{name}-pub", url = "{url}", connection_type = "openehr-rest-query" }}]
"#
    ))?)
}

fn joined(
    members: &[ConsoleMember],
) -> Result<ferrofed_registry::snapshot::join::Joined, LoadError> {
    RegistrySnapshot::from_toml_str(DOCUMENT)?
        .with_console(members.iter().map(ConsoleMember::document))
}

#[test]
fn a_console_member_joins_the_documents_and_is_marked_the_consoles() -> TestResult {
    let c = member("c", "org-c", "https://cdr-c.example.org/openehr")?;
    let joined = joined(&[c])?;
    assert!(joined.shadowed.is_empty());
    let snapshot = joined.snapshot;
    assert_eq!(2, snapshot.nodes().count());
    assert!(snapshot.is_console(&"node-c".parse::<NodeId>()?));
    assert!(!snapshot.is_console(&"node-a".parse::<NodeId>()?));
    assert!(
        snapshot
            .endpoint(&"node-c-pub".parse::<EndpointId>()?)
            .is_some()
    );
    Ok(())
}

#[test]
fn the_document_takes_a_console_member_that_shares_an_identity_and_names_it() -> TestResult {
    let same_node: ConsoleMember = toml::from_str(
        r#"
organisation = { id = "org-z" }
node = { id = "node-a", system_id = "cdr-z.example.org" }
endpoints = [{ id = "node-z-pub", url = "https://cdr-z.example.org/openehr", connection_type = "openehr-rest-query" }]
"#,
    )?;
    let same_organisation = member("d", "org-a", "https://cdr-d.example.org/openehr")?;
    let same_url = member("e", "org-e", "https://cdr-a.example.org/openehr")?;
    let free = member("f", "org-f", "https://cdr-f.example.org/openehr")?;
    let joined = joined(&[same_node, same_organisation, same_url, free])?;
    let shadowed: Vec<&str> = joined.shadowed.iter().map(NodeId::as_str).collect();
    assert_eq!(vec!["node-a", "node-d", "node-e"], shadowed);
    let snapshot = joined.snapshot;
    assert!(!snapshot.is_console(&"node-a".parse::<NodeId>()?));
    assert!(snapshot.is_console(&"node-f".parse::<NodeId>()?));
    assert_eq!(2, snapshot.nodes().count());
    Ok(())
}

#[test]
fn two_console_members_may_not_share_an_identity() -> TestResult {
    let c = member("c", "org-c", "https://cdr-c.example.org/openehr")?;
    let twin = member("d", "org-d", "https://cdr-c.example.org/openehr")?;
    assert!(matches!(
        joined(&[c.clone(), twin]),
        Err(LoadError::DuplicateEndpointUrl { .. })
    ));
    let mut renamed = member("d", "org-c", "https://cdr-d.example.org/openehr")?;
    renamed.organisation.name = Some(String::from("another name"));
    assert!(matches!(
        joined(&[c.clone(), renamed]),
        Err(LoadError::DuplicateOrganisation(_))
    ));
    let mut sharing = member("d", "org-c", "https://cdr-d.example.org/openehr")?;
    sharing.organisation.name.clone_from(&c.organisation.name);
    let joined = joined(&[c, sharing])?;
    assert_eq!(3, joined.snapshot.nodes().count());
    Ok(())
}

#[test]
fn a_console_member_that_breaks_a_membership_rule_is_refused() -> TestResult {
    let mut c = member("c", "org-c", "https://u:pw@cdr-c.example.org/openehr")?;
    assert!(matches!(
        joined(std::slice::from_ref(&c)),
        Err(LoadError::EndpointUrl { .. })
    ));
    c.endpoints.clear();
    assert!(matches!(
        joined(&[c]),
        Err(LoadError::NodeWithoutEndpoint(_))
    ));
    Ok(())
}

#[test]
fn a_console_member_round_trips_through_toml_and_never_prints_its_url() -> TestResult {
    let c = member(
        "c",
        "org-c",
        "https://cdr-c.example.org/openehr?token=synthetic-7",
    )?;
    let written = toml::to_string(&c)?;
    let read: ConsoleMember = toml::from_str(&written)?;
    assert_eq!(c, read);
    assert!(!format!("{c:?}").contains("synthetic-7"));
    Ok(())
}
