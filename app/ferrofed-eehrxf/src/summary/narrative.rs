// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A section's generated narrative.
//!
//! It says what the section holds, which members gave no answer, which
//! compositions no mapping covers and which members serve their own summary.
//! No specification governs the wording: our own design.

use std::collections::{BTreeMap, BTreeSet};

use crate::summary::{Covered, Elsewhere, NOT_EXHAUSTIVE, Origin};

/// The names of the organisations at `authors`.
pub(super) fn authors_shown(
    authors: &BTreeSet<String>,
    members: &BTreeMap<usize, String>,
    origins: &[Origin],
) -> Vec<String> {
    members
        .iter()
        .filter(|(_, url)| authors.contains(*url))
        .filter_map(|(index, _)| origins.get(*index))
        .map(|origin| origin.organisation.shown().to_owned())
        .collect()
}

/// The paragraphs of a section's generated narrative.
pub(super) fn narrative(
    entries: usize,
    (silent, uncovered): (&[String], &BTreeMap<(usize, String), usize>),
    origins: &[Origin],
    contributors: &[String],
) -> Vec<String> {
    let mut paragraphs = Vec::new();
    paragraphs.push(if entries == 0 {
        String::from("No entry.")
    } else {
        format!("{entries} entries, from {}.", contributors.join(", "))
    });
    paragraphs.push(NOT_EXHAUSTIVE.to_owned());
    if !silent.is_empty() {
        paragraphs.push(format!("No answer from: {}.", silent.join(", ")));
    }
    for ((index, template), count) in uncovered {
        let member = origins
            .get(*index)
            .map_or("a member", |origin| origin.organisation.shown());
        paragraphs.push(format!(
            "Not shown: {count} record(s) of the template {template} from {member}, which no mapping covers."
        ));
    }
    paragraphs
}

/// The paragraphs that name each member of `elsewhere` that answered as
/// serving its own patient summary, which holds its data for the section.
pub(super) fn covered_elsewhere(elsewhere: &[Elsewhere]) -> Vec<String> {
    elsewhere
        .iter()
        .filter_map(|member| {
            let shown = member.organisation.shown();
            match member.answer {
                Covered::Document => Some(format!(
                    "Not mapped here: the records of {shown}, which serves its own patient summary."
                )),
                Covered::Absent => Some(format!(
                    "Not mapped here: the records of {shown}, which serves its own patient summary and holds none for this patient."
                )),
                Covered::Silent { .. } => None,
            }
        })
        .collect()
}
