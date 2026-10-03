// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2: per session, keyed by
//! `ehr_id`, expiring with the session's time-to-live, and holding no patient
//! identifier (no specification governs what a binding holds: our own
//! design).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::time::{Duration, Instant};

use ferrofed_identity::binding::{Bound, IdentityChange, ResolutionBindings, SessionKey};
use ferrofed_registry::id::{EhrId, NodeId};

const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const EHR_B: &str = "3333bbbb-3333-4333-8333-333333333333";

fn node(id: &str) -> NodeId {
    NodeId::new(id).expect("a node id")
}

fn ehr(id: &str) -> EhrId {
    EhrId::new(id).expect("an ehr_id")
}

#[test]
fn a_session_finds_the_member_its_resolution_bound() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now, &ehr(EHR_A)),
        "step 2 answers with the bound member (§12.5.1)"
    );
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now, &ehr(&EHR_A.to_ascii_uppercase())),
        "an ehr_id that differs only in case is the same identifier"
    );
}

#[test]
fn another_session_sees_nothing() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let now = Instant::now();
    bindings.record(
        &SessionKey::new("session-1"),
        now,
        [(&node("node-a"), &ehr(EHR_A))],
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&SessionKey::new("session-2"), now, &ehr(EHR_A)),
        "bindings belong to the client session (§12.5.1 step 2)"
    );
}

#[test]
fn a_binding_past_its_time_to_live_is_never_routed_on() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    let later = now + Duration::from_secs(61);
    assert_eq!(Bound::None, bindings.lookup(&session, later, &ehr(EHR_A)));
}

#[test]
fn two_members_with_one_ehr_id_are_ambiguous_and_never_chosen_between() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_A)),
        ],
    );
    assert_eq!(
        Bound::Several(vec![node("node-a"), node("node-b")]),
        bindings.lookup(&session, now, &ehr(EHR_A)),
        "step 2 yields no unambiguous answer, so routing moves on (N41, N42)"
    );
}

#[test]
fn forgetting_a_session_drops_its_bindings() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    bindings.forget(&session);
    assert_eq!(Bound::None, bindings.lookup(&session, now, &ehr(EHR_A)));
    assert!(
        format!("{bindings:?}").contains("sessions: 0"),
        "Debug counts sessions and shows no binding: {bindings:?}"
    );
}

#[test]
fn a_configured_lifetime_is_the_bound_and_not_a_default() {
    let bindings = ResolutionBindings::new(Duration::from_millis(1_500));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now + Duration::from_millis(1_499), &ehr(EHR_A)),
        "still live just inside the configured lifetime"
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, now + Duration::from_millis(1_500), &ehr(EHR_A)),
        "gone exactly at the configured lifetime"
    );
}

#[test]
fn an_identity_change_on_an_ehr_id_drops_it_in_every_session() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let one = SessionKey::new("session-1");
    let two = SessionKey::new("session-2");
    let now = Instant::now();
    bindings.record(
        &one,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_B)),
        ],
    );
    bindings.record(&two, now, [(&node("node-a"), &ehr(EHR_A))]);
    let dropped = bindings.identity_changed(&IdentityChange::Ehrs(vec![ehr(EHR_A)]));
    assert_eq!(2, dropped, "one binding of EHR_A in each session");
    assert_eq!(Bound::None, bindings.lookup(&one, now, &ehr(EHR_A)));
    assert_eq!(Bound::None, bindings.lookup(&two, now, &ehr(EHR_A)));
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&one, now, &ehr(EHR_B)),
        "a binding the change does not touch stays"
    );
}

#[test]
fn an_identity_change_matches_an_ehr_id_without_regard_to_case() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    let upper = EHR_A.to_ascii_uppercase();
    assert_eq!(
        1,
        bindings.identity_changed(&IdentityChange::Ehrs(vec![ehr(&upper)]))
    );
    assert_eq!(Bound::None, bindings.lookup(&session, now, &ehr(EHR_A)));
}

#[test]
fn an_unscoped_identity_change_drops_every_binding() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let one = SessionKey::new("session-1");
    let two = SessionKey::new("session-2");
    let now = Instant::now();
    bindings.record(
        &one,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_B)),
        ],
    );
    bindings.record(&two, now, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(3, bindings.identity_changed(&IdentityChange::Unscoped));
    assert_eq!(Bound::None, bindings.lookup(&one, now, &ehr(EHR_B)));
    assert_eq!(Bound::None, bindings.lookup(&two, now, &ehr(EHR_A)));
    assert!(
        format!("{bindings:?}").contains("sessions: 0"),
        "no session is left holding anything: {bindings:?}"
    );
}

#[test]
fn a_departed_member_takes_every_binding_that_names_it() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let one = SessionKey::new("session-1");
    let two = SessionKey::new("session-2");
    let now = Instant::now();
    bindings.record(
        &one,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_B)),
        ],
    );
    bindings.record(
        &two,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_A)),
        ],
    );
    let departed = std::collections::BTreeSet::from([node("node-b")]);
    assert_eq!(2, bindings.forget_members(&departed));
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&one, now, &ehr(EHR_A)),
        "a binding naming only a remaining member stays"
    );
    assert_eq!(Bound::None, bindings.lookup(&one, now, &ehr(EHR_B)));
    assert_eq!(
        Bound::None,
        bindings.lookup(&two, now, &ehr(EHR_A)),
        "a collision is dropped whole, never narrowed to node-a (N42)"
    );
    assert_eq!(0, bindings.forget_members(&departed), "nothing is left");
}

#[test]
fn a_consent_denial_drops_the_session_bindings_of_the_denied_member_only() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let one = SessionKey::new("session-1");
    let two = SessionKey::new("session-2");
    let now = Instant::now();
    bindings.record(
        &one,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_B)),
        ],
    );
    bindings.record(&two, now, [(&node("node-b"), &ehr(EHR_B))]);
    let denied = std::collections::BTreeSet::from([node("node-b")]);
    assert_eq!(1, bindings.forget_denied(&one, &denied), "N27a");
    assert_eq!(Bound::None, bindings.lookup(&one, now, &ehr(EHR_B)));
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&one, now, &ehr(EHR_A)),
        "a binding of a member the pre-filter did not deny stays"
    );
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&two, now, &ehr(EHR_B)),
        "the denial belongs to its session's query, never another session's"
    );
    let none = std::collections::BTreeSet::new();
    assert_eq!(
        0,
        bindings.forget_denied(&one, &none),
        "no denial drops nothing"
    );
}

#[test]
fn a_binding_naming_an_absent_member_is_dropped_and_one_naming_present_members_kept() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let other = SessionKey::new("session-2");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_A)),
            (&node("node-a"), &ehr(EHR_B)),
        ],
    );
    bindings.record(&other, now, [(&node("node-b"), &ehr(EHR_A))]);
    let present = |member: &NodeId| *member == node("node-a");
    assert!(bindings.forget_absent(&session, &ehr(EHR_A), present));
    assert!(!bindings.forget_absent(&session, &ehr(EHR_B), present));
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, now, &ehr(EHR_A)),
        "a stale collision is dropped whole, never narrowed to node-a (N42)"
    );
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now, &ehr(EHR_B))
    );
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&other, now, &ehr(EHR_A)),
        "another session's binding is its own"
    );
}

#[test]
fn an_identity_change_naming_no_bound_ehr_id_drops_nothing() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(
        0,
        bindings.identity_changed(&IdentityChange::Ehrs(vec![ehr(EHR_B)]))
    );
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now, &ehr(EHR_A))
    );
}
