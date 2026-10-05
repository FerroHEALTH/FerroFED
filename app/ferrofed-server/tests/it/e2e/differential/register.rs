// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The adjudicated differences between FerroFED and the reference
//! implementation, each with its verdict against the specification and the
//! place it is recorded.
//!
//! An entry covers one cause: every aspect it names, at every step it names,
//! differs because of it. A run fails when it finds a difference no entry
//! covers, and when an entry names a step and aspect the run no longer finds
//! different, so a new divergence is adjudicated before it is accepted and a
//! fixed one leaves the register.

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
    /// The specification can be read to require either answer, or neither;
    /// an item is recorded on the standing upstream-report issue.
    SpecificationAmbiguity,
    /// The specification admits both answers in so many words, so both
    /// gateways conform; the entry cites the text that admits both.
    Permitted,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::FerrofedDefect => "FerroFED defect",
            Self::ReferenceDivergence => "reference divergence",
            Self::SpecificationAmbiguity => "specification ambiguity",
            Self::Permitted => "both conform",
        })
    }
}

/// One adjudicated cause of difference.
#[derive(Debug)]
pub(crate) struct Entry {
    /// The test that finds it.
    pub(crate) test: &'static str,
    /// The steps at which it shows.
    pub(crate) steps: &'static [&'static str],
    /// The aspects that differ because of it, at each of those steps.
    pub(crate) aspects: &'static [&'static str],
    /// The verdict.
    pub(crate) verdict: Verdict,
    /// What differs and why, with the governing citation.
    pub(crate) cause: &'static str,
    /// Where it is recorded: an issue, or an item of the upstream report.
    pub(crate) recorded: &'static str,
}

/// Every adjudicated cause of difference.
pub(crate) const REGISTER: &[Entry] = &[];

/// Returns the entry that covers `aspect` at `step` in `test`, when one does.
pub(crate) fn find(test: &str, step: &str, aspect: &str) -> Option<&'static Entry> {
    REGISTER.iter().find(|entry| {
        entry.test == test && entry.steps.contains(&step) && entry.aspects.contains(&aspect)
    })
}

/// Checks the differences `found` in `test` against the register.
///
/// # Errors
///
/// Returns an error naming every difference no entry covers and every step
/// and aspect an entry names that the run did not find different.
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
    let mut stale = Vec::new();
    for entry in REGISTER.iter().filter(|entry| entry.test == test) {
        for step in entry.steps.iter().filter(|step| ran.contains(step)) {
            for aspect in entry.aspects {
                if !found.contains_key(&((*step).to_owned(), (*aspect).to_owned())) {
                    stale.push(format!("{step} {aspect} ({})", entry.recorded));
                }
            }
        }
    }
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
