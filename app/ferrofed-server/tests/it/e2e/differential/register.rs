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
    reason = "no difference the run finds is a FerroFED defect, and the variant stays for the next one"
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

/// The aspect list that covers every aspect of its steps: the step differs
/// as a whole for the entry's one cause.
const WHOLE_STEP: &[&str] = &[EVERY_ASPECT];

/// The wildcard of [`WHOLE_STEP`].
const EVERY_ASPECT: &str = "*";

/// The aspects that differ where the reference implementation answers a
/// failing fan-out with an error body and FerroFED with a result set.
const FAILURE_BODY: &[&str] = &[
    "body",
    "columns",
    "error.its-rest-shape",
    "rows",
    "rows.count",
    "schema",
];

/// The steps of the standard run where both gateways answer with
/// `meta.federation` and neither difference covers the whole step.
const STANDARD_META: &[&str] = &[
    "t1-post",
    "t2-external-ref-carrier",
    "t2-entry-carrier",
    "t2-subject-column",
    "t3-endpoint-directive",
    "t3-organisation-directive",
    "t3-endpoint-header",
    "t3-named-member-unresolved",
    "t4-node-error",
    "t4-offline",
    "t4-time-out",
    "t4-prefer-wait",
    "t4-partial",
    "t4-found-nowhere",
    "t9-ehr-id-where-form",
    "t9-ehr-id-from-form",
];

/// The steps of the standard run where both gateways answer with rows.
const STANDARD_ROWS: &[&str] = &[
    "t1-post",
    "t2-external-ref-carrier",
    "t2-entry-carrier",
    "t2-subject-column",
    "t3-endpoint-directive",
    "t3-organisation-directive",
    "t3-endpoint-header",
    "t3-named-member-unresolved",
    "t4-partial",
    "t9-ehr-id-where-form",
    "t9-ehr-id-from-form",
];

/// The steps of the twin run where both gateways answer with rows.
const TWIN_ROWS: &[&str] = &[
    "t5-duplicates",
    "t5-distinct",
    "t5-order-ascending-limit",
    "t5-order-descending-limit",
];

/// Where a run finds the caller's credential at a node.
const CALLER_CREDENTIAL: &str = "the reference implementation forwards the caller's own `Authorization` to every node (its `passthrough` outbound profile), where §13.1 and N25 have the gateway authenticate onward as itself (CP-17)";

/// Where the T159 report on the reference implementation is recorded.
const T159: &str = "#212 T159 (issuecomment-5956657380)";

