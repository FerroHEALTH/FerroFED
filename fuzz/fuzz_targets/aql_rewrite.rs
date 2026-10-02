// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Arbitrary AQL text and parameters into the rewrite of §7.1, with the
//! identifier-hygiene property of §5.4.1 (N33) checked on every accepted query.
//!
//! The input is the AQL text up to the first NUL byte; the bytes after it
//! choose the `OFFSET` strategy (`reject`, or `bounded` with its window), the
//! parameter values, the paging members, the targeting and the decomposable
//! aggregates. With no bytes after it, the deployment is the server default:
//! `bounded` at 1000 rows, and every aggregate recombined, `AVG` as its `SUM`
//! and `COUNT`. A
//! refusal is the rewrite doing its job and is never a finding. A panic is a
//! finding, and so is a node query that still carries the patient identifier:
//!
//! - in any string or integer literal of the node query's syntax tree, an
//!   independent check that does not rely on the rewrite's own search; and
//! - rebuilt by a string function, found by giving the node query back its
//!   subject predicate and analysing it again, so the crate's own folding
//!   (`CONCAT`, `CONCAT_WS`, `SUBSTRING`, §5.4.1 "in any position") judges
//!   its own output.

#![no_main]

use std::num::{NonZeroU32, NonZeroUsize};

use libfuzzer_sys::arbitrary::Unstructured;
use libfuzzer_sys::fuzz_target;
use openehr_base::v1_3::base_types::identification::hier_object_id::HierObjectId;
use openehr_federation::aggregate::AggregateFunction;
use openehr_federation::aql::refusal::Refusal;
use openehr_federation::aql::{Analysis, Context, OffsetStrategy, Paging, Targeting, analyse};
use openehr_query::ast::{ClassExprOperand, ContainsExpr, LikeOperand, Primitive, WhereExpr};
use openehr_query::bind::Parameters;
use openehr_query::parser::parse_str;
use openehr_query::printer::{escape_string, to_aql};
use openehr_query::visit::Visit;

/// The `ehr_id` every node query is scoped to.
const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";
/// The default issuing namespace, synthetic (the `2.999` example arc).
const NAMESPACE: &str = "urn:oid:2.999.1";
/// The value a parameter takes when the input supplies none.
const SENTINEL: &str = "sentinel-4711";
/// The rows the bounded strategy asks of one node when the input draws no
/// window: the server's default `federation.max_offset_window`.
const WINDOW: NonZeroU32 = NonZeroU32::MIN.saturating_add(999);

fuzz_target!(|data: &[u8]| {
    let (text, tail) = match data.iter().position(|byte| *byte == 0) {
        Some(nul) => (
            data.get(..nul).unwrap_or_default(),
            data.get(nul + 1..).unwrap_or_default(),
        ),
        None => (data, &[][..]),
    };
    let Ok(aql) = std::str::from_utf8(text) else {
        return;
    };
    let mut choices = Unstructured::new(tail);
    let strategy = offset_strategy(&mut choices);
    let parameters = parameters(aql, &mut choices);
    let paging = Paging {
        offset: choices.arbitrary().unwrap_or(None),
        fetch: choices.arbitrary().unwrap_or(None),
    };
    let context = context(&mut choices)
        .with_offset_strategy(strategy)
        .with_decomposable_aggregates(decomposable(&mut choices));
    let Ok(Analysis::Patient(query)) = analyse(aql, &parameters, paging, &context) else {
        return;
    };
    let value = query.subject().value();
    if value.is_empty() {
        return;
    }
    let Ok(ehr_id) = HierObjectId::new(EHR_ID) else {
        return;
    };
    let node = query.for_node(&ehr_id);
    let Ok(tree) = parse_str(node.aql()) else {
        panic!("a node query the printer wrote does not parse back");
    };
    let mut literals = Literals {
        value,
        found: false,
    };
    literals.visit_select_query(&tree);
    assert!(
        !literals.found,
        "a node query carries the identifier in a literal"
    );

    let Some(variable) = ehr_variable(&tree.from) else {
        return;
    };
    // NOTE: §5.4.1, the probe is conclusive only when nothing the rewrite
    // itself inserted (the EHR containment and the ehr_id scope) holds the value.
    let inserted = format!("EHR {variable} {variable}/ehr_id/value='{EHR_ID}'");
    if inserted.contains(value) {
        return;
    }
    let Ok(predicate) = parse_str(&format!(
        "SELECT {variable}/ehr_id/value FROM EHR {variable} \
         WHERE {variable}/ehr_status/subject/external_ref/id/value = '{}'",
        escape_string(value)
    )) else {
        return;
    };
    let Some(subject) = predicate.where_ else {
        return;
    };
    let mut probe = tree;
    probe.where_ = Some(match probe.where_.take() {
        Some(scope) => WhereExpr::And(Box::new(subject), Box::new(scope)),
        None => subject,
    });
    let again = analyse(
        &to_aql(&probe),
        &Parameters::new(),
        Paging::default(),
        &context
            .clone()
            .with_default_namespace(query.subject().namespace()),
    );
    assert!(
        !matches!(
            again,
            Err(Refusal::IdentifierElsewhere { .. } | Refusal::UnfoldableFunction { .. })
        ),
        "a node query rebuilds the identifier the rewrite consumed"
    );
});

