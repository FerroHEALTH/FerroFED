// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The ITS-REST subset of the result-set schema held to the document §9.1
//! names as normative: the `ResultSet`, `ResultSetMetadata`,
//! `ResultSetColumn` and `ResultSetRow` schemas of ITS-REST Release-1.1.0
//! `computable/OAS/query-validation.openapi.yaml`. A drift on either side, a
//! member added, dropped, retyped, required or closed, fails here.

use crate::support::{self, Shape};

/// The one member the specification adds to `meta` (§9.1, "this
/// specification uses that extension point once").
const FEDERATION: &str = "federation";

/// `shape` with the `federation` member taken out, after checking the
/// schema carries it as the required member §9.1 and N17 make it.
fn without_federation(mut shape: Shape) -> Shape {
    assert!(
        shape.properties.remove(FEDERATION).is_some(),
        "§9.1: the schema's meta carries the federation member"
    );
    let before = shape.required.len();
    shape.required.retain(|name| name != FEDERATION);
    assert_eq!(
        before - 1,
        shape.required.len(),
        "§9.1, N17: the schema's meta requires the federation member"
    );
    shape
}

#[test]
fn the_result_set_is_the_its_rest_result_set() {
    let its_rest = support::its_rest_shape("ResultSet").expect("ITS-REST defines ResultSet");
    let mut schema =
        support::schema_shape(support::RESULT_SET_SCHEMA, "").expect("the schema has a root");
    let meta = schema
        .properties
        .remove("meta")
        .expect("the schema's root has a meta member");
    schema
        .properties
        .insert("meta".to_owned(), without_federation(meta));
    assert_eq!(
        its_rest, schema,
        "§9.1: the root restates ITS-REST ResultSet, member for member"
    );
}

#[test]
fn the_metadata_is_the_its_rest_metadata_plus_federation() {
    let its_rest =
        support::its_rest_shape("ResultSetMetadata").expect("ITS-REST defines ResultSetMetadata");
    let schema = support::schema_shape(
        support::RESULT_SET_SCHEMA,
        "/$defs/federatedResultSetMetadata",
    )
    .expect("the schema defines the federated metadata");
    assert_eq!(
        its_rest,
        without_federation(schema),
        "§9.1: meta is ITS-REST ResultSetMetadata with the one federation member"
    );
}

#[test]
fn the_column_is_the_its_rest_column() {
    let its_rest =
        support::its_rest_shape("ResultSetColumn").expect("ITS-REST defines ResultSetColumn");
    let schema =
        support::schema_shape(support::RESULT_SET_SCHEMA, "/$defs/itsRest/resultSetColumn")
            .expect("the schema restates the column");
    assert_eq!(
        its_rest, schema,
        "§9.1: $defs/itsRest/resultSetColumn restates ITS-REST ResultSetColumn"
    );
}

#[test]
fn the_row_is_the_its_rest_row() {
    let its_rest = support::its_rest_shape("ResultSetRow").expect("ITS-REST defines ResultSetRow");
    let schema = support::schema_shape(support::RESULT_SET_SCHEMA, "/$defs/itsRest/resultSetRow")
        .expect("the schema restates the row");
    assert_eq!(
        its_rest, schema,
        "§9.1: $defs/itsRest/resultSetRow restates ITS-REST ResultSetRow"
    );
}
