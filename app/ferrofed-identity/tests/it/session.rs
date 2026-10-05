// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2: per session, keyed by
//! `ehr_id`, each expiring a time-to-live after the last resolution that
//! returned it, and holding no patient identifier (no specification governs what a binding holds: our own
//! design).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use ferrofed_identity::session::{Bound, IdentityChange, ResolutionBindings, SessionKey};
use ferrofed_registry::id::{EhrId, NodeId};

const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";
const EHR_B: &str = "3333bbbb-3333-4333-8333-333333333333";
const EHR_C: &str = "4444cccc-4444-4444-8444-444444444444";

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

#[test]
fn a_full_store_drops_the_other_session_that_expires_soonest() {
    let capacity = NonZeroUsize::new(2).expect("two is not zero");
    let bindings = ResolutionBindings::new(Duration::from_secs(60)).with_capacity(capacity);
    let (older, newer, current) = (
        SessionKey::new("session-older"),
        SessionKey::new("session-newer"),
        SessionKey::new("session-current"),
    );
    let start = Instant::now();
    bindings.record(&older, start, [(&node("node-a"), &ehr(EHR_A))]);
    let later = start + Duration::from_secs(1);
    bindings.record(&newer, later, [(&node("node-b"), &ehr(EHR_B))]);
    bindings.record(&current, later, [(&node("node-a"), &ehr(EHR_C))]);
    assert_eq!(
        2,
        bindings.len(),
        "the store never holds more than its capacity"
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&older, later, &ehr(EHR_A)),
        "the session that expires soonest makes room"
    );
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&newer, later, &ehr(EHR_B))
    );
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&current, later, &ehr(EHR_C))
    );
}

#[test]
fn a_session_alone_past_the_capacity_keeps_what_fits_and_holds_no_more() {
    let capacity = NonZeroUsize::new(1).expect("one is not zero");
    let bindings = ResolutionBindings::new(Duration::from_secs(60)).with_capacity(capacity);
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [
            (&node("node-a"), &ehr(EHR_A)),
            (&node("node-b"), &ehr(EHR_B)),
        ],
    );
    assert_eq!(1, bindings.len());
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, now, &ehr(EHR_A))
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, now, &ehr(EHR_B)),
        "a binding that does not fit is not held"
    );
    bindings.record(&session, now, [(&node("node-c"), &ehr(EHR_A))]);
    assert_eq!(
        Bound::Several(vec![node("node-a"), node("node-c")]),
        bindings.lookup(&session, now, &ehr(EHR_A)),
        "a second claimant of a held ehr_id adds no binding, so it is kept (§12.5.2)"
    );
}

#[test]
fn a_binding_no_later_resolution_returns_expires_while_the_session_lives_on() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let start = Instant::now();
    bindings.record(&session, start, [(&node("node-a"), &ehr(EHR_A))]);
    let resolve_b = |seconds: u64| {
        bindings.record(
            &session,
            start + Duration::from_secs(seconds),
            [(&node("node-b"), &ehr(EHR_B))],
        );
    };
    resolve_b(20);
    resolve_b(40);
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, start + Duration::from_secs(59), &ehr(EHR_A)),
        "live just inside its own lifetime"
    );
    resolve_b(60);
    resolve_b(80);
    resolve_b(100);
    let later = start + Duration::from_secs(101);
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, later, &ehr(EHR_A)),
        "a binding the session's later resolutions never returned is not routed on past its lifetime"
    );
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&session, later, &ehr(EHR_B)),
        "the session lives on through the bindings its resolutions keep returning"
    );
    assert_eq!(1, bindings.len(), "the expired binding is no longer held");
}

#[test]
fn a_binding_each_resolution_returns_keeps_routing() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let start = Instant::now();
    for seconds in [0, 40, 80, 120] {
        bindings.record(
            &session,
            start + Duration::from_secs(seconds),
            [(&node("node-a"), &ehr(EHR_A))],
        );
    }
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, start + Duration::from_secs(179), &ehr(EHR_A)),
        "each resolution that returns the binding renews it"
    );
    assert_eq!(
        Bound::None,
        bindings.lookup(&session, start + Duration::from_secs(180), &ehr(EHR_A)),
        "gone a lifetime after the last resolution that returned it"
    );
}

/// Patient X resolved to `(node-c, EHR_A)` and patient Y to `(node-a, EHR_A)`
/// in one session, which then resolves only Y.
fn a_collision_whose_one_claimant_lapsed(start: Instant) -> (ResolutionBindings, SessionKey) {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    bindings.record(&session, start, [(&node("node-c"), &ehr(EHR_A))]);
    for seconds in [10, 50, 90, 130] {
        bindings.record(
            &session,
            start + Duration::from_secs(seconds),
            [(&node("node-a"), &ehr(EHR_A))],
        );
    }
    (bindings, session)
}

#[test]
fn a_collision_persists_after_one_claimant_lapses() {
    let start = Instant::now();
    let (bindings, session) = a_collision_whose_one_claimant_lapsed(start);
    assert_eq!(
        Bound::Several(vec![node("node-a"), node("node-c")]),
        bindings.lookup(&session, start + Duration::from_secs(150), &ehr(EHR_A)),
        "node-c's binding lapsed, yet the ehr_id stays a collision: routing it to node-a would \
         break the tie by where one patient resolved (§12.5.2, N42)"
    );
}

#[test]
fn an_identity_change_clears_a_sticky_collision() {
    let start = Instant::now();
    let (bindings, session) = a_collision_whose_one_claimant_lapsed(start);
    assert_eq!(
        1,
        bindings.identity_changed(&IdentityChange::Ehrs(vec![ehr(EHR_A)]))
    );
    let later = start + Duration::from_secs(140);
    assert_eq!(Bound::None, bindings.lookup(&session, later, &ehr(EHR_A)));
    bindings.record(&session, later, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&session, later, &ehr(EHR_A)),
        "a resolution after the change binds afresh"
    );
}

#[test]
fn an_identity_change_drops_a_renewed_binding_as_it_drops_any_other() {
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let start = Instant::now();
    bindings.record(&session, start, [(&node("node-a"), &ehr(EHR_A))]);
    let renewed = start + Duration::from_secs(40);
    bindings.record(&session, renewed, [(&node("node-a"), &ehr(EHR_A))]);
    assert_eq!(
        1,
        bindings.identity_changed(&IdentityChange::Ehrs(vec![ehr(EHR_A)]))
    );
    assert_eq!(Bound::None, bindings.lookup(&session, renewed, &ehr(EHR_A)));
}

#[test]
fn a_full_store_drops_expired_bindings_before_a_live_session() {
    let capacity = NonZeroUsize::new(2).expect("two is not zero");
    let bindings = ResolutionBindings::new(Duration::from_secs(60)).with_capacity(capacity);
    let (renewing, other) = (SessionKey::new("session-1"), SessionKey::new("session-2"));
    let start = Instant::now();
    bindings.record(&renewing, start, [(&node("node-a"), &ehr(EHR_A))]);
    bindings.record(
        &renewing,
        start + Duration::from_secs(50),
        [(&node("node-b"), &ehr(EHR_B))],
    );
    let later = start + Duration::from_secs(70);
    bindings.record(&other, later, [(&node("node-a"), &ehr(EHR_C))]);
    assert_eq!(2, bindings.len());
    assert_eq!(
        Bound::One(node("node-b")),
        bindings.lookup(&renewing, later, &ehr(EHR_B)),
        "the session's live binding stays; only its expired one made room"
    );
    assert_eq!(
        Bound::One(node("node-a")),
        bindings.lookup(&other, later, &ehr(EHR_C))
    );
}
