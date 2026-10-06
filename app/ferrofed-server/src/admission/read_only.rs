// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The admission check without writes, for a member whose governance
//! forbids test data in its production CDR.
//!
//! The run creates no EHR. It sends the node one AQL query, through the
//! endpoint's node client and the outbound gate, for the `ehr_id` and the
//! `system_id` of up to `count` EHRs the node already holds, and reads no
//! subject and no clinical content. It reports on the conditions of §12b.2
//! that read can reach, and marks the rest `cannot-check`:
//!
//! | Condition | What the run does |
//! |---|---|
//! | `ehr_id` generation | each `ehr_id` read is a version-4 UUID, and no two are equal |
//! | No reuse | cannot be checked, as in a full run |
//! | No adoption of foreign `ehr_id`s | cannot be checked, as in a full run |
//! | `system_id` uniqueness | each EHR reports the `system_id` the registry records for the node, or one a `[[creating_system]]` entry routes to it |
//! | `ehr_id` exchange | cannot be checked: the run creates no subject whose `ehr_id` it knows |
//!
//! §12b.1 asks that the conditions be verified by test, so the report names
//! every condition a run without writes leaves unproven. No specification
//! governs the form of the check: our own design.

use std::collections::BTreeMap;
use std::time::Instant;

use ferrofed_engine::dispatch::{Contact, DispatchOptions, NodeQuery, NodeReply};
use ferrofed_engine::outbound_id::OutboundId;
use ferrofed_registry::creating_system::CreatingSystemRoute;
use ferrofed_registry::id::{EndpointId, SystemId};
use ferrofed_registry::snapshot::{Node, RegistrySnapshot};
use openehr_its::rest::generated::query::ResultSetRow;

use super::report::{Condition, Finding, Report, Verdict};
use super::{AdmissionError, finding, no_foreign_adoption, no_reuse, uuid_form};
use crate::chain;
use crate::conveyed;
use crate::facade::cells;
use crate::federation::Federation;

/// The query of existing EHRs: each one's `ehr_id` and the `system_id` of
/// the system it was created on (RM `EHR.ehr_id`, `EHR.system_id`).
pub const EXISTING_EHRS: &str = "SELECT e/ehr_id/value, e/system_id/value FROM EHR e";

/// The columns [`EXISTING_EHRS`] selects.
const WIDTH: usize = 2;

/// One existing EHR the node returned.
struct Existing {
    /// The row the EHR is in, from 1, which names it in the report.
    row: usize,
    ehr_id: String,
    system_id: String,
}

/// Runs the admission check without writes against `endpoint` of
/// `federation`, reading at most `count` existing EHRs on its node.
///
/// # Errors
///
/// Returns [`AdmissionError::UnknownEndpoint`] when the registry does not
/// hold `endpoint`, [`AdmissionError::Clock`] when no deadline can be set,
/// and [`AdmissionError::Unconveyed`] when the federation holds no signer.
/// Every failure of the node is a finding.
pub async fn check(
    federation: &Federation,
    endpoint: &EndpointId,
    count: u8,
) -> Result<Report, AdmissionError> {
    let snapshot = federation.snapshot();
    let unknown = || AdmissionError::UnknownEndpoint(endpoint.clone());
    let declared = snapshot.endpoint(endpoint).ok_or_else(unknown)?;
    let node = snapshot.node(declared.node()).ok_or_else(unknown)?;
    let client = federation.clients().get(endpoint).ok_or_else(unknown)?;
    let deadline = Instant::now()
        .checked_add(federation.budget().per_node())
        .ok_or(AdmissionError::Clock)?;
    let conveyance = conveyed::gateway(federation).map_err(AdmissionError::Unconveyed)?;
    let options = DispatchOptions::new(deadline, conveyance).with_request_id(OutboundId::mint());
    let query = NodeQuery::new(EXISTING_EHRS)
        .with_width(WIDTH)
        .with_fetch(u32::from(count));

    let existing = match client.query(&query, &options).await {
        Ok(NodeReply::Answered { result_set, .. }) => existing(&result_set.rows),
        Ok(NodeReply::Failed { outcome, contact }) => Err(format!(
            "the query of existing EHRs ended {}: {}",
            outcome.status(),
            contacted(contact)
        )),
        Err(error) => Err(format!(
            "the query of existing EHRs was not sent: {}",
            chain(&error)
        )),
    };
    let findings = match &existing {
        Ok(existing) => vec![
            generation(existing),
            no_reuse(),
            no_foreign_adoption(),
            system_id(snapshot, node, existing),
            exchange(),
        ],
        Err(line) => vec![
            Finding::new(
                Condition::EhrIdGeneration,
                Verdict::Fail,
                vec![line.clone()],
            ),
            no_reuse(),
            no_foreign_adoption(),
            Finding::new(
                Condition::SystemIdUniqueness,
                Verdict::Fail,
                vec![line.clone()],
            ),
            exchange(),
        ],
    };
    Ok(Report::read_only(
        endpoint.clone(),
        node.id().clone(),
        existing.map_or(0, |existing| existing.len()),
        findings,
    ))
}

/// What a failed query showed of the node, without its body, which could
/// echo what the node holds.
fn contacted(contact: Contact) -> String {
    match contact {
        Contact::Answered(status) => format!("the node answered {status}"),
        Contact::Silent => "the node gave no answer".to_owned(),
        Contact::Unsent | Contact::Capped => "the request never left the gateway".to_owned(),
    }
}

