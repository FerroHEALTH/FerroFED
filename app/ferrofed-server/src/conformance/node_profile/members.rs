// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The node profile of every active member, as `conformance run
//! --node-profile` reports it.
//!
//! Each member the federation asks through an active endpoint gets every
//! check of the profile, over that endpoint's node client: the invocation
//! check on the EHR the run seeded there, the subjectless EHR, the error
//! pass-through, and the admission check's §12b.2 conditions (§12b.1,
//! §16.2; CP-18, CP-27, CP-33a). The two release checks need a refusal the
//! operator arranges with the node's own policy, which a run cannot set, so
//! they are recorded not observable with that reason (CP-18, CP-19). Every
//! EHR a check creates is listed with the run's writes, the outbound gate
//! withholds the run's patient from every request (§5.4.1, N33), and every
//! evidence line is redacted against it. No specification governs the
//! form of the run: our own design.

use std::sync::Arc;

use ferrofed_engine::hygiene::Withheld;
use ferrofed_registry::id::EndpointId;
use ferrofed_registry::snapshot::RegistrySnapshot;
use uuid::Uuid;

use super::interface::Interface;
use super::{Check, Finding, Profile, Verdict, checks};
use crate::admission::report::Condition;
use crate::admission::{self, DEFAULT_COUNT};
use crate::chain;
use crate::conformance::fixture::Fixture;
use crate::conformance::seed::Written;
use crate::federation::Federation;

/// Why a run observes no access decision: the refusal is the node's own.
pub const ACCESS_NOT_ARRANGED: &str = "a run arranges no refusal at the node: the access decision under test is the node's own policy, which the operator arranges with the node's access controls and a run cannot set, so nothing at its interface shows the decision";

/// Why a run observes no consent check: the refusal is the node's own.
pub const CONSENT_NOT_ARRANGED: &str = "a run arranges no consent refusal at the node: ITS-REST defines no consent resource, and a consent decision is recorded at the node by means a run cannot reach, so nothing at its interface shows a consent check";

/// The §12b.2 conditions, in the order of its table.
const CONDITIONS: [Condition; 5] = [
    Condition::EhrIdGeneration,
    Condition::NoReuse,
    Condition::NoForeignAdoption,
    Condition::SystemIdUniqueness,
    Condition::EhrIdExchange,
];

/// Runs the node profile against every active member of `federation`.
///
/// The checks read the EHRs `fixture` holds, every EHR a check created is
/// recorded in `written`, and one profile per member is returned, in
/// registry order.
///
/// A member a check cannot reach is recorded with that check not observable
/// and the typed cause, never with a pass.
pub async fn every_member(
    federation: &Federation,
    fixture: &Fixture,
    written: &mut Vec<Written>,
) -> Vec<Profile> {
    let snapshot = federation.snapshot();
    let active: Vec<EndpointId> = snapshot
        .nodes()
        .filter_map(|node| snapshot.asked_through(node.id()))
        .map(|endpoint| endpoint.id().clone())
        .collect();
    let mut profiles = Vec::with_capacity(active.len());
    for endpoint in active {
        let mut profile = Profile::new(product(snapshot, &endpoint));
        for finding in member(federation, fixture, &endpoint, written).await {
            let evidence = finding
                .evidence()
                .iter()
                .map(|line| fixture.patient.redact(line))
                .collect();
            profile.record(Finding::new(finding.check(), finding.verdict(), evidence));
        }
        profiles.push(profile);
    }
    profiles
}

/// The product and release the registry records for `endpoint`'s node, or
/// the node or endpoint id where it records none.
fn product(snapshot: &RegistrySnapshot, endpoint: &EndpointId) -> String {
    snapshot
        .endpoint(endpoint)
        .and_then(|declared| snapshot.node(declared.node()))
        .map_or_else(
            || endpoint.as_str().to_owned(),
            |node| match (node.product(), node.version()) {
                (Some(product), Some(version)) => format!("{product} {version}"),
                (Some(product), None) => product.to_owned(),
                _ => format!("node {}", node.id()),
            },
        )
}

