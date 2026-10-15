// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A receiving configuration the gateway refuses to start with.

use std::collections::BTreeMap;

use ferrofed_server::config::Config;
use ferrofed_server::federation::Federation;
use ferrofed_server::federation::error::FederationError;

use super::{receiving, rows, text};
use crate::facade::{EHR_A, NAMESPACE, PATIENT};
use crate::fhir::TestResult;

#[test]
fn a_member_the_registry_does_not_hold_refuses_the_start() -> TestResult {
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org", ""),
        &crossref,
        &receiving("node-z", &[NAMESPACE]),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let loaded = Federation::load(&settings);
    assert!(
        matches!(
            loaded,
            Err(FederationError::ReceivingMemberUnknown {
                category: "Patient-Summaries",
                ref member,
            }) if member == "node-z"
        ),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn receiving_with_no_resolver_refuses_the_start() -> TestResult {
    let dir = tempfile::tempdir()?;
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org", ""),
        "",
        &receiving("node-a", &[NAMESPACE]),
    )?;
    let settings =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve()?;
    let loaded = Federation::load(&settings);
    assert!(
        matches!(loaded, Err(FederationError::ReceivingWithoutResolver)),
        "{loaded:?}"
    );
    Ok(())
}

#[test]
fn receiving_with_no_namespace_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let crossref = rows(&[(NAMESPACE, PATIENT, "node-a", EHR_A)]);
    let text = text(
        dir.path(),
        ("http://a.example.org", "http://b.example.org", ""),
        &crossref,
        &receiving("node-a", &[]),
    )?;
    let resolved =
        Config::from_sources(Some(&crate::support::signed(&text)), &BTreeMap::new())?.resolve();
    assert!(
        matches!(
            resolved,
            Err(ferrofed_server::config::error::Error::Fhir(
                ferrofed_server::config::fhir::FhirError::Missing {
                    key: "fhir.receive_namespaces"
                }
            ))
        ),
        "{resolved:?}"
    );
    Ok(())
}
