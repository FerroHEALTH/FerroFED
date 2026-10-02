// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reference implementation's 17 AQL golden cases as a corpus, compared
//! by AST equality and each adjudicated against the specification text (the
//! research on #18). A case is evidence, never the oracle: where FerroFED's
//! outcome differs from the case, the row below says which section decides.
//!
//! The cases FerroFED passes are recorded in
//! `conformance/aql-golden/pass-list.txt`, which the README badge is rendered
//! from. The corpus test fails when a listed case stops passing or an unlisted
//! case passes, and rewrites the list when `FERROFED_CONFORMANCE_UPDATE` is
//! `1`.

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use std::collections::BTreeSet;
use std::num::NonZeroU32;

use openehr_federation::aggregate::{AggregateFunction, Recombination, Recombine};
use openehr_federation::aql::refusal::{Refusal, Unreducible};
use openehr_federation::aql::{Analysis, Context, OffsetStrategy};

use super::{analysed, ask_all, assert_same_aql};

/// The vendored corpus directory.
const CORPUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/specs/federation-ref/src/test/resources/aql-golden"
);

/// What FerroFED does with a case.
enum Verdict {
    /// The node query equals the case's expected query.
    Rewrites,
    /// The node query differs from the case's, and equals this one.
    RewritesTo(&'static str),
    /// The query is refused with the refusal this predicate accepts.
    Refuses(fn(&Refusal) -> bool),
    /// The case waits on its tracker issue: the query is refused with the
    /// refusal this predicate accepts, and the case does not pass until it is
    /// adjudicated.
    Awaits(fn(&Refusal) -> bool),
}

/// The pass list the golden badge is rendered from.
const PASS_LIST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../conformance/aql-golden/pass-list.txt"
);

/// The variable that makes the corpus test rewrite the pass list when it is
/// `1`.
const UPDATE: &str = "FERROFED_CONFORMANCE_UPDATE";

/// The comment block the corpus test writes above the cases.
const PASS_LIST_HEADER: &str = "\
# SPDX-FileCopyrightText: Vernum Projecten B.V.
# SPDX-License-Identifier: BUSL-1.1
#
# The AQL golden cases of the reference implementation that FerroFED passes,
# one file name per line, then the corpus total. The golden test writes it
# when FERROFED_CONFORMANCE_UPDATE is 1, and fails when a listed case stops
# passing or an unlisted case passes. scripts/conformance/matrix.sh
# --badges-write renders the badge from it.
";

/// The adjudicated outcome of every case, by file name.
fn verdict(case: &str) -> Verdict {
    match case {
        "01-basic-subject-rewrite.case"
        | "04-observation-with-archetype-predicate.case"
        | "08-dv-identifier-clinician-path-dispatched.case"
        | "11-no-subject-passthrough.case"
        | "12-comment-hiding-identifier-stripped.case"
        | "15-external-ref-namespace-consumed.case" => Verdict::Rewrites,
        // §5.4.3, N33, CP-38: the ENTRY-level carrier is resolution input,
        // consumed and stripped as external_ref is; 13 resolves in the declared
        // default namespace, 14 in its issuer.
        "13-entry-subject-carrier-rewrite.case" | "14-entry-subject-issuer-consumed.case" => {
            Verdict::Rewrites
        }
        // §11.6.1, N9, N39: the node keeps LIMIT n, orders
        // on the uid after the client's key, and carries the key as a hidden
        // column the Tier re-applies ORDER BY on.
        "03-subject-projection-reinjection.case" => Verdict::RewritesTo(
            "SELECT c/uid/value, c/context/start_time/value FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '550e8400-e29b-41d4-a716-446655440000' ORDER BY c/context/start_time/value DESC, c/uid/value ASC LIMIT 10",
        ),
        // §7.1: a second value in a patient carrier, which may name
        // a relative that no path tells apart; the reading that cannot leak.
        "17-entry-subject-second-value-rejected.case" => {
            Verdict::Refuses(|r| matches!(r, Refusal::SecondSubject { .. }))
        }
        // §11.6.2 option 1: the corpus runs under the reject strategy.
        "05-offset-rejected.case" => Verdict::Refuses(|r| *r == Refusal::OffsetUnsupported),
        // N14, §11.6.3: the corpus runs undirected, with no aggregate declared
        // decomposable.
        "06-undirected-aggregate-rejected.case" => {
            Verdict::Refuses(|r| matches!(r, Refusal::UndirectedAggregate { .. }))
        }
        "07-subject-under-or-rejected.case" => Verdict::Refuses(|r| {
            matches!(
                r,
                Refusal::Unreducible {
                    reason: Unreducible::NotConjunctive,
                    ..
                }
            )
        }),
        "09-subject-like-rejected.case" => Verdict::Refuses(|r| {
            matches!(
                r,
                Refusal::Unreducible {
                    reason: Unreducible::NotEquality,
                    ..
                }
            )
        }),
        // §7.1: two different values cannot reduce to one scope.
        "10-multiple-subjects-rejected.case" => {
            Verdict::Refuses(|r| matches!(r, Refusal::SecondSubject { .. }))
        }
        // §5.4.1, §5.4.3: the resolved value on the composer path.
        "16-composer-smuggling-patient-id-rejected.case" => {
            Verdict::Refuses(|r| matches!(r, Refusal::IdentifierElsewhere { .. }))
        }
        // TODO(#70): the FROM ENDPOINT directive, parsed with the openehr-query federation feature.
        "02-directive-strip-projections.case" => {
            Verdict::Awaits(|r| matches!(r, Refusal::NotAql { .. }))
        }
        other => panic!("golden case {other} has no adjudication; add one before it runs"),
    }
}

