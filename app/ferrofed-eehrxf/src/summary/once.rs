// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A composition two or more members answered with under one full version
//! id, an imported copy (Federation Tier §10.3), listed once, and the
//! narrative line that says so.

use super::Origin;

/// A composition two or more origins answered with under one full
/// `OBJECT_VERSION_ID` (Federation Tier §10.3, an imported copy), listed once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedOnce {
    /// The origin whose copy the section lists, by its index in the origins.
    pub kept: usize,
    /// The origins whose copy the section leaves out, by index.
    pub dropped: Vec<usize>,
}

/// One narrative line for each composition `listed` names as listed once,
/// naming the organisations of the kept and the dropped copies.
pub(super) fn lines(listed: &[ListedOnce], origins: &[Origin]) -> Vec<String> {
    let shown = |index: usize| {
        origins.get(index).map_or_else(
            || index.to_string(),
            |origin| origin.organisation.shown().to_owned(),
        )
    };
    listed
        .iter()
        .map(|once| {
            let dropped: Vec<String> = once.dropped.iter().map(|index| shown(*index)).collect();
            format!(
                "A record {} also holds as a copy of the one from {} is listed once, from {}.",
                dropped.join(", "),
                shown(once.kept),
                shown(once.kept)
            )
        })
        .collect()
}
