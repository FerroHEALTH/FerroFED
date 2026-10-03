// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Declared values held to their declared kind on a single-node route
//! (§5.4.1, N33).
//!
//! A routed request forwards the client's value of each header and query
//! parameter the matched ITS-REST operation declares ([`crate::hygiene`]).
//! The route resolves nothing, so the outbound gate has no identifier to
//! compare a forwarded value against. What the gateway can do is hold every
//! declared value to the kind `openehr-its`'s parameter table states for it
//! ([`ParamKind`]): a date-time is a date-time, an enumerated value is one of
//! the listed values, a UUID is a UUID. A value of such a structured kind
//! carries only what its kind admits, and a value that does not match its
//! kind is refused before anything is sent ([`held`]). A header is named by
//! the name the table declares, and a query parameter by its position and
//! that name, never by its value.
//!
//! A kind the table states as free text ([`is_free_text`]) admits any value,
//! so the gateway cannot classify it and forwards it as received.

use std::fmt;

use http::{HeaderMap, HeaderValue};
use openehr_base::v1_3::foundation_types::time::iso8601_date::Iso8601Date;
use openehr_base::v1_3::foundation_types::time::iso8601_date_time::Iso8601DateTime;
use openehr_base::validate::Validate;
use openehr_its::rest::routes::{Param, ParamKind, RouteMatch};

use crate::hygiene;

// NOTE: §5.4.1, N33: whether a forwarded client value is composed is unresolved (#212), so If-Match,
// openehr-audit-details, openehr-item-tag, openehr-template-id, openehr-version, openehr-version-item-tag,
// path, tag_key, tag_target_path and tag_value, free text in the EHR area, pass unclassified.
/// Whether a value of `kind` is free text the gateway cannot classify: a
/// string with no `format`, or one whose `format` no kind names, an object, a
/// schema with no `type`, or an array of any of these.
#[must_use]
pub fn is_free_text(kind: &ParamKind) -> bool {
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

/// Holds the declared values of a routed request to their declared kinds:
/// each parameter of `query` and each header of `headers` that `operation`
/// declares, as the route would forward them.
///
/// A query value is read percent-decoded. A parameter or header the route
/// does not forward is not read here: [`hygiene::forwarded_query`] refuses
/// the one and [`hygiene::forwarded_headers`] strips the other.
///
/// # Errors
///
/// Returns [`MalformedValue`] for the first declared value, query parameters
/// first, that does not match its kind.
pub fn held(
    operation: &RouteMatch,
    query: Option<&str>,
    headers: &HeaderMap,
) -> Result<(), MalformedValue> {
    if let Some(query) = query {
        query_values(operation, query)?;
    }
    header_values(operation, headers)
}

/// The field lines of `headers` that match the kind `operation` declares for
/// them, and every field line `operation` does not declare.
///
/// The ask-all probe sends the client's headers under an operation of its
/// own, whose kinds may differ from those of the operation the client
/// addressed; a value that does not fit the probe's operation is left out of
/// the probe.
#[must_use]
pub fn fitting(operation: &RouteMatch, headers: &HeaderMap) -> HeaderMap {
    let mut kept = HeaderMap::new();
    for (name, value) in headers {
        let fits = operation
            .header_param(name.as_str())
            .is_none_or(|param| header_fits(param, value));
        if fits {
            kept.append(name.clone(), value.clone());
        }
    }
    kept
}

/// Holds each declared parameter of `query` to its kind.
fn query_values(operation: &RouteMatch, query: &str) -> Result<(), MalformedValue> {
    let pairs = query.split('&').filter(|pair| !pair.is_empty());
    for (index, pair) in pairs.enumerate() {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let Some(param) = operation.query_key(&hygiene::percent_decoded(name)) else {
            continue;
        };
        let list = !param.explode;
        if !fits(&param.kind, &hygiene::percent_decoded(value), list) {
            return Err(MalformedValue {
                carrier: Carrier::Query {
                    position: index.saturating_add(1),
                    name: param.name,
                },
                kind: param.kind,
            });
        }
    }
    Ok(())
}

/// Holds each header of `headers` the route forwards to its kind.
fn header_values(operation: &RouteMatch, headers: &HeaderMap) -> Result<(), MalformedValue> {
    let forwarded = hygiene::forwarded_headers(operation, headers);
    for (name, value) in &forwarded {
        let Some(param) = operation.header_param(name.as_str()) else {
            continue;
        };
        if !header_fits(param, value) {
            return Err(MalformedValue {
                carrier: Carrier::Header { name: param.name },
                kind: param.kind,
            });
        }
    }
    Ok(())
}

/// Whether the field line `value` of the declared header `param` matches its
/// kind.
///
/// A header array is a comma-separated list (OAS 3.0.3 §Style Values,
/// `simple`). A value that is not visible ASCII is free text or no match.
fn header_fits(param: &Param, value: &HeaderValue) -> bool {
    if is_free_text(&param.kind) {
        return true;
    }
    value
        .to_str()
        .is_ok_and(|text| fits(&param.kind, text, true))
}

/// Whether `value` matches `kind`; `list` reads an array value as a
/// comma-separated list, and otherwise as one item.
fn fits(kind: &ParamKind, value: &str, list: bool) -> bool {
    match kind {
        ParamKind::Text | ParamKind::Formatted(_) | ParamKind::Object | ParamKind::Unspecified => {
            true
        }
        ParamKind::Enum(values) => values.contains(&value),
        ParamKind::Uuid => uuid::Uuid::try_parse(value).is_ok(),
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

/// Where a declared value that does not match its kind travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    /// A query parameter.
    Query {
        /// Its position in the query string, counted from 1.
        position: usize,
        /// The name the operation declares it under.
        name: &'static str,
    },
    /// A request header.
    Header {
        /// The name the operation declares it under.
        name: &'static str,
    },
}

impl fmt::Display for Carrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Query { position, name } => write!(f, "query parameter {position} ({name})"),
            Self::Header { name } => write!(f, "the {name} header"),
        }
    }
}

