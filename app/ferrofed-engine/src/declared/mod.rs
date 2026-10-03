// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Declared values held to their declared kind on a single-node route
//! (§5.4.1, N33).
//!
//! A routed request carries the client's path, and the client's value of
//! each header and query parameter the matched ITS-REST operation declares
//! ([`crate::hygiene`]). The route resolves nothing, so the outbound gate has
//! no identifier to compare those values against. What the gateway does
//! instead, before anything is sent ([`held`]):
//!
//! - each path identifier parses as the openEHR identifier it names, and the
//!   path then travels as the client sent it;
//! - each query value matches the kind `openehr-its`'s parameter table
//!   states for it ([`ParamKind`]): a date-time is a date-time, an
//!   enumerated value is one of the listed values;
//! - `Accept`, `Content-Type` and `Prefer` are parsed by their own grammars
//!   and composed as listed values, so a node receives the operation's own
//!   spelling and never the client's text;
//! - every other declared header matches its kind, as a query value does.
//!
//! A value of a structured kind carries only what its kind admits. A
//! malformed value is a [`Refusal`] that names a header by its declared name
//! and a path or query parameter by its position and declared name, never by
//! its value. A kind the table states as free text ([`is_free_text`]) admits
//! any value, so the gateway cannot classify it and forwards it as received.

mod negotiate;
mod path;

use std::fmt;

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, HeaderValue};
use openehr_base::v1_3::base_types::identification::lexical::is_uuid;
use openehr_base::v1_3::foundation_types::time::iso8601_date::Iso8601Date;
use openehr_base::v1_3::foundation_types::time::iso8601_date_time::Iso8601DateTime;
use openehr_base::validate::Validate;
use openehr_its::rest::routes::{Param, ParamKind, RouteMatch};

use crate::hygiene;

/// The `Prefer` request header (RFC 7240 §2).
const PREFER: &str = "prefer";

// NOTE: §5.4.1, N33: whether a forwarded client value is composed is unresolved (#212), so If-Match,
// openehr-audit-details, openehr-item-tag, openehr-template-id, openehr-version, openehr-version-item-tag,
// the path key, path, tag_key, tag_target_path and tag_value, free text in the EHR area, pass unclassified.
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

/// Holds the declared values of a routed request to their declared kinds,
/// and returns the headers the route sends for `headers`.
///
/// The path parameters of `operation` are read first, then each parameter of
/// `query` the operation declares, percent-decoded, then each header of
/// `headers` the route forwards. A parameter or header the route does not
/// forward is not read here: [`hygiene::forwarded_query`] refuses the one and
/// [`hygiene::forwarded_headers`] strips the other.
///
/// The returned headers are the forwarded ones, with `Accept`,
/// `Content-Type` and `Prefer` composed as listed values. An operation that
/// lists `Accept` values receives the first listed one when the client sent
/// no `Accept`, as for `*/*`.
///
/// # Errors
///
/// Returns [`Refusal::Malformed`] for the first value that does not match its
/// kind, [`Refusal::NotAcceptable`] for an `Accept` that admits no listed
/// media type, and [`Refusal::UnsupportedMediaType`] for a `Content-Type`
/// that names none.
pub fn held(
    operation: &RouteMatch,
    query: Option<&str>,
    headers: &HeaderMap,
) -> Result<HeaderMap, Refusal> {
    path::held(operation)?;
    if let Some(query) = query {
        query_values(operation, query)?;
    }
    composed(operation, headers, Strictness::Refuse)
}

/// The headers the ask-all probe sends for `headers` under its own
/// `operation`.
///
/// Each is composed as [`held`] composes it, and a header that would be
/// refused is left out; for `Accept`, the first listed value is sent.
///
/// The probe is the gateway's own read under an operation of its own, whose
/// listed values may differ from those of the operation the client
/// addressed, and the client's request was held to its own operation before
/// any probe.
#[must_use]
pub fn fitting(operation: &RouteMatch, headers: &HeaderMap) -> HeaderMap {
    composed(operation, headers, Strictness::LeaveOut).unwrap_or_default()
}

/// What a header that does not fit its operation comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Strictness {
    /// The request is refused.
    Refuse,
    /// The header is left out.
    LeaveOut,
}

/// One header as the route sends it.
enum Sent {
    /// The client's field lines, each held to its kind.
    Lines(Vec<HeaderValue>),
    /// One value the gateway composed.
    Composed(HeaderValue),
    /// Nothing: the client stated nothing the operation lists.
    Nothing,
}

