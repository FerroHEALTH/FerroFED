// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! `LIMIT n OFFSET k` with `k > 0` across a fan-out (§11.6.2, N9, N39): never
//! pushed down, refused under the reject strategy, and under the bounded
//! strategy dispatched as `LIMIT k + n` with no `OFFSET`, the Tier skipping
//! `k` rows, within the configured bound and refused past it.

use std::num::NonZeroU32;

use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aql::refusal::{OffsetPage, Refusal};
use openehr_federation::aql::{Analysis, Context, OffsetStrategy, Paging, Targeting, analyse};
use openehr_federation::order::{Direction, ResultOrder, SortKey};
use openehr_query::bind::Parameters;

use super::{EHR_ID, ask_all, assert_same_aql};

const SUBJECT: &str = "e/ehr_status/subject/external_ref/id/value";
const FROM: &str = "FROM EHR e CONTAINS COMPOSITION c";
const SCOPED: &str = "FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value";

/// The bound of the fixture deployments, 20 rows per node.
fn bound() -> NonZeroU32 {
    NonZeroU32::new(20).expect("20 is not zero")
}

fn bounded() -> Context {
    ask_all().with_offset_strategy(OffsetStrategy::Bounded {
        max_window: bound(),
    })
}

fn rejecting() -> Context {
    ask_all().with_offset_strategy(OffsetStrategy::Reject)
}

fn patient_query(tail: &str) -> String {
    format!("SELECT c/uid/value {FROM} WHERE {SUBJECT} = '4711' {tail}")
}

fn analysed(aql: &str, paging: Paging, context: &Context) -> Result<Analysis, Refusal> {
    analyse(aql, &Parameters::new(), paging, context)
}

fn node_aql(analysis: &Analysis) -> String {
    match analysis {
        Analysis::Patient(query) => query
            .for_node(&HierObjectId::new(EHR_ID).expect("the fixture ehr_id is a HIER_OBJECT_ID"))
            .aql()
            .to_owned(),
        Analysis::Unscoped(query) => query.node_query().aql().to_owned(),
    }
}

fn refusal(aql: &str, paging: Paging, context: &Context) -> Refusal {
    match analysed(aql, paging, context) {
        Err(refusal) => refusal,
        Ok(analysis) => panic!("expected a refusal, got {analysis:?}"),
    }
}

// conformance: CP-32
#[test]
fn the_reject_strategy_refuses_an_offset_past_zero() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 10 OFFSET 5");
    let refused = refusal(&aql, Paging::default(), &rejecting());
    assert_eq!(refused, Refusal::OffsetUnsupported, "§11.6.2 option 1, N39");
    assert_eq!(refused.kind(), "offset-unsupported");
}

#[test]
fn a_context_refuses_offset_unless_a_strategy_is_declared() {
    assert_eq!(
        Context::new(Targeting::AskAll).offset_strategy(),
        OffsetStrategy::Reject,
        "the library refuses until the deployment declares a strategy"
    );
}

// conformance: CP-32
#[test]
fn the_bounded_strategy_asks_each_node_for_k_plus_n_rows_and_never_pushes_offset() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 10 OFFSET 5");
    let analysis = analysed(&aql, Paging::default(), &bounded()).expect("the page is bounded");
    assert_same_aql(
        &node_aql(&analysis),
        &format!("SELECT c/uid/value {SCOPED} = '{EHR_ID}' ORDER BY c/uid/value LIMIT 15"),
    );
    assert_eq!(
        analysis.order(),
        &ResultOrder::new(
            vec![SortKey::new(0, Direction::Ascending)],
            vec![0],
            Some(15)
        )
        .with_offset(5),
        "§11.6.2: k + n rows per node, then the Tier keeps rows [k, k + n)"
    );
}

// conformance: CP-32
#[test]
fn the_offset_and_fetch_members_page_like_the_clauses_under_the_bounded_strategy() {
    let aql = patient_query("ORDER BY c/context/start_time/value DESC");
    let analysis = analysed(
        &aql,
        Paging {
            offset: Some(4),
            fetch: Some(3),
        },
        &bounded(),
    )
    .expect("the page is bounded");
    assert_same_aql(
        &node_aql(&analysis),
        &format!(
            "SELECT c/uid/value, c/context/start_time/value {SCOPED} = '{EHR_ID}' \
             ORDER BY c/context/start_time/value DESC, c/uid/value ASC LIMIT 7"
        ),
    );
    assert_eq!(analysis.order().limit(), Some(7));
    assert_eq!(
        analysis.order().offset(),
        4,
        "the offset member pages like OFFSET (§11.6.2)"
    );
}

// conformance: CP-32
#[test]
fn an_unscoped_query_is_paged_the_same_way() {
    let aql = format!("SELECT c/name/value {FROM} ORDER BY c/name/value LIMIT 2 OFFSET 3");
    let analysis = analysed(&aql, Paging::default(), &bounded()).expect("the page is bounded");
    assert_same_aql(
        &node_aql(&analysis),
        &format!(
            "SELECT c/name/value, c/uid/value {FROM} \
             ORDER BY c/name/value, c/uid/value ASC LIMIT 5"
        ),
    );
    assert_eq!(analysis.order().offset(), 3);
}

