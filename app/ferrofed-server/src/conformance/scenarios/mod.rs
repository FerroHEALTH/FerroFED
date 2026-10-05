// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The scenario checks of the §16.3 tracks, one module per track, each a
//! function over a [`Gateway`](crate::conformance::client::Gateway) and the
//! [`Fixture`].
//!
//! A check holds what a client observes: the status, the header fields and
//! the answer. What only node-side capture or an injected fault can show
//! stays with the end-to-end suite, which calls these same checks and then
//! reads its capturing proxies. The query text a check sends is the
//! client's own, written as a Connectathon participant writes it.

pub mod track1;
pub mod track11;
pub mod track2;
pub mod track3;
pub mod track4;
pub mod track5;
pub mod track6;
pub mod track7;
pub mod track9;

use crate::conformance::client::Federated;
use crate::conformance::fixture::Fixture;
use crate::conformance::{Failure, ensure};

/// What a scenario expects of one endpoint's §11.1 status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// Exactly this status.
    Is(&'static str),
    /// One of these statuses.
    OneOf(&'static [&'static str]),
    /// Any status but `active`: the endpoint contributes nothing.
    NotActive,
}

impl Expected {
    /// Whether `status` meets the expectation.
    #[must_use]
    pub fn admits(self, status: &str) -> bool {
        match self {
            Self::Is(expected) => status == expected,
            Self::OneOf(expected) => expected.contains(&status),
            Self::NotActive => status != "active",
        }
    }
}

/// The statuses a member the patient does not resolve at may carry on an
/// undirected query: not resolved, or not named by the localizer (§11.1).
pub const UNRESOLVED: Expected = Expected::OneOf(&["not-resolved", "not-localized"]);

/// Holds that `answer` reports every member of `fixture`, in registry
/// order, each with the status `expect` gives it (§9.2, §11.1, N16).
///
/// # Errors
///
/// Returns [`Failure::Check`] naming `what` when a member is missing, out of
/// order, or carries another status.
pub fn statuses(
    answer: &Federated,
    fixture: &Fixture,
    expect: impl Fn(&crate::conformance::fixture::Member) -> Expected,
    what: &str,
) -> Result<(), Failure> {
    let reported = answer.statuses();
    let members: Vec<&str> = fixture
        .members
        .iter()
        .map(|member| member.endpoint.as_str())
        .collect();
    let ids: Vec<&str> = reported.iter().map(|(id, _)| *id).collect();
    ensure(ids == members, || {
        format!("{what}: the endpoints reported are {ids:?}, and the registry holds {members:?}")
    })?;
    for (member, (_, status)) in fixture.members.iter().zip(&reported) {
        let expected = expect(member);
        ensure(expected.admits(status), || {
            format!(
                "{what}: {} is reported {status}, and {expected:?} was expected",
                member.endpoint
            )
        })?;
    }
    Ok(())
}

/// Holds the statuses of an undirected patient query: every holding member
/// active, every other one not resolved or not localized.
///
/// # Errors
///
/// Returns [`Failure::Check`] as [`statuses`] does.
pub fn undirected(answer: &Federated, fixture: &Fixture, what: &str) -> Result<(), Failure> {
    statuses(
        answer,
        fixture,
        |member| {
            if member.holding.is_some() {
                Expected::Is("active")
            } else {
                UNRESOLVED
            }
        },
        what,
    )
}

/// Holds the statuses of a query directed at `named` alone: each of them
/// active and every other member excluded (§8.1, §11.1, N11).
///
/// # Errors
///
/// Returns [`Failure::Check`] as [`statuses`] does.
pub fn directed_at(
    answer: &Federated,
    fixture: &Fixture,
    named: &[&str],
    what: &str,
) -> Result<(), Failure> {
    statuses(
        answer,
        fixture,
        |member| {
            if named.contains(&member.endpoint.as_str()) {
                Expected::Is("active")
            } else {
                Expected::Is("excluded")
            }
        },
        what,
    )
}

/// Returns the compositions the member `endpoint` holds of the patient.
///
/// # Errors
///
/// Returns [`Failure::Check`] when the member does not hold the patient.
pub fn held_by(fixture: &Fixture, endpoint: &str) -> Result<usize, Failure> {
    fixture
        .member(endpoint)
        .and_then(|member| member.holding.as_ref())
        .map(|holding| holding.compositions)
        .ok_or_else(|| Failure::Check(format!("{endpoint} holds the patient")))
}

/// Returns `count` as the `u64` a result set carries.
///
/// # Errors
///
/// Returns [`Failure::Check`] when it does not fit, which no fixture's count
/// can reach.
pub fn as_count(count: usize) -> Result<u64, Failure> {
    u64::try_from(count).map_err(|error| Failure::Check(format!("the count {count}: {error}")))
}

/// The plain patient query of a client: each composition's uid.
#[must_use]
pub fn patient_compositions(fixture: &Fixture) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {}",
        fixture.patient.predicate()
    )
}
