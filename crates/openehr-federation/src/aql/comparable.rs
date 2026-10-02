// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Whether a node can order its rows on a selected path.
//!
//! AQL orders rows on a path on the assumption that "data identified by the
//! path … are comparable", through the operators "available to primitives and
//! `Ordered` types" (AQL master03-syntax §ORDER BY). A path is comparable here
//! when the RM shows that every value it can name is one of those: the path is
//! walked from the class its variable is bound to in `FROM`, attribute by
//! attribute, through `openehr-rm`'s static model, where an attribute an RM
//! class lacks is looked up on its subtypes, and a foundation type stays the
//! value type it is. A path that crosses a reference (AQL master03-syntax
//! selects `e/ehr_status/subject/external_ref/id/value` through
//! `EHR.ehr_status`) continues on the class the model names as the
//! reference's target as well as on the reference itself. The walk fails, and
//! the path is not comparable, when:
//!
//! - the path is a bare variable naming a whole RM object (`SELECT c`) of a
//!   class that is not ordered;
//! - an attribute is on no subtype of the type reached so far, including an
//!   attribute after a primitive or past a reference with no named target;
//! - the last attribute is a container, whose value is a collection;
//! - the type reached admits a value that is neither a primitive nor an
//!   `Ordered` type (a `DV_TEXT`, or the `DATA_VALUE` of an `ELEMENT`, which
//!   an archetype narrows but the RM does not).

use std::collections::BTreeSet;

use openehr_query::ast::{ClassExprOperand, ContainsExpr, IdentifiedPath};
use openehr_rm::v1_2::model::{self, Container};

/// Whether every value `path` can name is a primitive or of an `Ordered` type,
/// the values AQL defines an order for (AQL master03-syntax §ORDER BY).
pub(super) fn comparable(path: &IdentifiedPath, from: &ContainsExpr) -> bool {
    let Some(root) = bound_class(from, &path.root).and_then(model::class) else {
        return false;
    };
    let mut types = BTreeSet::from([root.name]);
    let mut collection = false;
    for part in path.path.iter().flat_map(|object| &object.parts) {
        let mut next = BTreeSet::new();
        collection = false;
        for declared in &types {
            for class in std::iter::once(*declared).chain(subtypes(declared).iter().copied()) {
                if let Some(attribute) = model::attribute(class, &part.name) {
                    next.insert(attribute.declared_type);
                    next.extend(attribute.ref_target);
                    collection |= attribute.container != Container::None;
                }
            }
        }
        if next.is_empty() {
            return false;
        }
        types = next;
    }
    !collection && types.iter().all(|declared| ordered(declared))
}

/// The concrete subtypes an attribute of type `declared` may hold: those of
/// an RM class, and none of a foundation type, which is a value type (BASE
/// `master03-primitive_types.adoc` §Overview).
fn subtypes(declared: &str) -> &'static [&'static str] {
    if model::is_foundation_type(declared) {
        &[]
    } else {
        model::descendants(declared)
    }
}

/// Whether every value of the declared type `declared` is a primitive or of
/// an `Ordered` type.
fn ordered(declared: &str) -> bool {
    if model::is_foundation_type(declared) {
        return model::is_primitive(declared) || model::conforms_to_ordered(declared);
    }
    let concrete = model::descendants(declared);
    !concrete.is_empty()
        && concrete
            .iter()
            .all(|class| model::conforms_to_ordered(class))
}

/// The RM class `FROM` binds `variable` to: its class name, or `VERSION` for
/// a version class.
fn bound_class<'a>(from: &'a ContainsExpr, variable: &str) -> Option<&'a str> {
    match from {
        ContainsExpr::Contained { operand, contains } => {
            let bound = match operand {
                ClassExprOperand::Class {
                    rm_type,
                    variable: Some(name),
                    ..
                } if name == variable => Some(rm_type.as_str()),
                ClassExprOperand::Version {
                    variable: Some(name),
                    ..
                } if name == variable => Some("VERSION"),
                ClassExprOperand::Class { .. } | ClassExprOperand::Version { .. } => None,
            };
            bound.or_else(|| {
                contains
                    .as_ref()
                    .and_then(|constraint| bound_class(&constraint.expr, variable))
            })
        }
        ContainsExpr::And(left, right) | ContainsExpr::Or(left, right) => {
            bound_class(left, variable).or_else(|| bound_class(right, variable))
        }
    }
}

#[cfg(test)]
mod tests {
    use openehr_query::ast::{ColumnExpr, SelectQuery};
    use openehr_query::parser::parse_str;

    use super::comparable;

    fn selected(aql: &str) -> Vec<bool> {
        let query: SelectQuery = parse_str(aql).unwrap();
        query
            .select
            .columns
            .iter()
            .map(|column| match &column.column {
                ColumnExpr::Path(path) => comparable(path, &query.from),
                _ => panic!("the fixture selects paths only"),
            })
            .collect()
    }

    #[test]
    fn primitives_and_ordered_data_values_are_comparable() {
        let aql = "SELECT c/uid/value, c/name/value, c/context/start_time, \
                   c/context/start_time/value, e/ehr_id/value, e/time_created, \
                   o/data/events/data/items/value/magnitude, v/commit_audit/time_committed \
                   FROM EHR e CONTAINS VERSION v CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
        assert_eq!(selected(aql), [true; 8]);
    }

    #[test]
    fn objects_unordered_data_values_and_collections_are_not() {
        let aql = "SELECT c, c/name, c/uid, o/data/events/data/items/value, c/content, \
                   c/name/value/length, c/no_such_attribute, x/name/value, \
                   e/ehr_status/other_details \
                   FROM EHR e CONTAINS COMPOSITION c CONTAINS OBSERVATION o";
        assert_eq!(selected(aql), [false; 9]);
    }

    #[test]
    fn a_variable_in_an_and_containment_is_found() {
        let aql = "SELECT o/name/value, a/time FROM EHR e CONTAINS COMPOSITION c \
                   CONTAINS (OBSERVATION o AND ACTION a)";
        assert_eq!(selected(aql), [true, true]);
    }

    #[test]
    fn a_path_across_a_reference_follows_its_ref_target() {
        let aql = "SELECT e/ehr_status/uid/value, e/ehr_status/subject/external_ref/id/value, \
                   e/ehr_status/is_queryable, e/ehr_status/subject, e/ehr_status/no_such_attribute \
                   FROM EHR e";
        assert_eq!(selected(aql), [true, true, true, false, false]);
    }
}