/// The deployment the corpus runs in: undirected ask-all, the reject `OFFSET`
/// strategy, the one case 05 expects (§11.6.2 option 1), and no decomposable
/// aggregate, the gateway case 06 expects (§11.6.3: "a gateway that does not
/// support it falls under the rule above").
fn corpus() -> Context {
    ask_all()
        .with_offset_strategy(OffsetStrategy::Reject)
        .with_decomposable_aggregates([])
}

/// One case file: its sections by heading.
struct Case {
    facade: String,
    ehr_id: String,
    expected: String,
}

fn read(path: &std::path::Path) -> Case {
    let text = std::fs::read_to_string(path).expect("the golden case is readable");
    let mut sections: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(name) = line.strip_prefix("== ") {
            sections.push((name.trim().to_owned(), String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line);
        }
    }
    let section = |name: &str| {
        sections
            .iter()
            .find(|(heading, _)| heading == name)
            .map_or_else(
                || panic!("{} has no `{name}` section", path.display()),
                |(_, body)| body.trim().to_owned(),
            )
    };
    Case {
        facade: section("facade"),
        ehr_id: section("ehr_id"),
        expected: section("expected"),
    }
}

#[test]
fn every_golden_case_has_its_adjudicated_outcome() {
    let mut paths: Vec<_> = std::fs::read_dir(CORPUS)
        .expect("the vendored corpus is present")
        .map(|entry| entry.expect("a corpus entry").path())
        .collect();
    paths.sort();
    assert_eq!(paths.len(), 17, "the vendored corpus holds 17 cases");
    let total = paths.len();
    let mut passed = BTreeSet::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .expect("a UTF-8 file name")
            .to_owned();
        let case = read(&path);
        let outcome = analysed(&case.facade, &corpus());
        match verdict(&name) {
            verdict @ (Verdict::Rewrites | Verdict::RewritesTo(_)) => {
                let analysis =
                    outcome.unwrap_or_else(|refusal| panic!("{name} was refused: {refusal}"));
                let node = match analysis {
                    Analysis::Patient(query) => {
                        let ehr_id = HierObjectId::new(case.ehr_id.as_str())
                            .expect("the case ehr_id is a HIER_OBJECT_ID");
                        query.for_node(&ehr_id).aql().to_owned()
                    }
                    Analysis::Unscoped(query) => {
                        assert_eq!(
                            case.ehr_id, "NONE",
                            "{name} names an ehr_id but has no patient"
                        );
                        query.node_query().aql().to_owned()
                    }
                };
                let expected = match verdict {
                    Verdict::RewritesTo(adjudicated) => adjudicated,
                    Verdict::Rewrites | Verdict::Refuses(_) | Verdict::Awaits(_) => {
                        case.expected.as_str()
                    }
                };
                assert_same_aql(&node, expected);
                passed.insert(name);
            }
            verdict @ (Verdict::Refuses(accepts) | Verdict::Awaits(accepts)) => {
                let refusal = match outcome {
                    Err(refusal) => refusal,
                    Ok(analysis) => panic!(
                        "{name} should be refused, got {analysis:?}; an awaited case is adjudicated before it passes"
                    ),
                };
                assert!(
                    accepts(&refusal),
                    "{name} drew the wrong refusal: {refusal:?}"
                );
                if matches!(verdict, Verdict::Refuses(_)) {
                    passed.insert(name);
                }
            }
        }
    }
    hold_the_pass_list(&passed, total);
}