/// The headers the route sends for `client` under `operation`.
fn composed(
    operation: &RouteMatch,
    client: &HeaderMap,
    strictness: Strictness,
) -> Result<HeaderMap, Refusal> {
    let forwarded = hygiene::forwarded_headers(operation, client);
    let mut sent = HeaderMap::new();
    for name in forwarded.keys() {
        let Some(param) = operation.header_param(name.as_str()) else {
            continue;
        };
        let lines: Vec<&HeaderValue> = forwarded.get_all(name).iter().collect();
        match (header(param, name, &lines), strictness) {
            (Ok(Sent::Lines(lines)), _) => {
                for line in lines {
                    sent.append(name.clone(), line);
                }
            }
            (Ok(Sent::Composed(value)), _) => {
                sent.insert(name.clone(), value);
            }
            (Ok(Sent::Nothing), _) | (Err(_), Strictness::LeaveOut) => {}
            (Err(refused), Strictness::Refuse) => return Err(refused),
        }
    }
    if !sent.contains_key(ACCEPT)
        && let Some(param) = operation.header_param(ACCEPT.as_str())
        && let ParamKind::Enum(offered) = param.kind
        && let Some(value) = negotiate::accept(offered, &[]).and_then(listed)
    {
        sent.insert(ACCEPT, value);
    }
    Ok(sent)
}

/// How the route sends the header `name`, declared as `param`, of the
/// client's field lines `lines`.
fn header(param: &Param, name: &HeaderName, lines: &[&HeaderValue]) -> Result<Sent, Refusal> {
    match param.kind {
        ParamKind::Enum(offered) if *name == ACCEPT => negotiate::accept(offered, lines)
            .and_then(listed)
            .map(Sent::Composed)
            .ok_or(Refusal::NotAcceptable { offered }),
        ParamKind::Enum(accepted) if *name == CONTENT_TYPE => {
            negotiate::content_type(accepted, lines)
                .and_then(listed)
                .map(Sent::Composed)
                .ok_or(Refusal::UnsupportedMediaType { accepted })
        }
        ParamKind::Enum(declared) if name.as_str() == PREFER => {
            Ok(match negotiate::prefer(declared, lines) {
                Some(value) => HeaderValue::try_from(value).map_or(Sent::Nothing, Sent::Composed),
                None => Sent::Nothing,
            })
        }
        kind => {
            if lines.iter().all(|line| header_fits(&kind, line)) {
                Ok(Sent::Lines(
                    lines.iter().map(|line| (*line).clone()).collect(),
                ))
            } else {
                Err(Refusal::Malformed(MalformedValue {
                    carrier: Carrier::Header { name: param.name },
                    expected: Expected::Kind(kind),
                }))
            }
        }
    }
}

/// The field value of the listed value `value`, or `None` for a listed value
/// that is no field value, which is never sent.
fn listed(value: &'static str) -> Option<HeaderValue> {
    HeaderValue::try_from(value).ok()
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
                expected: Expected::Kind(param.kind),
            });
        }
    }
    Ok(())
}

/// Whether the field line `value` of a header of `kind` matches it.
///
/// A header array is a comma-separated list (OAS 3.0.3 §Style Values,
/// `simple`). A value that is not visible ASCII is free text or no match.
fn header_fits(kind: &ParamKind, value: &HeaderValue) -> bool {
    if is_free_text(kind) {
        return true;
    }
    value.to_str().is_ok_and(|text| fits(kind, text, true))
}

/// Whether `value` matches `kind`; `list` reads an array value as a
/// comma-separated list, and otherwise as one item.
fn fits(kind: &ParamKind, value: &str, list: bool) -> bool {
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

/// Why a routed request's declared values keep it from being sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// A path, query or header value does not match its declared kind.
    #[error(transparent)]
    Malformed(#[from] MalformedValue),
    /// The `Accept` header admits none of the media types the operation
    /// lists (RFC 9110 §12.5.1, §12.4.1).
    #[error(
        "the Accept header admits none of {}, the media types the ITS-REST operation answers in, so the request was not sent",
        .offered.join(", ")
    )]
    NotAcceptable {
        /// The media types the operation lists.
        offered: &'static [&'static str],
    },
    /// The `Content-Type` header names none of the media types the operation
    /// lists, or a parameter other than a `utf-8` charset (RFC 9110 §8.3).
    #[error(
        "the Content-Type header is not one of {}, the media types the ITS-REST operation takes, with at most a utf-8 charset, so the request was not sent",
        .accepted.join(", ")
    )]
    UnsupportedMediaType {
        /// The media types the operation lists.
        accepted: &'static [&'static str],
    },
}

