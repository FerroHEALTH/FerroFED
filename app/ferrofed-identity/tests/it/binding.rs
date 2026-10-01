// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The resolution bindings of §12.5.1 step 2: per session, keyed by
//! `ehr_id`, expiring with the session's time-to-live, and holding no patient
//! identifier (decision A20).
#![allow(
    clippy::expect_used,
    reason = "fixture builders fail the test they serve on an impossible value"
)]

use std::time::{Duration, Instant};

use ferrofed_identity::binding::{Bound, ResolutionBindings, SessionKey};
use ferrofed_registry::id::{EhrId, NodeId};

const EHR_A: &str = "2222aaaa-2222-4222-8222-222222222222";

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
        "bindings belong to the client session (decision A20)"
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
