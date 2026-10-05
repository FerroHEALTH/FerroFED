// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every scenario a conformance run reports, in the order it runs, with the
//! §17 points and §16.3 tracks each scores.
//!
//! A [`Kind::Live`] scenario runs against the deployment. A
//! [`Kind::Harness`] one is the part of an end-to-end scenario a live
//! deployment cannot provide for, and is reported `not-run` with its reason,
//! so a point or a track scored by it never reads `pass` from a run. The
//! tokens mirror the `// conformance:` markers of the end-to-end tests the
//! checks are shared with. No specification governs the catalogue's form:
//! our own design.

/// The reason of a part judged on what reached a node.
pub const WIRE: &str = "judged on node-side wire capture (section 16.3), which a run against a deployment does not have";

/// The reason of a part that needs a node to fail.
pub const FAULT: &str = "needs a fault injected at a node or at a service the gateway calls, which a run against a deployment does not inject";

/// The reason of a part that needs a node to answer late.
pub const DELAY: &str =
    "needs a delay injected at a node, which a run against a deployment does not inject";

/// The reason of a part that needs the gateway configured for it.
pub const CONFIG: &str = "needs a gateway configured for the one scenario, and a run scores the deployment as it is configured";

/// The reason of the collision half of track 11.
pub const COLLISION: &str = "needs one ehr_id seeded at two members, a fixture section 16.3 makes the harness operator's to create because section 12b.2 forbids a node to reach it, so a run against a deployment never creates it";

/// A scenario a run executes against the deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Scenario {
    /// Track 1: one single-CDR-shaped result set by `POST` and `GET`.
    SingleCdrShaped,
    /// Track 2: both patient carriers return the same rows.
    BothCarriers,
    /// Track 2: a selected subject column is the client's input.
    SubjectColumn,
    /// Track 3: `FROM ENDPOINT` selects the node set.
    DirectiveEndpoint,
    /// Track 3: `FROM ORGANISATION` selects the node set.
    DirectiveOrganisation,
    /// Track 3: a named member where the patient does not resolve.
    NamedUnresolved,
    /// Track 3: the endpoint header selects what the directive selects.
    EndpointHeader,
    /// Track 3: a directive and a header in conflict are refused.
    TargetingConflict,
    /// Track 3: `?endpoint=` targets nothing.
    EndpointParameter,
    /// Track 4: a patient found nowhere is a `200`.
    FoundNowhere,
    /// Track 5: duplicates pass through and `DISTINCT` folds them.
    DuplicatesAndDistinct,
    /// Track 5: `ORDER BY` and `LIMIT` over the union.
    OrderAndLimit,
    /// Track 5: an undirected `COUNT` recombined or refused as declared.
    AggregateAsDeclared,
    /// Track 5: a directed `COUNT` is answered.
    AggregateDirected,
    /// Track 6: a follow-up read reaches the member holding the row.
    FollowUpRead,
    /// Track 6: an unseen `ehr_id` is found by the probe.
    ProbeRead,
    /// Track 6: a write nothing routes is refused.
    UnroutedWrite,
    /// Track 6: a new EHR naming no member is refused.
    NewEhrUntargeted,
    /// Track 6: a new EHR is created at the member named.
    NewEhrNamed,
    /// Track 7: a caller with no credential is refused.
    Unauthenticated,
    /// Track 9: the self-description and DEMOGRAPHIC as declared.
    SelfDescription,
    /// Track 9: a definition request naming one member.
    DefinitionNamed,
    /// Track 9: a definition request naming no one member is refused.
    DefinitionUnnamed,
    /// Track 9: a stored query runs by name over the members.
    StoredQueryRun,
    /// Track 9: a second `PUT` of a stored version is refused.
    StoredQuerySecondPut,
    /// Track 9: a directed stored query runs federated.
    StoredQueryDirected,
    /// Track 9: a plain client given only the base reads and writes.
    PlainClient,
    /// Track 11: a versioned write nothing routes is refused.
    VersionedWriteUnrouted,
}

/// How a scenario is scored by a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Run against the deployment.
    Live(Scenario),
    /// Reported `not-run`, for the reason given.
    Harness(&'static str),
}

