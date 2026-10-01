// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `meta.federation` in the ITS-REST `ResultSetMetadata`: one unprefixed
//! member, never flat and never `_`-prefixed (§9.1, N17, CP-35).

use std::collections::BTreeMap;

use ferrofed_wire::envelope;
use ferrofed_wire::error::WireError;
use ferrofed_wire::id::EndpointId;
use ferrofed_wire::meta::{FederationMeta, TimeoutBudget};
use ferrofed_wire::outcome::{EndpointOutcome, Outcome};
use openehr_its::rest::generated::query::{ResultSet, ResultSetMetadata};

use crate::support;

fn metadata() -> ResultSetMetadata {
    ResultSetMetadata {
        _href: None,
        _type: None,
        _schema_version: None,
        _created: None,
        _generator: None,
        _executed_aql: None,
        additional_properties: BTreeMap::new(),
    }
}

fn federation() -> FederationMeta {
    let endpoint = EndpointId::new("node_1").expect("a non-empty endpoint id");
    FederationMeta::new(vec![EndpointOutcome::new(
        endpoint,
        Outcome::Active { latency_ms: 7 },
    )])
    .expect("one endpoint")
    .with_timeout(TimeoutBudget {
        per_node_ms: Some(5000),
        overall_ms: Some(15000),
        policy: Some("all-or-nothing".to_owned()),
        ..TimeoutBudget::default()
    })
}

#[test]
fn the_federation_additions_sit_under_one_unprefixed_member() {
    let mut meta = metadata();
    envelope::attach(&federation(), &mut meta).expect("meta.federation writes");
    let names: Vec<&String> = meta.additional_properties.keys().collect();
    assert_eq!(
        names,
        ["federation"],
        "§9.1: the federation contributes exactly one member"
    );

    let result_set = ResultSet {
        meta: Some(meta),
        name: None,
        q: None,
        columns: None,
        rows: Vec::new(),
    };
    let text = serde_json::to_string(&result_set).expect("the envelope serializes");
    support::validate_text(support::RESULT_SET_SCHEMA, &text).expect("the envelope validates");
    let meta_text = support::at(&text, "/meta").expect("the envelope carries meta");
    for member in support::member_names(&meta_text).expect("meta is an object") {
        assert!(
            !member.starts_with('_') && !envelope::FORBIDDEN_ON_META.contains(&member.as_str()),
            "CP-35: meta carries `{member}`"
        );
    }
}

#[test]
fn a_flat_or_prefixed_federation_member_is_refused_both_ways() {
    for member in envelope::FORBIDDEN_ON_META {
        let mut meta = metadata();
        meta.additional_properties.insert(
            member.to_owned(),
            serde_json::to_value(true).expect("a JSON value"),
        );
        assert!(
            matches!(
                envelope::attach(&federation(), &mut meta),
                Err(WireError::FlatFederationMember { .. })
            ),
            "a gateway MUST NOT emit `meta.{member}` (§9.1)"
        );
        assert!(
            matches!(
                envelope::read(&meta),
                Err(WireError::FlatFederationMember { .. })
            ),
            "a client MUST NOT read `meta.{member}` (§9.1)"
        );
    }
}

#[test]
fn a_second_federation_member_is_refused() {
    let mut meta = metadata();
    envelope::attach(&federation(), &mut meta).expect("the first write succeeds");
    assert!(
        matches!(
            envelope::attach(&federation(), &mut meta),
            Err(WireError::DuplicateMember { .. })
        ),
        "meta carries one federation member"
    );
}

#[test]
fn a_federated_result_set_without_federation_is_refused() {
    assert!(
        matches!(
            envelope::read(&metadata()),
            Err(WireError::MissingMember { .. })
        ),
        "the schema requires meta.federation"
    );
}

#[test]
fn the_record_reads_back_unchanged() {
    let mut meta = metadata();
    let written = federation();
    envelope::attach(&written, &mut meta).expect("meta.federation writes");
    let read = envelope::read(&meta).expect("meta.federation reads");
    assert_eq!(read, written, "attach and read are inverses");
}
