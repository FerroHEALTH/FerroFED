// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! All four `its_rest` declarations in every mode: each area declares its
//! class, and each declaration changes with its own settings and no other
//! (§7a.1, §7a.2, N30, N32).

use std::collections::{BTreeMap, BTreeSet};

use openehr_federation::options::ItsRestAreas;

use super::{Mode, Setup, TestResult};

// conformance: CP-23 CP-25
#[tokio::test]
async fn every_mode_declares_all_four_areas_and_each_follows_only_its_own_settings() -> TestResult {
    let mut query = BTreeMap::<bool, BTreeSet<String>>::new();
    let mut ehr = BTreeSet::new();
    let mut definition = BTreeMap::<(bool, bool, bool), BTreeSet<String>>::new();
    let mut demographic = BTreeMap::<bool, BTreeSet<String>>::new();
    for mode in Mode::all() {
        let setup = Setup::new(mode, &[]).await?;
        let ItsRestAreas {
            query: declared_query,
            ehr: declared_ehr,
            definition: declared_definition,
            demographic: declared_demographic,
            ..
        } = setup.declared().await?.federation.its_rest;
        let declared_demographic = declared_demographic.as_str().to_owned();
        assert!(
            declared_query.starts_with("federated:"),
            "§7a.1: {mode:?}: {declared_query}"
        );
        assert!(
            declared_ehr.starts_with("routed:"),
            "§7a.1: {mode:?}: {declared_ehr}"
        );
        assert!(
            declared_definition.starts_with("routed-single-node:"),
            "§7a.1, §12.6: {mode:?}: {declared_definition}"
        );
        let class = if mode.demographic {
            "routed-single-node:"
        } else {
            "unsupported:"
        };
        assert!(
            declared_demographic.starts_with(class),
            "§7a.1, N32: {mode:?}: {declared_demographic}"
        );
        query
            .entry(mode.registry())
            .or_default()
            .insert(declared_query);
        ehr.insert(declared_ehr);
        definition
            .entry((mode.registry(), mode.distribution(), mode.fan_out))
            .or_default()
            .insert(declared_definition);
        demographic
            .entry(mode.demographic)
            .or_default()
            .insert(declared_demographic);
    }
    assert_eq!(1, ehr.len(), "no setting changes the EHR area: {ehr:?}");
    for (name, declarations) in [
        ("query", query.into_values().collect::<Vec<_>>()),
        ("definition", definition.into_values().collect()),
        ("demographic", demographic.into_values().collect()),
    ] {
        for one in &declarations {
            assert_eq!(
                1,
                one.len(),
                "{name} follows only its own settings: {one:?}"
            );
        }
        let distinct: BTreeSet<_> = declarations.iter().flatten().collect();
        assert_eq!(
            declarations.len(),
            distinct.len(),
            "each of its settings changes the {name} declaration: {distinct:?}"
        );
    }
    Ok(())
}
