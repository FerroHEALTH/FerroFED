// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The per-endpoint record and `complete`: the §9.5 and §11.1 rules a schema
//! cannot state, and the conditionals it can, asserted on both directions.

use ferrofed_wire::error::WireError;
use ferrofed_wire::id::EndpointId;
use ferrofed_wire::meta::FederationMeta;
use ferrofed_wire::outcome::{ConsentRefusal, EndpointOutcome, ErrorDetail, Outcome};
use ferrofed_wire::status::EndpointStatus;

use crate::support;

fn id(text: &str) -> EndpointId {
    EndpointId::new(text).expect("a non-empty endpoint id")
}

fn failure() -> ErrorDetail {
    ErrorDetail::text("connection refused").expect("a non-empty message")
}

fn read(json: &str) -> Result<EndpointOutcome, serde_json::Error> {
    serde_json::from_str(json)
}

/// Every outcome shape, one per status plus both consent-denied origins.
fn every_outcome() -> Vec<Outcome> {
    vec![
        Outcome::Active { latency_ms: 118 },
        Outcome::Offline {
            latency_ms: 5,
            error: failure(),
        },
        Outcome::TimeOut {
            latency_ms: 5000,
            error: failure(),
        },
        Outcome::NodeError {
            latency_ms: 40,
            error: failure(),
        },
        Outcome::NotResolved { error: failure() },
        Outcome::ConsentDenied {
            refused_by: ConsentRefusal::PreFilter,
            error: None,
        },
        Outcome::ConsentDenied {
            refused_by: ConsentRefusal::Node { latency_ms: 12 },
            error: Some(failure()),
        },
        Outcome::Excluded { error: None },
        Outcome::NotLocalized { error: None },
    ]
}

#[test]
fn the_status_vocabulary_is_the_eight_values_of_11_1() {
    let wire: Vec<String> = EndpointStatus::ALL
        .iter()
        .map(|status| serde_json::to_string(status).expect("a status serializes"))
        .collect();
    assert_eq!(
        wire,
        [
            "\"active\"",
            "\"offline\"",
            "\"time-out\"",
            "\"node-error\"",
            "\"not-resolved\"",
            "\"consent-denied\"",
            "\"excluded\"",
            "\"not-localized\"",
        ],
        "§11.1 and N16 fix exactly these eight values"
    );
    for status in EndpointStatus::ALL {
        assert_eq!(
            format!("\"{status}\""),
            serde_json::to_string(&status).expect("a status serializes"),
            "Display and the wire spelling agree"
        );
    }
}