/// One value for every `$name` the text uses, chosen by the input.
fn parameters(aql: &str, choices: &mut Unstructured<'_>) -> Parameters {
    let mut parameters = Parameters::new();
    for name in parameter_names(aql) {
        if choices.is_empty() {
            parameters.insert(&name, Primitive::String(SENTINEL.to_owned()));
            continue;
        }
        match choices.int_in_range(0..=4_u8).unwrap_or(0) {
            0 => parameters.insert(
                &name,
                Primitive::String(choices.arbitrary().unwrap_or_default()),
            ),
            1 => parameters.insert(&name, Primitive::Integer(choices.arbitrary().unwrap_or(0))),
            2 => parameters.insert(&name, Primitive::Real(choices.arbitrary().unwrap_or(0.0))),
            3 => parameters.insert(
                &name,
                Primitive::Boolean(choices.arbitrary().unwrap_or(false)),
            ),
            _ => parameters.insert_not_a_literal(&name),
        }
    }
    parameters
}

/// The parameter names the text writes, without their `$`.
fn parameter_names(aql: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = aql;
    while let Some(dollar) = rest.find('$') {
        let after = rest.get(dollar + 1..).unwrap_or_default();
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
        rest = after;
    }
    names
}

/// How `OFFSET k > 0` is answered (§11.6.2, N39), chosen by the input: an odd
/// first byte refuses every such page, and anything else bounds `k + n` by a
/// window drawn from the next bytes, or by [`WINDOW`] when they draw zero.
fn offset_strategy(choices: &mut Unstructured<'_>) -> OffsetStrategy {
    // NOTE: no specification governs this: our own design; an exhausted input
    // reads `false` and a zero window, so a bare seed runs the server default.
    let reject: bool = choices.arbitrary().unwrap_or(false);
    if reject {
        return OffsetStrategy::Reject;
    }
    let window = choices.int_in_range(0..=2 * WINDOW.get()).unwrap_or(0);
    OffsetStrategy::Bounded {
        max_window: NonZeroU32::new(window).unwrap_or(WINDOW),
    }
}

/// The aggregates recombined across nodes (§11.6.3), chosen by the input:
/// every one, the server default, unless the next byte is odd, which declares
/// none.
fn decomposable(choices: &mut Unstructured<'_>) -> Vec<AggregateFunction> {
    // NOTE: no specification governs this: our own design; an exhausted input
    // reads `false`, so a bare seed runs the server default.
    let none: bool = choices.arbitrary().unwrap_or(false);
    if none {
        Vec::new()
    } else {
        AggregateFunction::ALL.to_vec()
    }
}

/// The deployment and request context, chosen by the input.
fn context(choices: &mut Unstructured<'_>) -> Context {
    let targeting = match choices.int_in_range(0..=3_u8).unwrap_or(0) {
        1 => Targeting::Directed {
            endpoints: NonZeroUsize::MIN,
        },
        2 => Targeting::Directed {
            endpoints: NonZeroUsize::MIN.saturating_add(1),
        },
        3 => Targeting::Localized,
        _ => Targeting::AskAll,
    };
    let context = Context::new(targeting);
    // NOTE: an exhausted input reads `false`, so a bare seed keeps the default
    // namespace and reaches the hygiene property instead of a refusal.
    let without_default: bool = choices.arbitrary().unwrap_or(false);
    if without_default {
        context
    } else {
        context.with_default_namespace(NAMESPACE)
    }
}

/// The first variable the containment binds to `EHR`.
fn ehr_variable(from: &ContainsExpr) -> Option<String> {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            if let ClassExprOperand::Class {
                rm_type,
                variable: Some(variable),
                ..
            } = operand
                && rm_type == "EHR"
            {
                return Some(variable.clone());
            }
            contains
                .as_ref()
                .and_then(|constraint| ehr_variable(&constraint.expr))
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            ehr_variable(left).or_else(|| ehr_variable(right))
        }
    }
}

/// Whether any literal of a tree carries `value`, the scope literal excepted.
struct Literals<'v> {
    value: &'v str,
    found: bool,
}

impl Literals<'_> {
    fn check(&mut self, text: &str) {
        if text != EHR_ID && text.contains(self.value) {
            self.found = true;
        }
    }
}

impl<'ast> Visit<'ast> for Literals<'_> {
    fn visit_primitive(&mut self, node: &'ast Primitive) {
        match node {
            Primitive::String(text) => self.check(text),
            Primitive::Integer(number) => self.check(&number.to_string()),
            Primitive::Real(_) | Primitive::Boolean(_) | Primitive::Null => {}
        }
    }

    fn visit_like_operand(&mut self, node: &'ast LikeOperand) {
        if let LikeOperand::String(text) = node {
            self.check(text);
        }
    }
}
