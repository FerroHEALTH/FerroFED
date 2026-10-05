// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! What an ITI-93 message does to the resolution bindings: a merge or a
//! delete drops the bindings of the `ehr_id`s it carries in a member's
//! domain, an update or a change that cannot be scoped drops every binding,
//! and a create drops none (track 8 of §16.3; PMIR §2:3.93.4.1.2).
#![allow(
    clippy::expect_used,
    reason = "a test fixture that does not build is a broken test"
)]

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use ferrofed_identity::ihe::pmir::change_of;
use ferrofed_identity::session::{Bound, IdentityChange, ResolutionBindings, SessionKey};
use ferrofed_registry::id::{EhrId, NodeId};
use ihe_iti::pmir::feed::Feed;

/// A member's `ehr_id` domain, inside the `urn:oid:2.999` example arc.
const EHR_DOMAIN: &str = "urn:oid:2.999.1.900";

/// A domain no member's `ehr_id`s are issued in.
const OTHER_DOMAIN: &str = "urn:oid:2.999.1.901";

const EHR_A: &str = "7a7a7a7a-7a7a-4a7a-8a7a-7a7a7a7a7a7a";
const EHR_B: &str = "7b7b7b7b-7b7b-4b7b-8b7b-7b7b7b7b7b7b";

fn domains() -> BTreeSet<String> {
    BTreeSet::from([EHR_DOMAIN.to_owned()])
}

fn ehr(value: &str) -> EhrId {
    EhrId::new(value).expect("a valid ehr_id")
}

/// A message whose history holds `entries`.
fn feed(entries: &[String]) -> Feed {
    let body = format!(
        r#"{{"resourceType":"Bundle","type":"message","entry":[{{"fullUrl":"https://pmir.example.org/fhir/MessageHeader/m-1","resource":{{"resourceType":"MessageHeader","id":"m-1","eventUri":"urn:ihe:iti:pmir:2019:patient-feed","destination":[{{"endpoint":"https://gateway.example.org/pmir/feed"}}],"source":{{"endpoint":"https://pmir.example.org/fhir"}},"focus":[{{"reference":"Bundle/h-1"}}]}}}},{{"fullUrl":"https://pmir.example.org/fhir/Bundle/h-1","resource":{{"resourceType":"Bundle","type":"history","entry":[{}]}}}}]}}"#,
        entries.join(",")
    );
    Feed::read(Some("application/fhir+json"), body.as_bytes()).expect("a feed")
}

/// A Patient `id` carrying the identifiers `(system, value)`, with `extra`
/// fields.
fn patient(id: &str, identifiers: &[(&str, &str)], extra: &str) -> String {
    let identifiers: Vec<String> = identifiers
        .iter()
        .map(|(system, value)| format!(r#"{{"system":"{system}","value":"{value}"}}"#))
        .collect();
    format!(
        r#"{{"resourceType":"Patient","id":"{id}","identifier":[{}]{extra}}}"#,
        identifiers.join(",")
    )
}

fn entry(method: &str, url: &str, resource: &str, status: &str) -> String {
    format!(
        r#"{{"fullUrl":"https://pmir.example.org/fhir/{url}","resource":{resource},"request":{{"method":"{method}","url":"{url}"}},"response":{{"status":"{status}"}}}}"#
    )
}

fn merge(identifiers: &[(&str, &str)]) -> String {
    entry(
        "PUT",
        "Patient/p-old",
        &patient(
            "p-old",
            identifiers,
            r#","active":false,"link":[{"other":{"reference":"Patient/p-new"},"type":"replaced-by"}]"#,
        ),
        "200",
    )
}

#[test]
fn a_merge_touches_the_ehr_ids_it_carries_in_a_member_domain() {
    let change = change_of(
        &feed(&[merge(&[(EHR_DOMAIN, EHR_A), (OTHER_DOMAIN, "SYNTHETIC-1")])]),
        &domains(),
    );
    assert_eq!(Some(IdentityChange::Ehrs(vec![ehr(EHR_A)])), change);
}

#[test]
fn a_merge_that_carries_no_member_ehr_id_cannot_be_scoped() {
    let change = change_of(
        &feed(&[merge(&[(OTHER_DOMAIN, "SYNTHETIC-1")])]),
        &domains(),
    );
    assert_eq!(Some(IdentityChange::Unscoped), change);
}

#[test]
fn a_member_domain_value_that_is_no_ehr_id_cannot_be_scoped() {
    let change = change_of(
        &feed(&[merge(&[(EHR_DOMAIN, "not an ehr_id")])]),
        &domains(),
    );
    assert_eq!(Some(IdentityChange::Unscoped), change);
}

#[test]
fn an_update_cannot_be_scoped() {
    let update = entry(
        "PUT",
        "Patient/p-1",
        &patient("p-1", &[(EHR_DOMAIN, EHR_A)], ""),
        "200",
    );
    assert_eq!(
        Some(IdentityChange::Unscoped),
        change_of(&feed(&[update]), &domains())
    );
}

#[test]
fn a_create_touches_nothing() {
    let create = entry(
        "POST",
        "Patient",
        &patient("p-1", &[(EHR_DOMAIN, EHR_A)], ""),
        "201",
    );
    assert_eq!(None, change_of(&feed(&[create]), &domains()));
}

#[test]
fn a_delete_touches_the_ehr_ids_its_patient_carries() {
    let delete = entry(
        "DELETE",
        "Patient/p-1",
        &patient("p-1", &[(EHR_DOMAIN, EHR_B)], ""),
        "200",
    );
    assert_eq!(
        Some(IdentityChange::Ehrs(vec![ehr(EHR_B)])),
        change_of(&feed(&[delete]), &domains())
    );
}

#[test]
fn the_ehr_ids_of_every_change_are_touched_together() {
    let delete = entry(
        "DELETE",
        "Patient/p-2",
        &patient("p-2", &[(EHR_DOMAIN, EHR_B)], ""),
        "200",
    );
    let change = change_of(&feed(&[merge(&[(EHR_DOMAIN, EHR_A)]), delete]), &domains());
    assert_eq!(
        Some(IdentityChange::Ehrs(vec![ehr(EHR_A), ehr(EHR_B)])),
        change
    );
}

#[test]
fn a_merge_drops_the_bindings_of_the_merged_identity_and_keeps_the_rest() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("caller-1");
    let node = NodeId::new("node-a").expect("a node id");
    let now = Instant::now();
    let (a, b) = (ehr(EHR_A), ehr(EHR_B));
    bindings.record(&session, now, [(&node, &a), (&node, &b)]);
    let change =
        change_of(&feed(&[merge(&[(EHR_DOMAIN, EHR_A)])]), &domains()).expect("a scoped change");
    assert_eq!(1, bindings.identity_changed(&change));
    assert_eq!(Bound::None, bindings.lookup(&session, now, &a));
    assert_eq!(Bound::One(node), bindings.lookup(&session, now, &b));
}