#[test]
fn an_unknown_status_is_refused() {
    let outcome = read(r#"{"id":"node_1","status":"unknown"}"#);
    assert!(outcome.is_err(), "the §11.1 set is closed");
}

#[test]
fn every_outcome_shape_validates_and_round_trips() {
    for outcome in every_outcome() {
        let entry = EndpointOutcome::new(id("node_1"), outcome)
            .with_node_id("node_1")
            .with_organisation("Org A");
        let federation = FederationMeta::new(vec![entry.clone()]).expect("one endpoint");
        let text = serde_json::to_string(&federation).expect("meta.federation serializes");
        let envelope = format!(r#"{{"rows":[],"meta":{{"federation":{text}}}}}"#);
        support::validate_text(support::RESULT_SET_SCHEMA, &envelope)
            .expect("every outcome shape validates");
        let again: FederationMeta = serde_json::from_str(&text).expect("meta.federation reads");
        assert_eq!(again, federation, "the record round-trips");
    }
}

#[test]
fn latency_is_never_emitted_before_dispatch() {
    let settled = [
        Outcome::NotResolved { error: failure() },
        Outcome::ConsentDenied {
            refused_by: ConsentRefusal::PreFilter,
            error: None,
        },
        Outcome::Excluded { error: None },
        Outcome::NotLocalized { error: None },
    ];
    for outcome in settled {
        let text = serde_json::to_string(&EndpointOutcome::new(id("node_3"), outcome))
            .expect("an outcome serializes");
        let members = support::member_names(&text).expect("an object");
        assert!(
            !members.iter().any(|member| member == "latency_ms"),
            "N40: a status settled before dispatch omits latency_ms, got {text}"
        );
    }
}

#[test]
fn the_reader_refuses_latency_settled_before_dispatch() {
    for (status, error) in [
        ("excluded", ""),
        ("not-localized", ""),
        ("not-resolved", r#","error":"no ehr_id""#),
    ] {
        let json = format!(r#"{{"id":"node_3","status":"{status}","latency_ms":0{error}}}"#);
        let refused = read(&json).expect_err("N40: a 0 would read as answered instantly");
        assert!(
            refused.to_string().contains("must not carry `latency_ms`"),
            "{status}: {refused}"
        );
    }
}

#[test]
fn the_reader_requires_latency_for_every_dispatched_status() {
    for (status, error) in [
        ("active", ""),
        ("offline", r#","error":"failed""#),
        ("time-out", r#","error":"failed""#),
        ("node-error", r#","error":"failed""#),
    ] {
        let json = format!(r#"{{"id":"node_1","status":"{status}"{error}}}"#);
        let refused = read(&json).expect_err("N40: latency_ms is MUST once dispatched");
        assert!(
            refused.to_string().contains("`latency_ms`"),
            "{status}: {refused}"
        );
    }
}

#[test]
fn the_reader_requires_an_error_for_every_failed_status() {
    for (status, latency) in [
        ("offline", r#","latency_ms":3"#),
        ("time-out", r#","latency_ms":5000"#),
        ("node-error", r#","latency_ms":40"#),
        ("not-resolved", ""),
    ] {
        let json = format!(r#"{{"id":"node_1","status":"{status}"{latency}}}"#);
        let refused = read(&json).expect_err("N40: error is MUST for a failed interaction");
        assert!(
            refused.to_string().contains("must carry `error`"),
            "{status}: {refused}"
        );
    }
}

#[test]
fn an_empty_error_is_refused() {
    let refused = read(r#"{"id":"node_1","status":"offline","latency_ms":3,"error":""}"#);
    assert!(refused.is_err(), "an empty error carries nothing (N40)");
    assert!(
        matches!(ErrorDetail::text(""), Err(WireError::EmptyMember { .. })),
        "the constructor refuses it too"
    );
}

#[test]
fn an_active_endpoint_carries_no_error() {
    let refused = read(r#"{"id":"node_1","status":"active","latency_ms":9,"error":"late"}"#)
        .expect_err("§11.1: a node error is never reported as active with an error attached");
    assert!(
        refused.to_string().contains("must not carry `error`"),
        "{refused}"
    );
}

#[test]
fn consent_denied_keeps_who_refused() {
    let pre_filter =
        read(r#"{"id":"node_2","status":"consent-denied"}"#).expect("a pre-filter refusal");
    assert_eq!(
        pre_filter.outcome(),
        &Outcome::ConsentDenied {
            refused_by: ConsentRefusal::PreFilter,
            error: None
        },
        "no latency means the Step-1 pre-filter decided (§13.2.1)"
    );
    let by_node = read(r#"{"id":"node_2","status":"consent-denied","latency_ms":12}"#)
        .expect("a node refusal");
    assert_eq!(
        by_node.outcome().latency_ms(),
        Some(12),
        "a latency means the node was asked and refused (N27)"
    );
}

#[test]
fn an_error_object_is_kept_member_by_member() {
    let text = r#"{"id":"node_1","status":"node-error","latency_ms":40,"error":{"status":503,"reason":"overloaded"}}"#;
    let outcome = read(text).expect("an object error is allowed");
    let Some(ErrorDetail::Object(members)) = outcome.outcome().error() else {
        panic!("the error should have been read as an object");
    };
    assert_eq!(members.len(), 2, "both members are kept");
    let emitted = serde_json::to_string(&outcome).expect("an outcome serializes");
    assert!(
        support::same_json(&emitted, text).expect("JSON"),
        "the object round-trips"
    );
}

#[test]
fn rows_come_only_from_an_active_endpoint() {
    let refused = read(
        r#"{"id":"node_1","status":"time-out","latency_ms":5000,"error":"late","row_count":3}"#,
    )
    .expect_err("§11.1: an unresponsive node contributes no rows");
    assert!(refused.to_string().contains("row_count"), "{refused}");
    read(r#"{"id":"node_1","status":"time-out","latency_ms":5000,"error":"late","row_count":0}"#)
        .expect("a count of 0 is truthful");
    let built =
        EndpointOutcome::new(id("node_1"), Outcome::Excluded { error: None }).with_row_count(1);
    assert!(
        matches!(built, Err(WireError::RowsWithoutAnswer { .. })),
        "the builder refuses it too"
    );
}

#[test]
fn a_member_named_twice_is_refused() {
    let refused = read(r#"{"id":"node_1","id":"node_2","status":"excluded"}"#)
        .expect_err("a duplicated member leaves its value ambiguous");
    assert!(refused.to_string().contains("more than once"), "{refused}");
}

#[test]
fn a_null_member_is_refused() {
    let refused = read(r#"{"id":"node_1","status":"excluded","node_id":null}"#);
    assert!(
        refused.is_err(),
        "the schema types node_id as a string, never null"
    );
}

#[test]
fn an_empty_endpoint_id_is_refused() {
    let refused = read(r#"{"id":"","status":"excluded"}"#);
    assert!(refused.is_err(), "the schema requires minLength 1");
    assert!(
        matches!(EndpointId::new(""), Err(WireError::EmptyMember { .. })),
        "the constructor refuses it too"
    );
}

#[test]
fn a_url_must_be_an_absolute_uri() {
    let refused = read(r#"{"id":"node_1","status":"excluded","url":"not a uri"}"#);
    assert!(refused.is_err(), "format: uri");
}

#[test]
fn unknown_members_round_trip() {
    let text =
        r#"{"id":"node_1","status":"active","latency_ms":7,"x-trace":{"hop":2},"zone":"eu"}"#;
    let outcome = read(text).expect("an open object keeps what it does not model");
    assert_eq!(outcome.extra().len(), 2, "both unknown members are kept");
    let emitted = serde_json::to_string(&outcome).expect("an outcome serializes");
    assert!(
        support::same_json(&emitted, text).expect("JSON"),
        "they are written back"
    );
}

#[test]
fn an_extra_member_cannot_shadow_a_modelled_one() {
    let mut outcome = EndpointOutcome::new(id("node_1"), Outcome::Active { latency_ms: 7 });
    outcome
        .extra_mut()
        .insert_serialized("status", "offline")
        .expect("the value serializes");
    let refused = serde_json::to_string(&outcome).expect_err("the object would name status twice");
    assert!(refused.to_string().contains("modelled member"), "{refused}");
}

#[test]
fn complete_is_true_only_when_every_in_scope_endpoint_is_active() {
    let active = EndpointOutcome::new(id("node_1"), Outcome::Active { latency_ms: 7 });
    let excluded = EndpointOutcome::new(id("node_2"), Outcome::Excluded { error: None });
    let unlocalized = EndpointOutcome::new(id("node_3"), Outcome::NotLocalized { error: None });
    let federation =
        FederationMeta::new(vec![active.clone(), excluded, unlocalized]).expect("three endpoints");
    assert!(
        federation.complete(),
        "§11.4: never-in-scope endpoints do not clear it"
    );

    for outcome in every_outcome().into_iter().filter(|outcome| {
        outcome.status().is_in_scope() && outcome.status() != EndpointStatus::Active
    }) {
        let status = outcome.status();
        let other = EndpointOutcome::new(id("node_4"), outcome);
        let federation = FederationMeta::new(vec![active.clone(), other]).expect("two endpoints");
        assert!(
            !federation.complete(),
            "§11.4: an in-scope `{status}` endpoint clears complete"
        );
    }
}

#[test]
fn a_declared_complete_that_disagrees_is_refused() {
    let text = r#"{"complete":true,"endpoints":[{"id":"node_1","status":"time-out","latency_ms":5000,"error":"late"}]}"#;
    let refused = serde_json::from_str::<FederationMeta>(text).expect_err("§11.4, N37");
    assert!(refused.to_string().contains("complete"), "{refused}");
}

#[test]
fn an_endpoint_reported_twice_is_refused() {
    let first = EndpointOutcome::new(id("node_1"), Outcome::Active { latency_ms: 7 });
    let second = EndpointOutcome::new(id("node_1"), Outcome::Excluded { error: None });
    assert!(
        matches!(
            FederationMeta::new(vec![first, second]),
            Err(WireError::DuplicateEndpoint { .. })
        ),
        "N16: each endpoint appears once, with one status"
    );
}

#[test]
fn the_status_rules_follow_the_schema_table() {
    for status in EndpointStatus::ALL {
        let requires_error = matches!(
            status,
            EndpointStatus::Offline
                | EndpointStatus::TimeOut
                | EndpointStatus::NodeError
                | EndpointStatus::NotResolved
        );
        let requires_latency = matches!(
            status,
            EndpointStatus::Active
                | EndpointStatus::Offline
                | EndpointStatus::TimeOut
                | EndpointStatus::NodeError
        );
        assert_eq!(
            status.requires_error(),
            requires_error,
            "{status}: the error conditional"
        );
        assert_eq!(
            status.requires_latency(),
            requires_latency,
            "{status}: the latency conditional"
        );
        assert_eq!(
            status.fails_all_or_nothing(),
            matches!(
                status,
                EndpointStatus::Offline | EndpointStatus::TimeOut | EndpointStatus::NodeError
            ),
            "{status}: §11.1, which statuses fail the query"
        );
    }
}