/// Every adjudicated cause of difference. Entries covering a whole step come
/// first, and a step one of them covers appears in no later entry.
pub(crate) const REGISTER: &[Entry] = &[
    Entry {
        test: "standard",
        steps: &["t1-get"],
        aspects: WHOLE_STEP,
        verdict: Verdict::ReferenceDivergence,
        cause: "the reference implementation answers the ITS-REST `GET {base}/v1/query/aql?q=` form `501`, where N1 and CP-1 require a conformant openEHR Query API, which defines that operation.",
        recorded: "#212 T159 addendum item 5 (issuecomment-5956659141)",
    },
    Entry {
        test: "standard",
        steps: &["t3-endpoint-parameter"],
        aspects: WHOLE_STEP,
        verdict: Verdict::Permitted,
        cause: "a `?endpoint=` query parameter is refused `400` by the reference implementation and ignored by FerroFED; §8.4 and CP-28 require only that it is not a targeting mechanism, which neither treats it as.",
        recorded: "§8.4, N35, CP-28",
    },
    Entry {
        test: "standard",
        steps: &["t7-anonymous-caller"],
        aspects: WHOLE_STEP,
        verdict: Verdict::ReferenceDivergence,
        cause: "the reference implementation answers a caller with no credential and dispatches its query, where N25 and CP-17 require the client to authenticate to the gateway.",
        recorded: "#212 T159 item 1 (issuecomment-5956657380)",
    },
    Entry {
        test: "standard",
        steps: &["t7-node-consent-refusal", "t7-directed-consent-refusal"],
        aspects: WHOLE_STEP,
        verdict: Verdict::SpecificationAmbiguity,
        cause: "a node's `403` consent refusal is `consent-denied` and `200` at FerroFED (its registry lists the node's refusal code) and `offline` and `424` at the reference implementation; §11.1 lets a node's own refusal set `consent-denied` but defines no signal for it, and §11.3 forbids failing the query on one.",
        recorded: "#212 T151 (issuecomment-5956657380), and T159 items 4 and 5",
    },
    Entry {
        test: "twin",
        steps: &["t5-offset"],
        aspects: WHOLE_STEP,
        verdict: Verdict::Permitted,
        cause: "`LIMIT 1 OFFSET 1` across both nodes is refused `400` by the reference implementation and computed from `k + n` rows per node by FerroFED; §11.6.2 and N39 admit both, and each gateway declares its strategy in `OPTIONS` (`reject`, `bounded`).",
        recorded: "§11.6.2, N39, CP-32",
    },
    Entry {
        test: "twin",
        steps: &["t5-undirected-count"],
        aspects: WHOLE_STEP,
        verdict: Verdict::Permitted,
        cause: "an undirected `COUNT` is refused `400` by the reference implementation and decomposed by FerroFED; §11.6.3 permits a declared decomposable aggregate, and each gateway declares its set in `OPTIONS`.",
        recorded: "§11.6.3, N14, N39, CP-10",
    },
    Entry {
        test: "standard",
        steps: STANDARD_ROWS,
        aspects: &["columns"],
        verdict: Verdict::SpecificationAmbiguity,
        cause: "`columns[].path` keeps the `FROM` variable at the reference implementation (`c/uid/value`) and drops it at FerroFED (`/uid/value`); §9.2 requires the gateway's own rendering and names both shapes without choosing one.",
        recorded: "#212 issuecomment-5955710203",
    },
    Entry {
        test: "twin",
        steps: TWIN_ROWS,
        aspects: &["columns"],
        verdict: Verdict::SpecificationAmbiguity,
        cause: "`columns[].path` keeps the `FROM` variable at the reference implementation and drops it at FerroFED; §9.2 names both shapes without choosing one.",
        recorded: "#212 issuecomment-5955710203",
    },
    Entry {
        test: "standard",
        steps: STANDARD_META,
        aspects: &["meta.federation.dedup"],
        verdict: Verdict::SpecificationAmbiguity,
        cause: "FerroFED records `meta.federation.dedup.mode: none` on a pass-through answer and the reference implementation, which offers the opt-in mode, records nothing; §10.2's rule that the applied mode MUST be recorded sits under the opt-in mode's rules, and the schema requires neither member.",
        recorded: "#212 obligations audit item 11 (issuecomment-5966718241)",
    },
    Entry {
        test: "twin",
        steps: &[
            "t5-duplicates",
            "t5-distinct",
            "t5-order-ascending-limit",
            "t5-order-descending-limit",
            "t5-directed-count",
        ],
        aspects: &["meta.federation.dedup"],
        verdict: Verdict::SpecificationAmbiguity,
        cause: "the applied dedup mode is recorded by FerroFED and not by the reference implementation; §10.2 and the schema disagree on whether it is required.",
        recorded: "#212 obligations audit item 11 (issuecomment-5966718241)",
    },
    Entry {
        test: "standard",
        steps: &[
            "t6-unseen-ehr-read",
            "t1-post",
            "t2-external-ref-carrier",
            "t2-entry-carrier",
            "t2-subject-column",
            "t4-node-error",
            "t4-time-out",
            "t4-prefer-wait",
            "t4-partial",
            "t9-ehr-id-where-form",
            "t9-ehr-id-from-form",
        ],
        aspects: &["node-a.caller-credential", "node-b.caller-credential"],
        verdict: Verdict::ReferenceDivergence,
        cause: CALLER_CREDENTIAL,
        recorded: T159,
    },
    Entry {
        test: "standard",
        steps: &[
            "t3-endpoint-directive",
            "t3-named-member-unresolved",
            "t4-offline",
            "t6-follow-up-composition",
            "t6-follow-up-ehr",
            "t9-commit",
        ],
        aspects: &["node-a.caller-credential"],
        verdict: Verdict::ReferenceDivergence,
        cause: CALLER_CREDENTIAL,
        recorded: T159,
    },
    Entry {
        test: "standard",
        steps: &[
            "t3-organisation-directive",
            "t3-endpoint-header",
            "t6-targeted-create",
        ],
        aspects: &["node-b.caller-credential"],
        verdict: Verdict::ReferenceDivergence,
        cause: CALLER_CREDENTIAL,
        recorded: T159,
    },
    Entry {
        test: "twin",
        steps: TWIN_ROWS,
        aspects: &["node-a.caller-credential", "node-b.caller-credential"],
        verdict: Verdict::ReferenceDivergence,
        cause: CALLER_CREDENTIAL,
        recorded: T159,
    },
    Entry {
        test: "twin",
        steps: &["t5-directed-count"],
        aspects: &["node-a.caller-credential"],
        verdict: Verdict::ReferenceDivergence,
        cause: CALLER_CREDENTIAL,
        recorded: T159,
    },
    Entry {
        test: "standard",
        steps: &[
            "t4-node-error",
            "t4-offline",
            "t4-time-out",
            "t4-prefer-wait",
        ],
        aspects: FAILURE_BODY,
        verdict: Verdict::SpecificationAmbiguity,
        cause: "a fan-out failed under the all-or-nothing default is a `RESULT_SET` with no rows at FerroFED and an error body carrying `meta` at the reference implementation; §11.4 requires the failing response to carry the diagnostic envelope without saying which body carries it.",
        recorded: "#212 T193",
    },
    Entry {
        test: "standard",
        steps: &["t4-node-error", "t4-partial"],
        aspects: &["meta.federation.endpoints in scope"],
        verdict: Verdict::ReferenceDivergence,
        cause: "a node answering `500` is `node-error` at FerroFED and `offline` at the reference implementation, where N16 and CP-11 require `node-error`.",
        recorded: "#212 T159 item 4 (issuecomment-5956657380)",
    },
    Entry {
        test: "standard",
        steps: &["t4-prefer-wait"],
        aspects: &["meta.federation.timeout"],
        verdict: Verdict::ReferenceDivergence,
        cause: "under `Prefer: wait=2` the reference implementation reports its configured `overall_ms` (6000) and the effective budget in a member the schema does not define, where §11.5 requires the effective budget in `meta.federation.timeout`, which the schema describes as the budget in force for the request.",
        recorded: "#212 T192",
    },
    Entry {
        test: "standard",
        steps: &["t4-found-nowhere"],
        aspects: &["columns"],
        verdict: Verdict::ReferenceDivergence,
        cause: "when the patient resolves nowhere the reference implementation answers an empty `columns[]`, where N17, §9.2 and CP-35 require the gateway's rendering of the client's AQL whatever the nodes answered.",
        recorded: "#212 T191",
    },
    Entry {
        test: "standard",
        steps: &[
            "t6-unrouted-write",
            "t6-untargeted-create",
            "t3-conflicting-targets",
        ],
        aspects: &["error.its-rest-shape"],
        verdict: Verdict::ReferenceDivergence,
        cause: "the reference implementation's error body has no `validationErrors`, which the ITS-REST `Error` schema requires beside `message` (N1).",
        recorded: "#212 T190",
    },
    Entry {
        test: "standard",
        steps: &["t9-ehr-id-where-form", "t9-ehr-id-from-form"],
        aspects: &[
            "header.openehr-federation-endpoint",
            "meta.federation.endpoints in scope",
            "node-b.query-scope",
            "node-b.requests",
        ],
        verdict: Verdict::SpecificationAmbiguity,
        cause: "a query naming node A's `ehr_id` goes to node A alone at FerroFED and to both nodes at the reference implementation, which sends node B node A's `ehr_id`; §12.5.1 orders only a path `ehr_id`, and N29 makes the AQL forms equivalent to it.",
        recorded: "#212 issuecomment-5968391427 item 1",
    },
    Entry {
        test: "standard",
        steps: &["t6-unseen-ehr-read"],
        aspects: &["node-b.requests"],
        verdict: Verdict::Permitted,
        cause: "after the ask-all probe the reference implementation reads the EHR from its owner a second time and FerroFED answers from the probe; §12.5.1 step 4 fixes the probe and not whether its answer to the same read is reused.",
        recorded: "§12.5.1, N41, CP-33",
    },
];

/// Returns the entry that covers `aspect` at `step` in `test`, when one does.
pub(crate) fn find(test: &str, step: &str, aspect: &str) -> Option<&'static Entry> {
    REGISTER.iter().find(|entry| {
        entry.test == test
            && entry.steps.contains(&step)
            && (entry.aspects == WHOLE_STEP || entry.aspects.contains(&aspect))
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
            if entry.aspects == WHOLE_STEP {
                if !found.keys().any(|(found_step, _)| found_step == step) {
                    stale.push(format!("{step} {EVERY_ASPECT} ({})", entry.recorded));
                }
                continue;
            }
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
