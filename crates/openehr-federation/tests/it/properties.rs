// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Properties over generated records: every `meta.federation` the types can
//! build validates against the schema, reads back equal, and reports
//! `complete` exactly as §11.4 defines it.

use std::collections::BTreeSet;

use openehr_federation::id::EndpointId;
use openehr_federation::meta::{DedupRecord, FederationMeta, TimeoutBudget};
use openehr_federation::object::Uri;
use openehr_federation::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
use openehr_federation::status::EndpointStatus;
use proptest::collection::{btree_set, vec};
use proptest::option;
use proptest::prelude::*;

use crate::support;

fn error() -> impl Strategy<Value = ErrorDetail> {
    "[a-z][a-z ]{0,20}".prop_map(|message| ErrorDetail::text(message).expect("a non-empty message"))
}

fn outcome() -> impl Strategy<Value = Outcome> {
    let latency = 0_u64..120_000;
    prop_oneof![
        latency
            .clone()
            .prop_map(|latency_ms| Outcome::Active { latency_ms }),
        (latency.clone(), error())
            .prop_map(|(latency_ms, error)| Outcome::Offline { latency_ms, error }),
        (latency.clone(), error())
            .prop_map(|(latency_ms, error)| Outcome::TimeOut { latency_ms, error }),
        (latency.clone(), error())
            .prop_map(|(latency_ms, error)| Outcome::NodeError { latency_ms, error }),
        error().prop_map(|error| Outcome::NotResolved { error }),
        (option::of(latency), option::of(error())).prop_map(|(latency, error)| {
            Outcome::ConsentDenied {
                refused_by: latency.map_or(ConsentRefusal::PreFilter, |latency_ms| {
                    ConsentRefusal::Node { latency_ms }
                }),
                error,
            }
        }),
        option::of(error()).prop_map(|error| Outcome::Excluded { error }),
        option::of(error()).prop_map(|error| Outcome::NotLocalized { error }),
    ]
}

fn endpoint(id: String) -> impl Strategy<Value = EndpointOutcome> {
    (
        outcome(),
        option::of("[a-z0-9_]{1,8}"),
        option::of("[a-z0-9.]{1,12}"),
        option::of("Org [A-Z]"),
        option::of(0_u64..1000),
        any::<bool>(),
    )
        .prop_map(
            move |(outcome, node_id, system_id, organisation, rows, with_url)| {
                let active = outcome.status() == EndpointStatus::Active;
                let mut entry =
                    EndpointOutcome::new(EndpointId::new(id.clone()).expect("non-empty"), outcome);
                if let Some(node_id) = node_id {
                    entry = entry.with_node_id(node_id);
                }
                if let Some(system_id) = system_id {
                    entry = entry.with_system_id(system_id);
                }
                if let Some(organisation) = organisation {
                    entry = entry.with_organisation(organisation);
                }
                if let Some(rows) = rows {
                    let rows = if active { rows } else { 0 };
                    entry = entry
                        .with_row_count(rows)
                        .expect("rows only from an active endpoint");
                }
                if with_url {
                    entry = entry.with_url(
                        Uri::new("https://cdr.example/openehr/v1").expect("an absolute URI"),
                    );
                }
                entry
            },
        )
}

fn federation() -> impl Strategy<Value = FederationMeta> {
    btree_set("node_[0-9]{1,3}", 0..6)
        .prop_flat_map(|ids: BTreeSet<String>| ids.into_iter().map(endpoint).collect::<Vec<_>>())
        .prop_flat_map(|endpoints| {
            (
                Just(endpoints),
                option::of((option::of(0_u64..10_000), option::of(0_u64..60_000))),
                option::of((option::of(0_u64..50), vec("node_[0-9]", 0..3))),
            )
        })
        .prop_map(|(endpoints, timeout, dedup)| {
            let mut federation = FederationMeta::new(endpoints).expect("distinct endpoint ids");
            if let Some((per_node_ms, overall_ms)) = timeout {
                federation = federation.with_timeout(TimeoutBudget {
                    per_node_ms,
                    overall_ms,
                    policy: Some("all-or-nothing".to_owned()),
                    ..TimeoutBudget::default()
                });
            }
            if let Some((suppressed_rows, suppressed)) = dedup {
                federation = federation.with_dedup(DedupRecord {
                    mode: Some("none".to_owned()),
                    suppressed_rows,
                    suppressed_endpoints: Some(
                        suppressed
                            .into_iter()
                            .map(|id| EndpointId::new(id).expect("non-empty"))
                            .collect(),
                    ),
                    ..DedupRecord::default()
                });
            }
            federation
        })
}

proptest! {
    #[test]
    fn every_record_validates_and_reads_back(federation in federation()) {
        let text = serde_json::to_string(&federation).expect("meta.federation serializes");
        let envelope = format!(r#"{{"rows":[],"meta":{{"federation":{text}}}}}"#);
        support::validate_text(support::RESULT_SET_SCHEMA, &envelope).expect("the envelope validates");
        let again: FederationMeta = serde_json::from_str(&text).expect("meta.federation reads");
        prop_assert_eq!(again, federation);
    }

    #[test]
    fn complete_is_derived_from_the_in_scope_statuses(federation in federation()) {
        let expected = federation
            .endpoints()
            .iter()
            .all(|endpoint| !endpoint.status().is_in_scope() || endpoint.status() == EndpointStatus::Active);
        prop_assert_eq!(federation.complete(), expected);
    }

    #[test]
    fn latency_is_present_exactly_when_dispatched(federation in federation()) {
        for endpoint in federation.endpoints() {
            let dispatched = endpoint.status().requires_latency()
                || matches!(
                    endpoint.outcome(),
                    Outcome::ConsentDenied { refused_by: ConsentRefusal::Node { .. }, .. }
                );
            prop_assert_eq!(endpoint.outcome().latency_ms().is_some(), dispatched);
        }
    }
}
