// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A string function that rebuilds the identifier at the node is refused
//! (§5.4.1, §5.4.2). `CONCAT`, `CONCAT_WS` and `SUBSTRING` are folded over their literal
//! arguments and the folded text is value-tested (§5.4.1 "in any position");
//! a string function over a literal that cannot be folded is refused on an
//! identifier-bearing path.

use openehr_federation::aql::Analysis;
use openehr_federation::aql::refusal::Refusal;

use super::{analysed, ask_all, refused};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";

/// A patient query for `4711` with `condition` beside it.
fn with(condition: &str) -> String {
    format!(
        "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {SUBJECT} = '4711' AND {condition}"
    )
}

fn assert_rebuilt(condition: &str) {
    let refusal = refused(&with(condition));
    assert!(
        matches!(refusal, Refusal::IdentifierElsewhere { .. }),
        "{condition} rebuilds the identifier, got {refusal:?}"
    );
}

#[test]
fn concat_of_split_literals_is_refused() {
    // The architecture's own case: CONCAT('12','345') rebuilds '12345'.
    assert_rebuilt("c/name/value = CONCAT('47', '11')");
    assert_rebuilt("c/name/value = concat('4', '7', '1', '1')");
}

#[test]
fn concat_ws_of_split_literals_is_refused() {
    assert_rebuilt("c/name/value = CONCAT_WS('', '47', '11')");
    assert_rebuilt("c/name/value = CONCAT_WS('7', '4', '11')");
}

#[test]
fn substring_of_a_literal_holding_the_identifier_is_refused() {
    // The literal alone already carries it; SUBSTRING over a folded CONCAT is
    // the case only the fold sees.
    assert_rebuilt("c/name/value = SUBSTRING(CONCAT('x47', '11y'), 2, 4)");
}

#[test]
fn a_nested_fold_is_refused() {
    assert_rebuilt("c/name/value = CONCAT(CONCAT('4', '7'), CONCAT_WS('', '1', '1'))");
    assert_rebuilt("c/name/value = CONCAT(SUBSTRING('x47', 2), '11')");
}

#[test]
fn a_fold_in_a_selected_column_is_refused() {
    let aql = format!(
        "SELECT CONCAT('47', '11') AS x, c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {SUBJECT} = '4711'"
    );
    assert!(
        matches!(refused(&aql), Refusal::IdentifierElsewhere { .. }),
        "§5.4.2: SELECT is a leak path"
    );
}

#[test]
fn an_unfoldable_string_function_over_a_literal_on_an_identifier_path_is_refused() {
    for condition in [
        "c/composer/identifiers/id = CONCAT('47', c/name/value)",
        "c/composer/identifiers/id = LENGTH('4711x')",
        "CONCAT('47', c/name/value) = c/composer/external_ref/id/value",
    ] {
        let refusal = refused(&with(condition));
        assert!(
            matches!(refusal, Refusal::UnfoldableFunction { .. }),
            "{condition} cannot be folded, got {refusal:?}"
        );
    }
}

#[test]
fn a_fold_that_does_not_rebuild_the_identifier_is_dispatched() {
    let aql = with("c/name/value = CONCAT('Vis', 'it')");
    assert!(
        matches!(analysed(&aql, &ask_all()), Ok(Analysis::Patient(_))),
        "an unrelated fold is query material"
    );
}

#[test]
fn an_unfoldable_function_off_an_identifier_path_is_dispatched() {
    let aql = with("c/name/value = CONCAT('Vis', c/context/setting/value)");
    assert!(
        matches!(analysed(&aql, &ask_all()), Ok(Analysis::Patient(_))),
        "no identifier path, no refusal"
    );
}

#[test]
fn a_folded_clinician_identifier_with_another_value_is_dispatched() {
    let aql = with("c/composer/identifiers/id = CONCAT('90', '01')");
    assert!(
        matches!(analysed(&aql, &ask_all()), Ok(Analysis::Patient(_))),
        "§5.4.3: the value decides, not the path"
    );
}

#[test]
fn no_fold_refusal_names_the_identifier() {
    for condition in [
        "c/name/value = CONCAT('sentinel', '4711')",
        "c/composer/identifiers/id = CONCAT('sentinel', c/name/value)",
    ] {
        let aql = format!(
            "SELECT c/uid/value FROM EHR e CONTAINS COMPOSITION c WHERE {SUBJECT} = 'sentinel4711' AND {condition}"
        );
        let refusal = refused(&aql);
        let shown = format!("{refusal} {refusal:?}");
        assert!(
            !shown.contains("sentinel4711"),
            "the refusal named the identifier: {shown}"
        );
    }
}