/// One scenario of the catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    /// The scenario's name in the report.
    pub name: &'static str,
    /// The §17 points and §16.3 tracks it scores, as `CP-n` and `track-n`.
    pub tokens: &'static [&'static str],
    /// How a run scores it.
    pub kind: Kind,
}

/// Returns the entry named `name`, scoring `tokens`, run against the
/// deployment.
const fn live(name: &'static str, tokens: &'static [&'static str], scenario: Scenario) -> Entry {
    Entry {
        name,
        tokens,
        kind: Kind::Live(scenario),
    }
}

/// Returns the entry named `name`, scoring `tokens`, reported `not-run`
/// for `reason`.
const fn harness(
    name: &'static str,
    tokens: &'static [&'static str],
    reason: &'static str,
) -> Entry {
    Entry {
        name,
        tokens,
        kind: Kind::Harness(reason),
    }
}

/// Every scenario, in the order a run executes them: the patient-scoped
/// reads first and the write that adds to the patient's EHR last.
pub const CATALOGUE: &[Entry] = &[
    live(
        "track 1: one single-CDR-shaped result set by POST and GET",
        &["CP-1", "CP-35", "track-1"],
        Scenario::SingleCdrShaped,
    ),
    harness(
        "track 1: each node query keyed on its own ehr_id, with no subject",
        &["CP-2", "track-1"],
        WIRE,
    ),
    harness(
        "track 1: columns[] the same whichever node answers first",
        &["CP-35", "track-1"],
        DELAY,
    ),
    live(
        "track 2: both patient carriers resolve and return the same rows",
        &["CP-3", "CP-38", "track-2"],
        Scenario::BothCarriers,
    ),
    harness(
        "track 2: each node asked by its own ehr_id, never receiving subject",
        &["CP-2", "CP-4", "track-2"],
        WIRE,
    ),
    live(
        "track 2: a selected subject column is the client's input",
        &["CP-7", "track-2"],
        Scenario::SubjectColumn,
    ),
    harness(
        "track 2: a selected subject column is never asked of a node",
        &["CP-7", "track-2"],
        WIRE,
    ),
    harness(
        "track 2: a member where the patient does not resolve is never asked",
        &["CP-36", "track-2"],
        WIRE,
    ),
    live(
        "track 3: FROM ENDPOINT selects the node set",
        &["CP-6", "CP-37", "track-3"],
        Scenario::DirectiveEndpoint,
    ),
    live(
        "track 3: FROM ORGANISATION selects the node set",
        &["CP-6", "track-3"],
        Scenario::DirectiveOrganisation,
    ),
    harness(
        "track 3: a member not named is never asked and no directive reaches a node",
        &["CP-6", "track-3"],
        WIRE,
    ),
    live(
        "track 3: a named member where the patient does not resolve is not-resolved",
        &["CP-6", "track-3"],
        Scenario::NamedUnresolved,
    ),
    live(
        "track 3: the endpoint header selects what the directive selects",
        &["CP-28", "track-3"],
        Scenario::EndpointHeader,
    ),
    live(
        "track 3: a directive and a header naming other members are refused",
        &["CP-28", "track-3"],
        Scenario::TargetingConflict,
    ),
    live(
        "track 3: ?endpoint= is no targeting mechanism",
        &["CP-28", "track-3"],
        Scenario::EndpointParameter,
    ),
    harness(
        "track 3: a refused request and the endpoint parameter reach no node",
        &["CP-28", "track-3"],
        WIRE,
    ),
    harness(
        "track 3: the localizer names the members asked and fails closed",
        &["CP-5", "CP-11", "CP-30", "track-3", "track-4"],
        FAULT,
    ),
    harness(
        "track 4: a node answering an error fails the query 424 node-error",
        &["CP-11", "CP-12", "CP-30", "track-4"],
        FAULT,
    ),
    harness(
        "track 4: an unreachable node fails the query 504 offline",
        &["CP-11", "CP-12", "CP-30", "track-4"],
        FAULT,
    ),
    harness(
        "track 4: a slow node is abandoned at the per-node timeout",
        &["CP-12", "CP-30", "CP-31", "track-4"],
        DELAY,
    ),
    harness(
        "track 4: a client wait shortens the budget and never extends it",
        &["CP-31", "track-4"],
        DELAY,
    ),
    harness(
        "track 4: partial returns the answering nodes' rows only where offered",
        &["CP-30", "track-4"],
        FAULT,
    ),
    live(
        "track 4: a patient found nowhere is a 200 with empty rows",
        &["CP-12", "CP-30", "track-4"],
        Scenario::FoundNowhere,
    ),
    harness(
        "track 4: a consent-denied member leaves a 200 with the rest",
        &["CP-30", "track-4"],
        CONFIG,
    ),
    live(
        "track 5: duplicates pass through and DISTINCT folds them",
        &["CP-8", "CP-9", "track-5"],
        Scenario::DuplicatesAndDistinct,
    ),
    live(
        "track 5: ORDER BY and LIMIT return the global top rows",
        &["CP-8", "CP-32", "track-5"],
        Scenario::OrderAndLimit,
    ),
    harness(
        "track 5: OFFSET is never pushed down to a node",
        &["CP-32", "track-5"],
        WIRE,
    ),
    harness(
        "track 5: the same top row whichever node answers last",
        &["CP-32", "track-5"],
        DELAY,
    ),
    live(
        "track 5: an undirected COUNT is recombined or refused as declared",
        &["CP-10", "CP-32", "track-5"],
        Scenario::AggregateAsDeclared,
    ),
    harness(
        "track 5: an undirected COUNT under the other declaration",
        &["CP-10", "track-5"],
        CONFIG,
    ),
    live(
        "track 5: a directed COUNT is answered",
        &["CP-10", "track-5"],
        Scenario::AggregateDirected,
    ),
    harness(
        "track 5: a refused aggregate asks no node for per-node rows",
        &["CP-32", "track-5"],
        WIRE,
    ),
    live(
        "track 6: a follow-up read reaches the member holding the row",
        &["CP-13", "CP-33", "track-6"],
        Scenario::FollowUpRead,
    ),
    harness(
        "track 6: the holding member is asked once and no other",
        &["track-6"],
        WIRE,
    ),
    live(
        "track 6: an ehr_id the gateway has not seen is found by the probe",
        &["CP-33", "track-6"],
        Scenario::ProbeRead,
    ),
    live(
        "track 6: a write nothing routes is refused",
        &["CP-33", "track-6"],
        Scenario::UnroutedWrite,
    ),
    harness(
        "track 6: the probe is read-only and a write is never probed",
        &["CP-33", "track-6"],
        WIRE,
    ),
    live(
        "track 6: a new EHR naming no member is refused",
        &["CP-15", "track-6"],
        Scenario::NewEhrUntargeted,
    ),
    live(
        "track 6: a new EHR is created at the member named",
        &["CP-15", "track-6"],
        Scenario::NewEhrNamed,
    ),
    harness(
        "track 6: the one chosen node creates the EHR and no other is asked",
        &["CP-15", "track-6"],
        WIRE,
    ),
    harness(
        "track 7: each node receives its onward token and the caller",
        &["CP-16", "CP-17", "track-7"],
        WIRE,
    ),
    live(
        "track 7: a caller that does not authenticate is refused",
        &["CP-17", "track-7"],
        Scenario::Unauthenticated,
    ),
    harness(
        "track 7: an unauthenticated request reaches no node",
        &["CP-17", "track-7"],
        WIRE,
    ),
    harness(
        "track 7: with no consent service, a node's refusal is reported",
        &["CP-30", "CP-36", "track-7"],
        FAULT,
    ),
    harness(
        "track 7: a member the consent service denies is never dispatched to",
        &["CP-36", "track-7"],
        CONFIG,
    ),
    harness(
        "track 7: a member the consent service admits still refuses",
        &["CP-30", "CP-36", "track-7"],
        FAULT,
    ),
    harness(
        "track 7: a directed query is consent-checked at the node",
        &["CP-36", "track-7"],
        FAULT,
    ),
    live(
        "track 9: OPTIONS lists the members and DEMOGRAPHIC answers as declared",
        &["CP-23", "CP-25", "track-9"],
        Scenario::SelfDescription,
    ),
    harness(
        "track 9: the self-description and DEMOGRAPHIC ask no node",
        &["CP-23", "CP-25", "track-9"],
        WIRE,
    ),
    live(
        "track 9: a definition request naming one member is its answer",
        &["CP-34", "track-9"],
        Scenario::DefinitionNamed,
    ),
    live(
        "track 9: a definition request naming no one member is refused",
        &["CP-34", "track-9"],
        Scenario::DefinitionUnnamed,
    ),
    harness(
        "track 9: one node answers a definition request and no catalogue is merged",
        &["CP-34", "track-9"],
        WIRE,
    ),
    live(
        "track 9: a stored query held at the gateway runs by name",
        &["CP-40", "track-9"],
        Scenario::StoredQueryRun,
    ),
    live(
        "track 9: a second PUT of a stored version is refused",
        &["CP-40", "track-9"],
        Scenario::StoredQuerySecondPut,
    ),
    live(
        "track 9: a directed stored query stays federated-executable",
        &["CP-40", "track-9"],
        Scenario::StoredQueryDirected,
    ),
    harness(
        "track 9: a stored definition stays at the gateway",
        &["CP-40", "track-9"],
        WIRE,
    ),
    harness(
        "track 9: a definition fan-out reports a one-node rejection as 207",
        &["CP-40", "track-9"],
        FAULT,
    ),
    live(
        "track 9: a plain client given only the base URL reads and writes",
        &["CP-21", "CP-22", "CP-24", "track-9"],
        Scenario::PlainClient,
    ),
    harness(
        "track 9: the gateway's base never reaches a node",
        &["CP-21", "track-9"],
        WIRE,
    ),
    harness(
        "track 10: the identifier in four positions reaches no node",
        &["CP-26", "track-10"],
        WIRE,
    ),
    harness(
        "track 10: a committed DV_IDENTIFIER arrives byte-identical",
        &["CP-26", "track-10"],
        WIRE,
    ),
    harness(
        "track 11: a read and a query of a duplicated ehr_id are refused 409",
        &["CP-33", "track-11"],
        COLLISION,
    ),
    harness(
        "track 11: once the index holds both claimants nothing reaches either",
        &["CP-33", "track-11"],
        COLLISION,
    ),
    live(
        "track 11: a versioned write nothing routes is refused 400",
        &["CP-33", "track-11"],
        Scenario::VersionedWriteUnrouted,
    ),
    harness(
        "track 11: the refused versioned write probes nobody",
        &["track-11"],
        WIRE,
    ),
];

