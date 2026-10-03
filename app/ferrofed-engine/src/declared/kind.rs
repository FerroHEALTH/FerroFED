// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The value kinds of `openehr-its`'s parameter table, and whether a value
//! matches its kind (§5.4.1, N33).
//!
//! A kind the table states as free text admits any value, so the gateway
//! cannot classify it and forwards it as received. The tests below hold the
//! register of those parameters, per routed area, to the tables.

use http::HeaderValue;
use openehr_base::v1_3::base_types::identification::lexical::is_uuid;
use openehr_base::v1_3::foundation_types::time::iso8601_date::Iso8601Date;
use openehr_base::v1_3::foundation_types::time::iso8601_date_time::Iso8601DateTime;
use openehr_base::validate::Validate;
use openehr_its::rest::routes::ParamKind;

// NOTE: §5.4.1, N33: whether a forwarded client value is composed is unresolved (#212), so the free text
// of every routed area (the EHR, definition and DEMOGRAPHIC areas and POST /ehr), each listed in a register
// the tests below hold to the openehr-its tables, passes unclassified.
/// Whether a value of `kind` is free text the gateway cannot classify: a
/// string with no `format`, or one whose `format` no kind names, an object, a
/// schema with no `type`, or an array of any of these.
#[must_use]
pub(super) fn is_free_text(kind: &ParamKind) -> bool {
    match kind {
        ParamKind::Text | ParamKind::Formatted(_) | ParamKind::Object | ParamKind::Unspecified => {
            true
        }
        ParamKind::Array(inner) => is_free_text(inner),
        ParamKind::Enum(_)
        | ParamKind::Uuid
        | ParamKind::Date
        | ParamKind::DateTime
        | ParamKind::Integer
        | ParamKind::Number
        | ParamKind::Boolean => false,
    }
}

/// Whether the field line `value` of a header of `kind` matches it.
///
/// A header array is a comma-separated list (OAS 3.0.3 §Style Values,
/// `simple`). A value that is not visible ASCII is free text or no match.
pub(super) fn header_fits(kind: &ParamKind, value: &HeaderValue) -> bool {
    if is_free_text(kind) {
        return true;
    }
    value.to_str().is_ok_and(|text| fits(kind, text, true))
}

/// Whether `value` matches `kind`; `list` reads an array value as a
/// comma-separated list, and otherwise as one item.
pub(super) fn fits(kind: &ParamKind, value: &str, list: bool) -> bool {
    match kind {
        ParamKind::Text | ParamKind::Formatted(_) | ParamKind::Object | ParamKind::Unspecified => {
            true
        }
        ParamKind::Enum(values) => values.contains(&value),
        ParamKind::Uuid => is_uuid(value),
        ParamKind::Date => is_date(value),
        ParamKind::DateTime => is_date_time(value),
        ParamKind::Integer => value.parse::<i64>().is_ok(),
        ParamKind::Number => value.parse::<f64>().is_ok_and(f64::is_finite),
        ParamKind::Boolean => value.parse::<bool>().is_ok(),
        ParamKind::Array(inner) if list => value
            .split(',')
            .all(|item| fits(inner, item.trim_matches([' ', '\t']), false)),
        ParamKind::Array(inner) => fits(inner, value, false),
    }
}

/// Whether `value` is an openEHR `Iso8601_date_time` in the extended format.
///
/// ITS-REST requires a date-time query parameter in the extended ISO 8601
/// format, with the semantics of the BASE `foundation_types.time` package,
/// and an offset only when needed (ITS-REST Overview §Datetime format).
fn is_date_time(value: &str) -> bool {
    let date_time = Iso8601DateTime {
        value: value.to_owned(),
    };
    date_time.invariants().is_empty() && date_time.is_extended()
}