/// Where a declared value that does not match its kind travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carrier {
    /// A path parameter.
    Path {
        /// Its position among the path's parameters, counted from 1.
        position: usize,
        /// The name the template declares it under.
        name: &'static str,
    },
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
            Self::Path { position, name } => write!(f, "path parameter {position} ({name})"),
            Self::Query { position, name } => write!(f, "query parameter {position} ({name})"),
            Self::Header { name } => write!(f, "the {name} header"),
        }
    }
}

/// What a declared value must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expected {
    /// A value of the kind the parameter table states.
    Kind(ParamKind),
    /// An openEHR `OBJECT_VERSION_ID`.
    ObjectVersionId,
    /// An openEHR `UID_BASED_ID`: an `OBJECT_VERSION_ID` or a
    /// `HIER_OBJECT_ID`.
    UidBasedId,
}

impl fmt::Display for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Kind(kind) => Described(kind).fmt(f),
            Self::ObjectVersionId => f.write_str("an openEHR OBJECT_VERSION_ID"),
            Self::UidBasedId => f.write_str("an openEHR OBJECT_VERSION_ID or HIER_OBJECT_ID"),
        }
    }
}

/// A declared value that does not match what the ITS-REST operation
/// declares for it, so the request is refused before anything is sent
/// (§5.4.1, N33).
///
/// It names where the value travelled and what it must be, never the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "{carrier} is not {expected}, which the ITS-REST operation declares for it, so the request was not sent"
)]
pub struct MalformedValue {
    carrier: Carrier,
    expected: Expected,
}

impl MalformedValue {
    /// Where the value travelled.
    #[must_use]
    pub fn carrier(&self) -> Carrier {
        self.carrier
    }