/// Compares the passing cases with the pass list: a listed case that no longer
/// passes fails in every mode, and under `FERROFED_CONFORMANCE_UPDATE=1` the
/// list is rewritten, else an unlisted pass or a moved total fails.
fn hold_the_pass_list(passed: &BTreeSet<String>, total: usize) {
    let text = std::fs::read_to_string(PASS_LIST).expect("the pass list is readable");
    let mut listed = BTreeSet::new();
    let mut listed_total = None;
    for line in text.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(count) = line.strip_prefix("total ") {
            listed_total = Some(count.parse::<usize>().expect("the total is a count"));
        } else {
            assert!(listed.insert(line.to_owned()), "{line} is listed twice");
        }
    }
    let regressed: Vec<_> = listed.difference(passed).collect();
    assert!(
        regressed.is_empty(),
        "golden cases the pass list records no longer pass: {regressed:?}"
    );
    if std::env::var(UPDATE).is_ok_and(|value| value == "1") {
        let mut text = PASS_LIST_HEADER.to_owned();
        for case in passed {
            text.push_str(case);
            text.push('\n');
        }
        text.push_str("total ");
        text.push_str(&total.to_string());
        text.push('\n');
        std::fs::write(PASS_LIST, text).expect("the pass list is writable");
        return;
    }
    let unrecorded: Vec<_> = passed.difference(&listed).collect();
    assert!(
        unrecorded.is_empty(),
        "golden cases pass that the pass list does not record; rerun with {UPDATE}=1: {unrecorded:?}"
    );
    assert_eq!(
        listed_total,
        Some(total),
        "the pass list records another corpus total; rerun with {UPDATE}=1"
    );
}

// conformance: CP-32
#[test]
fn golden_case_05_is_refused_under_reject_and_paged_under_bounded() {
    let case = read(&std::path::Path::new(CORPUS).join("05-offset-rejected.case"));
    assert_eq!(case.expected, "ERROR:FED_OFFSET_UNSUPPORTED");
    let refusal = analysed(&case.facade, &corpus()).expect_err("§11.6.2 option 1");
    assert_eq!(refusal, Refusal::OffsetUnsupported, "N39");
    let window = NonZeroU32::new(1000).expect("1000 is not zero");
    let bounded = ask_all().with_offset_strategy(OffsetStrategy::Bounded { max_window: window });
    let Ok(Analysis::Patient(query)) = analysed(&case.facade, &bounded) else {
        panic!("case 05 names a patient and its page is bounded");
    };
    let ehr_id = HierObjectId::new(case.ehr_id.as_str()).expect("a HIER_OBJECT_ID");
    assert_same_aql(
        query.for_node(&ehr_id).aql(),
        &format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_id/value = '{}' ORDER BY c/uid/value LIMIT 15",
            case.ehr_id
        ),
    );
    assert_eq!(
        query_offset(&case.facade, &bounded),
        5,
        "§11.6.2: the Tier skips k"
    );
}

// conformance: CP-10 CP-32
#[test]
fn golden_case_06_is_refused_with_nothing_declared_and_recombined_under_the_default_set() {
    let case = read(&std::path::Path::new(CORPUS).join("06-undirected-aggregate-rejected.case"));
    assert_eq!(case.expected, "ERROR:FED_AGGREGATE_UNSUPPORTED");
    let refusal = analysed(&case.facade, &corpus()).expect_err("N14");
    assert!(
        matches!(refusal, Refusal::UndirectedAggregate { .. }),
        "§11.6.3: a gateway that declares none falls under the block: {refusal:?}"
    );
    let declared = ask_all().with_decomposable_aggregates(AggregateFunction::ALL);
    let analysis = analysed(&case.facade, &declared).expect("§11.6.3: COUNT decomposes");
    assert_eq!(
        analysis.recombination().map(Recombination::columns),
        Some(&[Recombine::Count { column: 0 }][..]),
        "the node counts are summed at the Tier"
    );
    assert_eq!(
        analysis
            .columns()
            .first()
            .map(|column| column.name.as_str()),
        Some("n"),
        "N17: the client's column"
    );
    let Analysis::Patient(query) = analysis else {
        panic!("case 06 names a patient");
    };
    let ehr_id = HierObjectId::new(case.ehr_id.as_str()).expect("a HIER_OBJECT_ID");
    assert_same_aql(
        query.for_node(&ehr_id).aql(),
        &format!(
            "SELECT COUNT(c/uid/value) AS n FROM EHR e CONTAINS COMPOSITION c \
             WHERE e/ehr_id/value = '{}'",
            case.ehr_id
        ),
    );
}

fn query_offset(facade: &str, context: &Context) -> u64 {
    analysed(facade, context)
        .expect("the page is bounded")
        .order()
        .offset()
}