/// Whether `value` is an openEHR `Iso8601_date` in the extended format
/// (ITS-REST Overview §Datetime format).
fn is_date(value: &str) -> bool {
    let date = Iso8601Date {
        value: value.to_owned(),
    };
    date.invariants().is_empty() && date.is_extended()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::is_free_text;
    use crate::declared::path::{PathValue, expected};
    use openehr_its::rest::generated::ehr::{ROUTE_PARAMS, ROUTES};
    use openehr_its::rest::generated::{definition, demographic};
    use openehr_its::rest::routes::{Param, ParamKind, ParamLocation};

    /// The free-text parameters of the EHR area, as the module's `// NOTE:`
    /// and the configuration page list them.
    const FREE_TEXT: [(ParamLocation, &str); 11] = [
        (ParamLocation::Path, "key"),
        (ParamLocation::Header, "If-Match"),
        (ParamLocation::Header, "openehr-audit-details"),
        (ParamLocation::Header, "openehr-item-tag"),
        (ParamLocation::Header, "openehr-template-id"),
        (ParamLocation::Header, "openehr-version"),
        (ParamLocation::Header, "openehr-version-item-tag"),
        (ParamLocation::Query, "path"),
        (ParamLocation::Query, "tag_key"),
        (ParamLocation::Query, "tag_target_path"),
        (ParamLocation::Query, "tag_value"),
    ];

    /// The free-text parameters of `params`, as `(location, name)`.
    fn unclassified(params: &[Param]) -> BTreeSet<(String, &'static str)> {
        params
            .iter()
            .filter(|param| match param.location {
                ParamLocation::Path => expected(param) == PathValue::Free,
                ParamLocation::Query | ParamLocation::Header => is_free_text(&param.kind),
                ParamLocation::Cookie => false,
            })
            .map(|param| (format!("{:?}", param.location), param.name))
            .collect()
    }

    /// `register` as the `(location, name)` set [`unclassified`] builds.
    fn listed(register: &[(ParamLocation, &'static str)]) -> BTreeSet<(String, &'static str)> {
        register
            .iter()
            .map(|(location, name)| (format!("{location:?}"), *name))
            .collect()
    }

    #[test]
    fn the_listed_free_text_parameters_are_the_tables() {
        let mut free = BTreeSet::new();
        for ((_, template, _), params) in ROUTES.iter().zip(ROUTE_PARAMS) {
            if !template.starts_with("/ehr/{ehr_id}") {
                continue;
            }
            free.extend(unclassified(params));
        }
        assert_eq!(listed(&FREE_TEXT), free);
    }

    /// The free-text parameters of the creation of an EHR, `POST /ehr`.
    const CREATION_FREE_TEXT: [(ParamLocation, &str); 2] = [
        (ParamLocation::Header, "openehr-audit-details"),
        (ParamLocation::Header, "openehr-version"),
    ];

    /// The free-text parameters of the definition area.
    const DEFINITION_FREE_TEXT: [(ParamLocation, &str); 7] = [
        (ParamLocation::Path, "qualified_query_name"),
        (ParamLocation::Path, "template_id"),
        (ParamLocation::Path, "version"),
        (ParamLocation::Query, "concept"),
        (ParamLocation::Query, "query_type"),
        (ParamLocation::Query, "template_id"),
        (ParamLocation::Query, "version"),
    ];

    /// The free-text parameters of the DEMOGRAPHIC area, routed where the
    /// deployment configures its endpoint (§7a.1, N32).
    const DEMOGRAPHIC_FREE_TEXT: [(ParamLocation, &str); 10] = [
        (ParamLocation::Path, "key"),
        (ParamLocation::Header, "If-Match"),
        (ParamLocation::Header, "openehr-audit-details"),
        (ParamLocation::Header, "openehr-item-tag"),
        (ParamLocation::Header, "openehr-template-id"),
        (ParamLocation::Header, "openehr-version"),
        (ParamLocation::Header, "openehr-version-item-tag"),
        (ParamLocation::Query, "tag_key"),
        (ParamLocation::Query, "tag_target_path"),
        (ParamLocation::Query, "tag_value"),
    ];

    /// The free-text parameters of the operations of `routes` that `kept`
    /// keeps, by `(method, template, operation_id)`.
    fn free_in(
        routes: &[(&'static str, &'static str, &'static str)],
        params: &[&'static [Param]],
        kept: impl Fn(&str, &str, &str) -> bool,
    ) -> BTreeSet<(String, &'static str)> {
        routes
            .iter()
            .zip(params)
            .filter(|((method, template, id), _)| kept(method, template, id))
            .flat_map(|(_, params)| unclassified(params))
            .collect()
    }

    #[test]
    fn every_routed_operation_forwards_only_listed_free_text() {
        assert_eq!(
            listed(&CREATION_FREE_TEXT),
            free_in(ROUTES, ROUTE_PARAMS, |method, template, _| {
                method == "POST" && template == "/ehr"
            }),
            "§12.4: POST /ehr"
        );
        assert_eq!(
            listed(&DEFINITION_FREE_TEXT),
            free_in(definition::ROUTES, definition::ROUTE_PARAMS, |_, _, _| {
                true
            }),
            "§12.6: the definition area"
        );
        assert_eq!(
            listed(&DEMOGRAPHIC_FREE_TEXT),
            free_in(demographic::ROUTES, demographic::ROUTE_PARAMS, |_, _, _| {
                true
            }),
            "§7a.1, N32: the DEMOGRAPHIC area"
        );
    }

    #[test]
    fn the_structured_kinds_hold_their_values() {
        let cases: [(ParamKind, &str, bool); 15] = [
            (
                ParamKind::Uuid,
                "7d44b88c-4199-4bad-97dc-d78268e01398",
                true,
            ),
            (ParamKind::Uuid, "7d44b88c41994bad97dcd78268e01398", false),
            (ParamKind::Uuid, "4711", false),
            (ParamKind::Date, "2015-01-20", true),
            (ParamKind::Date, "2015-01-20T19:30:22Z", false),
            (ParamKind::Date, "2015-01-20[patient=4711]", false),
            (ParamKind::Integer, "-12", true),
            (ParamKind::Integer, "12a", false),
            (ParamKind::Number, "1.5", true),
            (ParamKind::Number, "NaN", false),
            (ParamKind::Boolean, "true", true),
            (ParamKind::Boolean, "yes", false),
            (ParamKind::Array(&ParamKind::Integer), "1, 2,3", true),
            (ParamKind::Array(&ParamKind::Integer), "1,x", false),
            (ParamKind::Enum(&["a", "b"]), "c", false),
        ];
        for (kind, value, expected) in cases {
            assert_eq!(
                expected,
                super::fits(&kind, value, true),
                "{kind:?} {value}"
            );
        }
    }
}