/// A declared value that does not match the kind the ITS-REST operation
/// declares for it, so the request is refused before anything is sent
/// (§5.4.1, N33).
///
/// It names where the value travelled and the kind, never the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "{carrier} is not {}, the kind the ITS-REST operation declares for it, so the request was not sent",
    Described(kind)
)]
pub struct MalformedValue {
    carrier: Carrier,
    kind: ParamKind,
}

impl MalformedValue {
    /// Where the value travelled.
    #[must_use]
    pub fn carrier(&self) -> Carrier {
        self.carrier
    }

    /// The kind the operation declares for the value.
    #[must_use]
    pub fn kind(&self) -> ParamKind {
        self.kind
    }
}

/// A kind as the refusal message names it.
struct Described<'a>(&'a ParamKind);

impl fmt::Display for Described<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            ParamKind::Enum(values) => write!(f, "one of {}", values.join(", ")),
            ParamKind::Uuid => f.write_str("a UUID"),
            ParamKind::Date => f.write_str("an extended ISO 8601 date"),
            ParamKind::DateTime => f.write_str("an extended ISO 8601 date-time"),
            ParamKind::Integer => f.write_str("an integer"),
            ParamKind::Number => f.write_str("a number"),
            ParamKind::Boolean => f.write_str("true or false"),
            ParamKind::Array(inner) => write!(f, "a list of items each {}", Described(inner)),
            ParamKind::Text
            | ParamKind::Formatted(_)
            | ParamKind::Object
            | ParamKind::Unspecified => f.write_str("text"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{Carrier, MalformedValue, held, is_free_text};
    use http::{HeaderMap, Method};
    use openehr_its::rest::generated::ehr::{ROUTE_PARAMS, ROUTES};
    use openehr_its::rest::routes::{Lookup, ParamKind, ParamLocation, RouteMatch, lookup};

    const EHR: &str = "/ehr/7d44b88c-4199-4bad-97dc-d78268e01398";

    fn operation(method: &Method, path: &str) -> RouteMatch {
        match lookup(method, path) {
            Lookup::Matched(matched) => matched,
            other => panic!("{method} {path} names no operation: {other:?}"),
        }
    }

    fn directory() -> RouteMatch {
        operation(&Method::GET, &format!("{EHR}/directory"))
    }

    fn headers(lines: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in lines {
            map.append(*name, value.parse().expect("a test header value"));
        }
        map
    }

    // conformance: CP-26
    #[test]
    fn a_well_formed_date_time_is_held() {
        for value in [
            "2015-01-20T19:30:22.765+01:00",
            "2015-01-20T19:30:22Z",
            "2015-01-20T19:30:22",
            "2015-01-20T19%3A30%3A22%2B01%3A00",
        ] {
            let query = format!("version_at_time={value}");
            assert_eq!(
                Ok(()),
                held(&directory(), Some(&query), &HeaderMap::new()),
                "{value}"
            );
        }
    }

    // conformance: CP-26
    #[test]
    fn a_malformed_date_time_is_refused_by_position_never_by_value() {
        for value in [
            "O%27Sentinel-4711",
            "123454711",
            "20150120T193022Z",
            "2015-01-20 19:30:22Z",
            "2015-13-20T19:30:22Z",
            "2015-01-20T19:30:22Z[patient=4711]",
            "2015-01-20T19:30:22Z%5Bpatient%3D4711%5D",
            "",
        ] {
            let query = format!("path=a&version_at_time={value}");
            let refused = held(&directory(), Some(&query), &HeaderMap::new());
            assert_eq!(
                Err(Carrier::Query {
                    position: 2,
                    name: "version_at_time"
                }),
                refused.map_err(|malformed| malformed.carrier()),
                "{value}"
            );
            let shown = refused.map_err(|e| e.to_string()).unwrap_err();
            assert!(!shown.contains("4711"), "{shown}");
        }
    }

    // conformance: CP-26
    #[test]
    fn an_enumerated_header_outside_its_values_is_refused() {
        let create = operation(&Method::POST, &format!("{EHR}/composition"));
        let sent = headers(&[("prefer", "return=minimal")]);
        assert_eq!(Ok(()), held(&create, None, &sent));
        for value in [
            "patient=4711",
            "return=minimal; patient=4711",
            "RETURN=MINIMAL",
        ] {
            let sent = headers(&[("prefer", value)]);
            let refused = held(&create, None, &sent);
            assert_eq!(
                Err(Carrier::Header { name: "Prefer" }),
                refused.map_err(|malformed| malformed.carrier()),
                "{value}"
            );
            let shown = refused.map_err(|e| e.to_string()).unwrap_err();
            assert!(!shown.contains("4711"), "{shown}");
            assert!(shown.contains("return=representation"), "{shown}");
        }
    }

    // conformance: CP-26
    #[test]
    fn a_free_text_parameter_passes_unclassified() {
        let tags = operation(&Method::GET, &format!("{EHR}/tags"));
        let query = "tag_key=4711&tag_value=O%27Sentinel-4711&tag_target_path=%2Fcontent";
        assert_eq!(Ok(()), held(&tags, Some(query), &HeaderMap::new()));
        let query = "path=O%27Sentinel-4711";
        assert_eq!(Ok(()), held(&directory(), Some(query), &HeaderMap::new()));
        let update = operation(&Method::PUT, &format!("{EHR}/composition/u::s::1"));
        let sent = headers(&[("if-match", "4711"), ("openehr-audit-details", "a=1, b=2")]);
        assert_eq!(Ok(()), held(&update, None, &sent));
    }

    #[test]
    fn an_undeclared_or_withheld_value_is_left_to_the_gate() {
        let sent = headers(&[("x-patient", "4711"), ("authorization", "Bearer 4711")]);
        assert_eq!(Ok(()), held(&directory(), Some("patient=4711"), &sent));
    }

    #[test]
    fn every_field_line_of_a_header_is_held() {
        let create = operation(&Method::POST, &format!("{EHR}/composition"));
        let sent = headers(&[("prefer", "return=minimal"), ("prefer", "4711")]);
        assert!(held(&create, None, &sent).is_err());
    }

    #[test]
    fn the_probe_keeps_only_the_values_that_fit_its_operation() {
        let probe = operation(&Method::GET, EHR);
        let sent = headers(&[
            ("accept", "application/openehr.wt.flat+json"),
            ("x-other", "kept"),
        ]);
        let kept = super::fitting(&probe, &sent);
        assert!(kept.get("accept").is_none(), "{kept:?}");
        assert!(kept.get("x-other").is_some(), "{kept:?}");
        let sent = headers(&[("accept", "application/json")]);
        assert_eq!(sent, super::fitting(&probe, &sent));
    }

    #[test]
    fn the_structured_kinds_hold_their_values() {
        let cases: [(ParamKind, &str, bool); 14] = [
            (
                ParamKind::Uuid,
                "7d44b88c-4199-4bad-97dc-d78268e01398",
                true,
            ),
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

    #[test]
    fn the_message_names_the_kind() {
        let malformed = MalformedValue {
            carrier: Carrier::Query {
                position: 1,
                name: "version_at_time",
            },
            kind: ParamKind::DateTime,
        };
        assert_eq!(
            "query parameter 1 (version_at_time) is not an extended ISO 8601 date-time, the kind the ITS-REST operation declares for it, so the request was not sent",
            malformed.to_string()
        );
    }

    /// The free-text parameters of the EHR area, as the module's `// NOTE:`
    /// and the configuration page list them.
    const FREE_TEXT: [(ParamLocation, &str); 10] = [
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

    #[test]
    fn the_listed_free_text_parameters_are_the_tables() {
        let mut free = BTreeSet::new();
        for ((_, template, _), params) in ROUTES.iter().zip(ROUTE_PARAMS) {
            if !template.starts_with("/ehr/{ehr_id}") {
                continue;
            }
            for param in *params {
                let carried =
                    matches!(param.location, ParamLocation::Query | ParamLocation::Header);
                if carried && is_free_text(&param.kind) {
                    free.insert((format!("{:?}", param.location), param.name));
                }
            }
        }
        let listed: BTreeSet<(String, &str)> = FREE_TEXT
            .iter()
            .map(|(location, name)| (format!("{location:?}"), *name))
            .collect();
        assert_eq!(listed, free);
    }
}