/// The tracks a run drives against the deployment (§16.3); track 8 is
/// provisional and track 10 is judged on node-side capture alone.
pub const LIVE_TRACKS: [u8; 9] = [1, 2, 3, 4, 5, 6, 7, 9, 11];

#[cfg(test)]
mod tests {
    use super::{CATALOGUE, Kind, LIVE_TRACKS};
    use std::collections::BTreeSet;

    #[test]
    fn every_live_track_has_a_live_scenario_and_every_entry_a_track() {
        for track in LIVE_TRACKS {
            let token = format!("track-{track}");
            assert!(
                CATALOGUE
                    .iter()
                    .any(|entry| matches!(entry.kind, Kind::Live(_))
                        && entry.tokens.contains(&token.as_str())),
                "{token} has a scenario a run executes"
            );
        }
        for entry in CATALOGUE {
            assert!(
                entry.tokens.iter().any(|token| token.starts_with("track-")),
                "{} names its track",
                entry.name
            );
        }
    }

    #[test]
    fn every_name_is_unique_and_every_live_scenario_runs_once() {
        let names: BTreeSet<_> = CATALOGUE.iter().map(|entry| entry.name).collect();
        assert_eq!(CATALOGUE.len(), names.len(), "one entry per name");
        let live: Vec<_> = CATALOGUE
            .iter()
            .filter_map(|entry| match entry.kind {
                Kind::Live(scenario) => Some(format!("{scenario:?}")),
                Kind::Harness(_) => None,
            })
            .collect();
        let unique: BTreeSet<_> = live.iter().collect();
        assert_eq!(
            live.len(),
            unique.len(),
            "each live scenario is listed once"
        );
    }
}
