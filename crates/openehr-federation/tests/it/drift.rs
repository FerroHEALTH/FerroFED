// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Drift between the vendored schemas and the types: every member, required
//! member, enum value and header the specification names has a Rust
//! counterpart. A re-pin that adds one fails here until it is modelled.

use openehr_federation::headers;
use openehr_federation::meta::{DedupRecord, FederationMeta, TimeoutBudget};
use openehr_federation::options::{
    Aggregates, AqlBehaviour, AuthDescription, Completeness, DedupPolicy, DefinitionBehaviour,
    GatewayDescription, ItsRestAreas, Localization, MemberEndpoint, OptIn, OptionsRoot, Paging,
    TimeoutPolicy,
};
use openehr_federation::outcome::EndpointOutcome;
use openehr_federation::status::EndpointStatus;

use crate::support;

fn sorted(members: &[&str]) -> Vec<String> {
    let mut members: Vec<String> = members.iter().map(|member| (*member).to_owned()).collect();
    members.sort();
    members
}

fn assert_members(schema: &str, pointer: &str, modelled: &[&str]) {
    let (properties, _) =
        support::schema_members(schema, pointer).expect("the schema object exists");
    assert_eq!(
        sorted(modelled),
        properties,
        "{schema}#{pointer}: the type models exactly the schema's members"
    );
}

#[test]
fn the_result_set_objects_have_rust_counterparts() {
    let schema = support::RESULT_SET_SCHEMA;
    assert_members(schema, "/$defs/federationMeta", FederationMeta::MEMBERS);
    assert_members(schema, "/$defs/endpointOutcome", EndpointOutcome::MEMBERS);
    assert_members(
        schema,
        "/$defs/federationMeta/properties/timeout",
        TimeoutBudget::MEMBERS,
    );
    assert_members(
        schema,
        "/$defs/federationMeta/properties/dedup",
        DedupRecord::MEMBERS,
    );
}

#[test]
fn the_options_objects_have_rust_counterparts() {
    let schema = support::OPTIONS_SCHEMA;
    let federation = "/properties/federation";
    let member = |name: &str| format!("{federation}/properties/{name}");
    assert_members(schema, "", OptionsRoot::MEMBERS);
    assert_members(schema, federation, GatewayDescription::MEMBERS);
    assert_members(schema, &member("aql"), AqlBehaviour::MEMBERS);
    assert_members(schema, &member("dedup"), DedupPolicy::MEMBERS);
    assert_members(schema, &member("timeout"), TimeoutPolicy::MEMBERS);
    assert_members(schema, &member("completeness"), Completeness::MEMBERS);
    assert_members(
        schema,
        &format!("{}/properties/opt_in", member("completeness")),
        OptIn::MEMBERS,
    );
    assert_members(schema, &member("paging"), Paging::MEMBERS);
    assert_members(schema, &member("aggregates"), Aggregates::MEMBERS);
    assert_members(schema, &member("definition"), DefinitionBehaviour::MEMBERS);
    assert_members(schema, &member("localization"), Localization::MEMBERS);
    assert_members(schema, &member("auth"), AuthDescription::MEMBERS);
    assert_members(schema, &member("its_rest"), ItsRestAreas::MEMBERS);
    assert_members(schema, "/$defs/memberEndpoint", MemberEndpoint::MEMBERS);
}

#[test]
fn the_status_enum_matches_the_schema() {
    let schema = support::schema_enum(
        support::RESULT_SET_SCHEMA,
        "/$defs/endpointQueryStatus/enum",
    )
    .expect("the schema has the status enum");
    let modelled: Vec<String> = EndpointStatus::ALL
        .iter()
        .map(|status| status.as_str().to_owned())
        .collect();
    assert_eq!(
        modelled, schema,
        "§11.1: the closed set, in the schema's order"
    );
}

/// Every required member of every `OPTIONS` object is required by the
/// reader: the §7a.2 example stops parsing when it is removed.
#[test]
fn every_required_options_member_is_required_by_the_reader() {
    let example = support::only_example("rest-facade.adoc").expect("§7a.2 carries one example");
    let objects = [
        ("", ""),
        ("/properties/federation", "/federation"),
        ("/properties/federation/properties/aql", "/federation/aql"),
        (
            "/properties/federation/properties/dedup",
            "/federation/dedup",
        ),
        (
            "/properties/federation/properties/timeout",
            "/federation/timeout",
        ),
        (
            "/properties/federation/properties/completeness",
            "/federation/completeness",
        ),
        (
            "/properties/federation/properties/paging",
            "/federation/paging",
        ),
        (
            "/properties/federation/properties/aggregates",
            "/federation/aggregates",
        ),
        (
            "/properties/federation/properties/definition",
            "/federation/definition",
        ),
        (
            "/properties/federation/properties/localization",
            "/federation/localization",
        ),
        (
            "/properties/federation/properties/its_rest",
            "/federation/its_rest",
        ),
        ("/$defs/memberEndpoint", "/endpoints/0"),
    ];
    for (schema_pointer, instance_pointer) in objects {
        let (_, required) = support::schema_members(support::OPTIONS_SCHEMA, schema_pointer)
            .expect("the schema object exists");
        for member in required {
            let text =
                support::without(&example, &format!("{instance_pointer}/{member}")).expect("edit");
            assert!(
                serde_json::from_str::<OptionsRoot>(&text).is_err(),
                "`{instance_pointer}/{member}` is required by the schema, so the reader must refuse its absence"
            );
        }
    }
}

/// Every required member of the result-envelope objects is required by the
/// reader: the §9.4 example stops parsing when it is removed.
#[test]
fn every_required_envelope_member_is_required_by_the_reader() {
    let example = support::only_example("result-set.adoc").expect("§9.4 carries one example");
    let federation =
        support::at(&example, "/meta/federation").expect("the example has meta.federation");
    for (schema_pointer, instance_pointer) in [
        ("/$defs/federationMeta", ""),
        ("/$defs/endpointOutcome", "/endpoints/0"),
    ] {
        let (_, required) = support::schema_members(support::RESULT_SET_SCHEMA, schema_pointer)
            .expect("the schema object exists");
        for member in required {
            let text = support::without(&federation, &format!("{instance_pointer}/{member}"))
                .expect("edit");
            assert!(
                serde_json::from_str::<FederationMeta>(&text).is_err(),
                "`{instance_pointer}/{member}` is required by the schema, so the reader must refuse its absence"
            );
        }
    }
}

#[test]
fn every_federation_header_the_specification_names_is_modelled() {
    let pages = support::all_pages().expect("the vendored pages read");
    let mut named: Vec<&str> = pages
        .match_indices("openEHR-federation-")
        .filter_map(|(start, _)| {
            let rest = pages.get(start..)?;
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
                .unwrap_or(rest.len());
            rest.get(..end)
        })
        .map(|name| name.trim_end_matches('-'))
        .collect();
    named.sort_unstable();
    named.dedup();
    let mut modelled: Vec<&str> = headers::ALL.to_vec();
    modelled.sort_unstable();
    assert_eq!(
        modelled, named,
        "every header the pages name is a constant, and no other"
    );
}