    /// What the operation declares the value must be.
    #[must_use]
    pub fn expected(&self) -> Expected {
        self.expected
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

    use super::path::{PathValue, expected};
    use super::{Carrier, Expected, MalformedValue, Refusal, held, is_free_text};
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

    fn create() -> RouteMatch {
        operation(&Method::POST, &format!("{EHR}/composition"))
    }

    fn headers(lines: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in lines {
            map.append(*name, value.parse().expect("a test header value"));
        }
        map
    }

    /// The value of `name` that `held` sends for `lines` under `operation`.
    fn sent(operation: &RouteMatch, lines: &[(&'static str, &str)], name: &str) -> Option<String> {
        held(operation, None, &headers(lines))
            .expect("the headers are held")
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    fn carrier(refused: &Result<HeaderMap, Refusal>) -> Option<Carrier> {
        match refused {
            Err(Refusal::Malformed(malformed)) => Some(malformed.carrier()),
            _ => None,
        }
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
            assert!(
                held(&directory(), Some(&query), &HeaderMap::new()).is_ok(),
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
            let shown = refused
                .clone()
                .map_or_else(|refusal| refusal.to_string(), |_| String::new());
            assert_eq!(
                Some(Carrier::Query {
                    position: 2,
                    name: "version_at_time"
                }),
                carrier(&refused),
                "{value}"
            );
            assert!(!shown.contains("4711"), "{shown}");
        }
    }

    // conformance: CP-26
    #[test]
    fn any_media_type_reaches_the_node_as_the_first_listed() {
        for lines in [vec![("accept", "*/*")], vec![]] {
            assert_eq!(
                Some("application/json".to_owned()),
                sent(&directory(), &lines, "accept"),
                "{lines:?}"
            );
        }
    }

    // conformance: CP-26
    #[test]
    fn an_accept_list_reaches_the_node_as_its_best_listed_match() {
        let lines = [(
            "accept",
            "text/html, application/xml;q=0.9, application/json;q=0.5, */*;q=0.1",
        )];
        assert_eq!(
            Some("application/xml".to_owned()),
            sent(&directory(), &lines, "accept")
        );
        let lines = [
            ("accept", "application/json; patient=4711"),
            ("accept", "application/xml;q=0.4"),
        ];
        assert_eq!(
            Some("application/xml".to_owned()),
            sent(&directory(), &lines, "accept")
        );
    }

    // conformance: CP-26
    #[test]
    fn an_accept_that_admits_nothing_listed_is_not_acceptable() {
        let refused = held(&directory(), None, &headers(&[("accept", "text/html")]));
        assert!(
            matches!(refused, Err(Refusal::NotAcceptable { .. })),
            "{refused:?}"
        );
    }

    // conformance: CP-26
    #[test]
    fn a_utf_8_charset_passes_and_is_dropped() {
        let lines = [("content-type", "application/json; charset=UTF-8")];
        assert_eq!(
            Some("application/json".to_owned()),
            sent(&create(), &lines, "content-type")
        );
    }

    // conformance: CP-26
    #[test]
    fn an_unlisted_content_type_is_unsupported() {
        for value in [
            "text/plain",
            "application/json; patient=4711",
            "application/json; charset=latin1",
        ] {
            let refused = held(&create(), None, &headers(&[("content-type", value)]));
            assert!(
                matches!(refused, Err(Refusal::UnsupportedMediaType { .. })),
                "{value}: {refused:?}"
            );
            let shown = refused.map_or_else(|refusal| refusal.to_string(), |_| String::new());
            assert!(!shown.contains("4711"), "{shown}");
        }
    }

    // conformance: CP-26
    #[test]
    fn prefer_reaches_the_node_as_its_listed_preferences_only() {
        let lines = [(
            "prefer",
            "return=representation; patient=4711, patient=4711",
        )];
        assert_eq!(
            Some("return=representation".to_owned()),
            sent(&create(), &lines, "prefer")
        );
        for value in ["patient=4711", "return=4711", "RETURN=MINIMAL"] {
            assert_eq!(
                None,
                sent(&create(), &[("prefer", value)], "prefer"),
                "RFC 7240 §2: an unknown preference is ignored, never refused: {value}"
            );
        }
    }

    // conformance: CP-26
    #[test]
    fn a_free_text_parameter_passes_unclassified() {
        let tags = operation(&Method::GET, &format!("{EHR}/tags"));
        let query = "tag_key=4711&tag_value=O%27Sentinel-4711&tag_target_path=%2Fcontent";
        assert!(held(&tags, Some(query), &HeaderMap::new()).is_ok());
        let query = "path=O%27Sentinel-4711";
        assert!(held(&directory(), Some(query), &HeaderMap::new()).is_ok());
        let update = operation(
            &Method::PUT,
            &format!("{EHR}/composition/8849182c-82ad-4088-a07f-48ead4180515"),
        );
        let lines = [("if-match", "4711"), ("openehr-audit-details", "a=1, b=2")];
        assert_eq!(Some("4711".to_owned()), sent(&update, &lines, "if-match"));
    }

    #[test]
    fn an_undeclared_or_withheld_value_is_left_to_the_gate() {
        let sent = headers(&[("x-patient", "4711"), ("authorization", "Bearer 4711")]);
        let held = held(&directory(), Some("patient=4711"), &sent).expect("held");
        assert!(
            held.get("x-patient").is_none() && held.get("authorization").is_none(),
            "{held:?}"
        );
    }

    #[test]
    fn the_probe_sends_what_fits_its_own_operation() {
        let probe = operation(&Method::GET, EHR);
        let sent = headers(&[("accept", "application/openehr.wt.flat+json")]);
        let kept = super::fitting(&probe, &sent);
        assert_eq!(
            Some("application/json"),
            kept.get("accept").and_then(|value| value.to_str().ok())
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

    #[test]
    fn the_message_names_what_the_value_must_be() {
        let malformed = MalformedValue {
            carrier: Carrier::Query {
                position: 1,
                name: "version_at_time",
            },
            expected: Expected::Kind(ParamKind::DateTime),
        };
        assert_eq!(
            "query parameter 1 (version_at_time) is not an extended ISO 8601 date-time, which the ITS-REST operation declares for it, so the request was not sent",
            malformed.to_string()
        );
    }

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

    #[test]
    fn the_listed_free_text_parameters_are_the_tables() {
        let mut free = BTreeSet::new();
        for ((_, template, _), params) in ROUTES.iter().zip(ROUTE_PARAMS) {
            if !template.starts_with("/ehr/{ehr_id}") {
                continue;
            }
            for param in *params {
                let unclassified = match param.location {
                    ParamLocation::Path => expected(param) == PathValue::Free,
                    ParamLocation::Query | ParamLocation::Header => is_free_text(&param.kind),
                    ParamLocation::Cookie => false,
                };
                if unclassified {
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
