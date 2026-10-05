// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The order of §12.5.1 read through `owner::located`, and the index's capacity (CP-33).

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use ferrofed_identity::binding::{ResolutionBindings, SessionKey};
use ferrofed_registry::ehr_index::EhrIndex;
use ferrofed_registry::incident::Detection;
use ferrofed_server::config::{Config, error};
use ferrofed_server::facade::owner::{self, Held, Step};
use http::HeaderMap;

use super::{ENDPOINT_A, ENDPOINT_B, TestResult, collision, ehr, named, node, snapshot, targeting};

// conformance: CP-33
#[test]
fn the_explicit_target_wins_over_a_binding_and_the_index() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-b")?, &ehr()?)]);
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(
        &snapshot,
        &targeting(ENDPOINT_A),
        Some(held),
        &index,
        &ehr()?,
    )?;
    assert_eq!(
        Some((ENDPOINT_A.to_owned(), Step::Target)),
        named(&located),
        "step 1 answers, so no later step is taken (§12.5.1, N41)"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn a_held_binding_wins_over_the_index() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(&session, now, [(&node("node-a")?, &ehr()?)]);
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((ENDPOINT_A.to_owned(), Step::Binding)),
        named(&located),
        "step 2 answers before step 3 (§12.5.1, N41)"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn the_index_answers_when_no_target_and_no_binding_does() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let now = Instant::now();
    bindings.record(
        &SessionKey::new("session-2"),
        now,
        [(&node("node-a")?, &ehr()?)],
    );
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-b")?);
    let session = SessionKey::new("session-1");
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((ENDPOINT_B.to_owned(), Step::Index)),
        named(&located),
        "another session's binding is never routed on (§12.5.1 step 2), so step 3 answers"
    );
    Ok(())
}

// conformance: CP-33
#[test]
fn a_step_naming_two_members_names_no_owner_and_no_later_step_picks_one() -> TestResult {
    let snapshot = snapshot()?;
    let bindings = ResolutionBindings::new(Duration::from_secs(60));
    let session = SessionKey::new("session-1");
    let now = Instant::now();
    bindings.record(
        &session,
        now,
        [(&node("node-a")?, &ehr()?), (&node("node-b")?, &ehr()?)],
    );
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-a")?);
    let held = Held {
        bindings: &bindings,
        session: &session,
        now,
    };
    let located = owner::located(&snapshot, &HeaderMap::new(), Some(held), &index, &ehr()?)?;
    assert_eq!(
        Some((
            vec![ENDPOINT_A.to_owned(), ENDPOINT_B.to_owned()],
            Detection::Binding
        )),
        collision(&located),
        "two bound members are a collision, and the index never picks one of them (§12.5.2, N42)"
    );
    assert!(named(&located).is_none());
    index.learn(&ehr()?, &node("node-a")?);
    index.learn(&ehr()?, &node("node-b")?);
    let located = owner::located(&snapshot, &HeaderMap::new(), None, &index, &ehr()?)?;
    assert_eq!(
        Some((
            vec![ENDPOINT_A.to_owned(), ENDPOINT_B.to_owned()],
            Detection::Index
        )),
        collision(&located),
        "two indexed members are never narrowed to one (§12.5.2, N42)"
    );
    assert!(named(&located).is_none());
    Ok(())
}

#[test]
fn a_member_the_registry_no_longer_holds_names_nothing() -> TestResult {
    let snapshot = snapshot()?;
    let index = EhrIndex::new(NonZeroUsize::MIN);
    index.learn(&ehr()?, &node("node-gone")?);
    let located = owner::located(&snapshot, &HeaderMap::new(), None, &index, &ehr()?)?;
    assert!(named(&located).is_none());
    Ok(())
}

#[test]
fn the_configured_index_capacity_is_the_one_resolved_and_zero_is_refused() -> TestResult {
    let text = "[federation]\nehr_index_capacity = 250\n";
    let settings = Config::from_sources(Some(text), &BTreeMap::new())?.resolve()?;
    assert_eq!(250, settings.federation.ehr_index_capacity.get());
    let default = Config::from_sources(Some(""), &BTreeMap::new())?.resolve()?;
    assert_eq!(100_000, default.federation.ehr_index_capacity.get());
    let zero = "[federation]\nehr_index_capacity = 0\n";
    match Config::from_sources(Some(zero), &BTreeMap::new())?.resolve() {
        Err(error::Error::Zero { key }) if key == "federation.ehr_index_capacity" => Ok(()),
        other => Err(format!("refused naming the key: {other:?}").into()),
    }
}
