// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Version-identity dedup at the Tier (§10.2, §10.3, N15, N36, CP-9).
//!
//! A duplicate is one `OBJECT_VERSION_ID` seen at more than one endpoint: an
//! import retains the original uid, so the copy at the importing node carries
//! the whole version id of the original (§10.3, the scenario). Of the
//! endpoints that hold a version, the copy kept is the one from the endpoint
//! whose `system_id` equals the version's `creating_system_id`, the
//! originating copy (§10.2), the lowest endpoint id among several, and the
//! copy from the lowest endpoint id when no holder is the originating system:
//! deterministic, as §10.2 requires. Every row a dropped copy contributed is
//! suppressed, and every row of the kept copy stays, so a query that returns
//! several rows per version keeps all of them. Two versions of one object are
//! two ids and both stay (§10.2: "a version-history query MUST keep distinct
//! rows"), and a row with no version uid is never suppressed.
//!
//! Identifiers compare under the BASE rule: two that differ only in case are
//! one identifier, both for the version ids that group copies and for the
//! `system_id` that names the originating copy (BASE
//! `master05-identification_package.adoc` §"Composite Identifiers and Case").
//! The comparison is the one `openehr-base` carries
//! ([`composite_ids_equal`], [`composite_id_key`]), and every kept row keeps
//! its text as the node sent it, the case-preserving half of the same rule.

use std::collections::{BTreeMap, BTreeSet};

use openehr_base::v1_3::base_types::identification::lexical::{
    composite_id_key, composite_ids_equal,
};

use super::{Placed, Suppressed};

/// Suppresses every row of `rows` whose version another endpoint holds and
/// keeps, and records what it suppressed. `systems` maps an endpoint to its
/// node's `system_id`, where known; the order of `rows` is kept.
pub(super) fn suppress(
    rows: Vec<Placed>,
    systems: &BTreeMap<String, String>,
) -> (Vec<Placed>, Suppressed) {
    let mut holders: BTreeMap<String, (&str, BTreeSet<&str>)> = BTreeMap::new();
    for (endpoint, row, _) in &rows {
        if let Some(version) = &row.version {
            holders
                .entry(composite_id_key(version.value()))
                .or_insert_with(|| (version.creating_system_id_str(), BTreeSet::new()))
                .1
                .insert(endpoint.as_str());
        }
    }
    // NOTE: §10.3 and BASE §"Composite Identifiers and Case": the write goes to the system whose
    // `system_id` equals the `creating_system_id` apart from case, so the kept copy is that system's.
    let keepers: BTreeMap<String, String> = holders
        .into_iter()
        .filter(|(_, (_, endpoints))| endpoints.len() > 1)
        .filter_map(|(version, (creating, endpoints))| {
            let originating = endpoints.iter().find(|endpoint| {
                systems
                    .get(**endpoint)
                    .is_some_and(|system| composite_ids_equal(system, creating))
            });
            let keeper = originating.or_else(|| endpoints.first())?;
            Some((version, (*keeper).to_owned()))
        })
        .collect();
    let mut suppressed = Suppressed::default();
    let mut dropped = BTreeSet::new();
    let kept = rows
        .into_iter()
        .filter(|(endpoint, row, _)| {
            let copy = row
                .version
                .as_ref()
                .and_then(|version| keepers.get(&composite_id_key(version.value())))
                .is_some_and(|keeper| keeper != endpoint);
            if copy {
                suppressed.rows = suppressed.rows.saturating_add(1);
                dropped.insert(endpoint.clone());
            }
            !copy
        })
        .collect();
    suppressed.endpoints = dropped.into_iter().collect();
    (kept, suppressed)
}