/// Every finding of the profile against `endpoint`.
async fn member(
    federation: &Federation,
    fixture: &Fixture,
    endpoint: &EndpointId,
    written: &mut Vec<Written>,
) -> Vec<Finding> {
    let withheld = Arc::new(Withheld::new([fixture.patient.value().clone()]));
    let mut findings = match Interface::of(federation, endpoint) {
        Ok(interface) => observed(&interface.with_withheld(withheld), fixture, written).await,
        Err(error) => {
            let reason = format!("the check could not start: {}", chain(&error));
            [
                Check::InvocableOnEhrId,
                Check::SubjectNotRequired,
                Check::ErrorsPassedThrough,
            ]
            .into_iter()
            .map(|check| Finding::not_arranged(check, reason.clone()))
            .collect()
        }
    };
    findings.push(Finding::not_arranged(
        Check::AccessDecidedAtNode,
        ACCESS_NOT_ARRANGED,
    ));
    findings.push(Finding::not_arranged(
        Check::ConsentBeforeRelease,
        CONSENT_NOT_ARRANGED,
    ));
    match admission::check(federation, endpoint, DEFAULT_COUNT).await {
        Ok(report) => {
            for ehr_id in report.created() {
                written.push(Written {
                    endpoint: endpoint.as_str().to_owned(),
                    what: format!("EHR {ehr_id} (the admission check's synthetic subject)"),
                });
            }
            findings.extend(report.findings().iter().map(Finding::of_admission));
        }
        Err(error) => {
            let reason = format!("the admission check could not start: {}", chain(&error));
            findings.extend(CONDITIONS.into_iter().map(|condition| {
                Finding::not_arranged(Check::IdentifierIntegrity(condition), reason.clone())
            }));
        }
    }
    findings
}

/// The findings of the checks that reach `interface`.
async fn observed(
    interface: &Interface,
    fixture: &Fixture,
    written: &mut Vec<Written>,
) -> Vec<Finding> {
    let endpoint = interface.endpoint().as_str();
    let seeded = fixture
        .member(endpoint)
        .and_then(|member| member.holding.as_ref())
        .map(|holding| holding.ehr_id.as_str());
    let invocable = match seeded.map(Uuid::try_parse) {
        Some(Ok(ehr_id)) => reached(
            Check::InvocableOnEhrId,
            checks::invocable_on_ehr_id(interface, ehr_id).await,
        ),
        Some(Err(error)) => Finding::not_arranged(
            Check::InvocableOnEhrId,
            format!("the ehr_id the cross-reference names here is no UUID the check can address: {error}"),
        ),
        None => Finding::not_arranged(
            Check::InvocableOnEhrId,
            match &fixture.shortfall {
                Some(shortfall) => format!(
                    "the run seeded no EHR with a composition at this member: {shortfall}"
                ),
                None => "the run seeded no EHR with a composition at this member: the cross-reference does not resolve the synthetic patient here".to_owned(),
            },
        ),
    };
    let subjectless = reached(
        Check::SubjectNotRequired,
        checks::subject_not_required(interface).await,
    );
    for ehr_id in subjectless.created() {
        written.push(Written {
            endpoint: endpoint.to_owned(),
            what: format!("EHR {ehr_id} with no subject (the node profile's subjectless EHR)"),
        });
    }
    let errors = reached(
        Check::ErrorsPassedThrough,
        checks::errors_passed_through(interface).await,
    );
    vec![invocable, subjectless, errors]
}

/// The finding of a check, or the one that it reached no answer.
fn reached(check: Check, outcome: Result<Finding, super::interface::CheckError>) -> Finding {
    outcome.unwrap_or_else(|error| {
        Finding::new(
            check,
            Verdict::NotObservable,
            vec![format!("the check reached no answer: {}", chain(&error))],
        )
    })
}