/// The existing EHRs of `rows`, or the line that fails the run when a row is
/// not two strings: a defective answer is never read as a shorter one.
fn existing(rows: &[ResultSetRow]) -> Result<Vec<Existing>, String> {
    rows.iter()
        .enumerate()
        .map(|(index, row)| {
            match (cells::text(row, 0), cells::text(row, 1)) {
                (Some(ehr_id), Some(system_id)) => Ok(Existing {
                    row: index.saturating_add(1),
                    ehr_id: ehr_id.to_owned(),
                    system_id: system_id.to_owned(),
                }),
                _ => Err(format!(
                    "row {} of the query of existing EHRs is not an ehr_id and a system_id as two strings",
                    index.saturating_add(1)
                )),
            }
        })
        .collect()
}

/// The `ehr_id` generation condition over the existing EHRs: each `ehr_id`
/// is a version-4 UUID and no two are equal (§12b.2, N42a).
fn generation(existing: &[Existing]) -> Finding {
    if existing.is_empty() {
        return Finding::new(
            Condition::EhrIdGeneration,
            Verdict::CannotCheck,
            vec![
                "the node returned no EHR, so a run without writes has no ehr_id to judge"
                    .to_owned(),
            ],
        );
    }
    let mut lines: Vec<(Verdict, String)> = existing
        .iter()
        .map(|one| uuid_form(&one.ehr_id, &format!("the ehr_id in row {}", one.row)))
        .collect();
    let mut seen: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for one in existing {
        seen.entry(one.ehr_id.to_ascii_lowercase())
            .or_default()
            .push(one.row);
    }
    let repeated: Vec<(Verdict, String)> = seen
        .values()
        .filter(|rows| rows.len() > 1)
        .map(|rows| {
            let rows: Vec<String> = rows.iter().map(ToString::to_string).collect();
            (
                Verdict::Fail,
                format!("the EHRs in rows {} share one ehr_id", rows.join(", ")),
            )
        })
        .collect();
    if repeated.is_empty() && existing.len() > 1 {
        lines.push((
            Verdict::Pass,
            format!("the {} ehr_ids are distinct", existing.len()),
        ));
    }
    lines.extend(repeated);
    lines.push((
        Verdict::Pass,
        "the evidence is the ehr_ids of EHRs the node already holds; a full run judges the ehr_ids it issues for new EHRs".to_owned(),
    ));
    finding(Condition::EhrIdGeneration, lines)
}

/// The `system_id` uniqueness condition over the existing EHRs: each reports
/// the `system_id` the registry records for the node, or one the registry
/// routes to it (§12b.2, §12.2, N42a).
fn system_id(snapshot: &RegistrySnapshot, node: &Node, existing: &[Existing]) -> Finding {
    let mut lines = vec![(
        Verdict::Pass,
        format!(
            "the registry records system_id {} for node {}, and its load refuses a second member with the same system_id, compared without regard to ASCII case",
            node.system_id(),
            node.id()
        ),
    )];
    if existing.is_empty() {
        lines.push((
            Verdict::CannotCheck,
            "the node returned no EHR, so the system_id it reports could not be read".to_owned(),
        ));
    }
    lines.extend(existing.iter().map(|one| reported(snapshot, node, one)));
    finding(Condition::SystemIdUniqueness, lines)
}

/// What the `system_id` an existing EHR reports shows.
// NOTE: §12.2, N21; an existing EHR may predate the node's own system_id, so one the registry
// routes nowhere leaves the condition undecided, while one routed to another member fails it.
fn reported(snapshot: &RegistrySnapshot, node: &Node, one: &Existing) -> (Verdict, String) {
    let (ehr, value) = (format!("the EHR in row {}", one.row), &one.system_id);
    let Ok(system_id) = SystemId::new(value.as_str()) else {
        return (
            Verdict::Fail,
            format!("{ehr} reports system_id {value}, which is not an openEHR uid"),
        );
    };
    match snapshot.registered_creating_system(&system_id) {
        Some((_, CreatingSystemRoute::Member { node: member })) if member == *node.id() => (
            Verdict::Pass,
            format!("{ehr} reports system_id {value}, the one the registry records"),
        ),
        Some((
            _,
            CreatingSystemRoute::Registered {
                node: member,
                endpoint,
            },
        )) if member == *node.id() => (
            Verdict::Pass,
            format!(
                "{ehr} reports system_id {value}, which a [[creating_system]] entry routes to endpoint {endpoint} of this node"
            ),
        ),
        Some((
            _,
            CreatingSystemRoute::Member { node: other }
            | CreatingSystemRoute::Registered { node: other, .. },
        )) => (
            Verdict::Fail,
            format!("{ehr} reports system_id {value}, which the registry routes to node {other}"),
        ),
        Some(_) | None => (
            Verdict::CannotCheck,
            format!(
                "{ehr} reports system_id {value}, which the registry routes to no member: the EHR may have been created on another system, and a read cannot show which system_id the node stamps into new EHRs"
            ),
        ),
    }
}

/// The `ehr_id` exchange condition, which a run without writes cannot
/// exercise.
fn exchange() -> Finding {
    Finding::new(
        Condition::EhrIdExchange,
        Verdict::CannotCheck,
        vec![
            "a run without writes creates no synthetic subject, and it reads no subject of an existing EHR, so it knows no patient whose ehr_id it could ask the cross-reference for (§5.5, N34)".to_owned(),
            "verify the exchange with a test patient the node's environment registers, or by a full run against a staging copy of the node".to_owned(),
        ],
    )
}
