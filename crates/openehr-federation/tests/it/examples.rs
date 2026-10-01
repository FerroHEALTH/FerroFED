// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The specification's own examples (§9.4, §7a.2) read, re-emitted, and held
//! to the vendored schemas.

use openehr_federation::envelope;
use openehr_federation::meta::FederationMeta;
use openehr_federation::options::OptionsRoot;
use openehr_its::rest::generated::query::{ResultSet, ResultSetMetadata};

use crate::support;

#[test]
fn the_9_4_example_is_valid_against_the_vendored_schema() {
    let example = support::only_example("result-set.adoc").expect("§9.4 carries one example");
    support::validate_text(support::RESULT_SET_SCHEMA, &example)
        .expect("the specification's own §9.4 example validates");
}

#[test]
fn the_9_4_example_round_trips_through_the_envelope() {
    let example = support::only_example("result-set.adoc").expect("§9.4 carries one example");
    let result_set: ResultSet = serde_json::from_str(&example).expect("an ITS-REST RESULT_SET");
    let metadata = result_set.meta.as_ref().expect("the example carries meta");
    let federation = envelope::read(metadata).expect("meta.federation reads");

    let mut rebuilt = ResultSet {
        meta: Some(ResultSetMetadata {
            _href: None,
            _type: None,
            _schema_version: None,
            _created: None,
            _generator: None,
            _executed_aql: None,
            additional_properties: std::collections::BTreeMap::new(),
        }),
        ..result_set.clone()
    };
    let rebuilt_meta = rebuilt.meta.as_mut().expect("meta was just set");
    envelope::attach(&federation, rebuilt_meta).expect("meta.federation writes");
    support::validate(support::RESULT_SET_SCHEMA, &rebuilt)
        .expect("the re-emitted envelope validates");

    let emitted = serde_json::to_string(&rebuilt).expect("the envelope serializes");
    assert!(
        support::same_json(&emitted, &example).expect("both are JSON"),
        "the re-emitted §9.4 example is the same JSON as the specification's"
    );
}

#[test]
fn the_9_4_federation_record_is_complete_with_an_excluded_member() {
    let example = support::only_example("result-set.adoc").expect("§9.4 carries one example");
    let text = support::at(&example, "/meta/federation").expect("the example has meta.federation");
    let federation: FederationMeta = serde_json::from_str(&text).expect("meta.federation reads");
    assert_eq!(
        federation.endpoints().len(),
        3,
        "the example reports three endpoints"
    );
    assert!(
        federation.complete(),
        "§9.4: node_3 is excluded, never in scope, and does not clear complete"
    );
}

#[test]
fn the_7a_2_example_round_trips_unchanged() {
    let example = support::only_example("rest-facade.adoc").expect("§7a.2 carries one example");
    support::validate_text(support::OPTIONS_SCHEMA, &example)
        .expect("the specification's own §7a.2 example validates");
    let options: OptionsRoot = serde_json::from_str(&example).expect("the OPTIONS body reads");
    support::validate(support::OPTIONS_SCHEMA, &options).expect("the re-emitted body validates");
    let emitted = serde_json::to_string(&options).expect("the OPTIONS body serializes");
    assert!(
        support::same_json(&emitted, &example).expect("both are JSON"),
        "the re-emitted §7a.2 example is the same JSON as the specification's"
    );
    let again: OptionsRoot = serde_json::from_str(&emitted).expect("the emitted body reads");
    assert_eq!(
        again, options,
        "reading the emitted body gives the same value"
    );
}