// conformance: CP-32
#[test]
fn an_ehr_only_page_asks_each_node_for_k_plus_n_rows_tie_broken_on_the_ehr_id() {
    let aql = "SELECT e/ehr_status/uid/value FROM EHR e \
               ORDER BY e/time_created/value LIMIT 2 OFFSET 3";
    let analysis = analysed(aql, Paging::default(), &bounded()).expect("the page is bounded");
    assert_same_aql(
        &node_aql(&analysis),
        "SELECT e/ehr_status/uid/value, e/time_created/value, e/ehr_id/value FROM EHR e \
         ORDER BY e/time_created/value, e/ehr_id/value ASC LIMIT 5",
    );
    assert_eq!(
        analysis.order(),
        &ResultOrder::new(
            vec![SortKey::new(1, Direction::Ascending)],
            vec![2],
            Some(5)
        )
        .with_offset(3),
        "§11.6.2: every node picks the same tied rows at its k + n cut"
    );
    assert_eq!(
        analysis.columns().len(),
        1,
        "N17, §9.2: the pushed key is not a column of the client's query"
    );
}

// conformance: CP-32
#[test]
fn a_page_whose_window_is_the_bound_is_accepted() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 5 OFFSET 15");
    let analysis = analysed(&aql, Paging::default(), &bounded()).expect("k + n is the bound");
    assert_eq!(analysis.order().limit(), Some(20));
}

// conformance: CP-32
#[test]
fn a_page_past_the_bound_is_refused_naming_the_bound() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 6 OFFSET 15");
    let refused = refusal(&aql, Paging::default(), &bounded());
    assert_eq!(
        refused,
        Refusal::OffsetPage {
            reason: OffsetPage::PastTheBound { max_window: 20 }
        },
        "§11.6.2: it MUST reject when it cannot bound k + n"
    );
    assert_eq!(refused.kind(), "offset-page");
    let message = refused.to_string();
    assert!(message.contains("20 rows per node"), "{message}");
    assert!(
        !message.contains("4711") && !message.contains("15"),
        "a refusal names the bound and never quotes the query: {message}"
    );
    assert_eq!(refused.at(), None);
}

// conformance: CP-32
#[test]
fn a_window_that_overflows_is_past_the_bound() {
    let aql = patient_query(&format!("ORDER BY c/uid/value LIMIT {} OFFSET 1", i64::MAX));
    assert_eq!(
        refusal(&aql, Paging::default(), &bounded()),
        Refusal::OffsetPage {
            reason: OffsetPage::PastTheBound { max_window: 20 }
        },
        "k + n is checked arithmetic"
    );
}

// conformance: CP-32
#[test]
fn an_offset_with_no_limit_is_refused_under_the_bounded_strategy() {
    let aql = patient_query("ORDER BY c/uid/value");
    let paging = Paging {
        offset: Some(5),
        fetch: None,
    };
    assert_eq!(
        refusal(&aql, paging, &bounded()),
        Refusal::OffsetPage {
            reason: OffsetPage::NoLimit
        },
        "§11.6.2: with no LIMIT, k + n cannot be bounded"
    );
}

// conformance: CP-32
#[test]
fn an_offset_with_no_order_by_is_refused_under_the_bounded_strategy() {
    let aql = patient_query("LIMIT 10 OFFSET 5");
    assert_eq!(
        refusal(&aql, Paging::default(), &bounded()),
        Refusal::OffsetPage {
            reason: OffsetPage::NoOrder
        },
        "§11.6.2 slices a merged order, and the query fixes none"
    );
}

#[test]
fn a_zero_offset_under_the_bounded_strategy_is_the_plain_limit() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 10 OFFSET 0");
    let analysis = analysed(&aql, Paging::default(), &bounded()).expect("no offset");
    assert_same_aql(
        &node_aql(&analysis),
        &format!("SELECT c/uid/value {SCOPED} = '{EHR_ID}' ORDER BY c/uid/value LIMIT 10"),
    );
    assert_eq!(analysis.order().offset(), 0);
}

#[test]
fn a_negative_offset_is_refused() {
    let aql = patient_query("ORDER BY c/uid/value LIMIT 10 OFFSET -1");
    let refused = refusal(&aql, Paging::default(), &bounded());
    assert!(
        matches!(refused, Refusal::NotAql { at: Some(_) }),
        "AQL: OFFSET is followed by a row count, got {refused:?}"
    );
}

#[test]
fn the_strategy_names_what_options_declares() {
    assert_eq!(OffsetStrategy::Reject.name(), "reject");
    assert_eq!(OffsetStrategy::Reject.max_window(), None);
    let bounded = OffsetStrategy::Bounded {
        max_window: bound(),
    };
    assert_eq!(bounded.name(), "bounded");
    assert_eq!(bounded.max_window(), Some(bound()));
}
