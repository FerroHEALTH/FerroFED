// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The adjudicated differences between FerroFED and the reference
//! implementation, each with its verdict against the specification and the
//! place it is recorded.
//!
//! A run fails when it finds a difference the register does not hold, and
//! when the register holds one the run no longer finds, so a new divergence
//! is adjudicated before it is accepted and a fixed one leaves the register.

use std::error::Error;
use std::fmt;

use crate::e2e::differential::{Differences, Outcome};

/// What the specification says about a difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    dead_code,
    reason = "the register holds no entry until the first run is adjudicated"
)]
pub(crate) enum Verdict {
    /// FerroFED departs from the specification; a Bug is filed.
    FerrofedDefect,
    /// The reference implementation departs from the specification; an item
    /// is recorded on the standing upstream-report issue.
    ReferenceDivergence,
    /// The specification admits both answers or neither; an item is
    /// recorded on the standing upstream-report issue.
    SpecificationAmbiguity,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FerrofedDefect => "FerroFED defect",
            Self::ReferenceDivergence => "reference divergence",
            Self::SpecificationAmbiguity => "specification ambiguity",
        })
    }
}

/// One adjudicated difference.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The test that finds it.
    pub(crate) test: &'static str,
    /// The step that finds it.
    pub(crate) step: &'static str,
    /// The aspect that differs.
    pub(crate) aspect: &'static str,
    /// The verdict.
    pub(crate) verdict: Verdict,
    /// Where it is recorded: an issue, or an item of the upstream report.
    pub(crate) recorded: &'static str,
}

/// Every adjudicated difference.
pub(crate) const REGISTER: &[Entry] = &[];

/// Returns the entry for `aspect` of `step` in `test`, when the register
/// holds one.
pub(crate) fn find(test: &str, step: &str, aspect: &str) -> Option<&'static Entry> {
    REGISTER
        .iter()
        .find(|entry| entry.test == test && entry.step == step && entry.aspect == aspect)
}

/// Checks the differences `found` in `test` against the register.
///
/// # Errors
///
/// Returns an error naming every difference the register does not hold and
/// every entry for `test` no step found.
pub(crate) fn check(
    test: &str,
    outcomes: &[Outcome],
    found: &Differences,
) -> Result<(), Box<dyn Error>> {
    let unadjudicated: Vec<String> = found
        .iter()
        .filter(|((step, aspect), _)| find(test, step, aspect).is_none())
        .map(|((step, aspect), (ours, theirs))| {
            format!("{step} {aspect}: FerroFED {ours:?}, reference {theirs:?}")
        })
        .collect();
    let ran: Vec<&str> = outcomes.iter().map(|outcome| outcome.step.id).collect();
    let stale: Vec<String> = REGISTER
        .iter()
        .filter(|entry| entry.test == test && ran.contains(&entry.step))
        .filter(|entry| !found.contains_key(&(entry.step.to_owned(), entry.aspect.to_owned())))
        .map(|entry| format!("{} {} ({})", entry.step, entry.aspect, entry.recorded))
        .collect();
    if unadjudicated.is_empty() && stale.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the differential run of {test} disagrees with the register.\n\
         Unadjudicated differences ({}):\n  {}\n\
         Register entries no longer found ({}):\n  {}",
        unadjudicated.len(),
        unadjudicated.join("\n  "),
        stale.len(),
        stale.join("\n  ")
    )
    .into())
}
