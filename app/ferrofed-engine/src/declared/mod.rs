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
//! - a body travels with a `Content-Type` the operation lists for it
//!   (`openehr-its`'s `request_media`), the first listed media type when the
//!   client sent none;
//! - every other declared header matches its kind, as a query value does.
//!
//! A value of a structured kind carries only what its kind admits. A
//! malformed value is a [`Refusal`] that names a header by its declared name
//! and a path or query parameter by its position and declared name, never by
//! its value. A kind the table states as free text admits
//! any value, so the gateway cannot classify it and forwards it as received.

//!
//! `headers` composes the headers, [`query`] reads the query string, `path`
//! parses the path identifiers and `kind` decides whether a value matches its
//! kind; this module holds them together and names a refusal.

mod headers;
mod kind;
mod negotiate;
mod path;
pub mod query;

use std::fmt;

use http::header::CONTENT_TYPE;
use http::{HeaderMap, HeaderValue};
use openehr_its::rest::routes::{ParamKind, RouteMatch};

use crate::declared::headers::{Strictness, body_media_type, composed};

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
/// A non-empty `body` always travels with a `Content-Type` the operation
/// lists ([`RouteMatch::request_media`]): the client's, as its listed value,
/// or, when the client sent none, the first media type the operation's body
/// is declared in.
///
/// [`hygiene::forwarded_query`]: crate::hygiene::forwarded_query
/// [`hygiene::forwarded_headers`]: crate::hygiene::forwarded_headers
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
    body: &[u8],
) -> Result<HeaderMap, Refusal> {
    holding(operation, (query, headers, body), Strictness::Refuse)
}

/// Holds the declared values of the ask-all probe, the gateway's own request
/// under its own `operation`, and returns the headers it sends for `headers`.
///
/// Each header is composed as [`held`] composes it, and one that would be
/// refused is left out; for `Accept`, the first listed value is sent. The
/// path, the query and the body are held as [`held`] holds them.
///
/// The probe's operation may list other values than the operation the client
/// addressed, and the client's request was held to its own operation before
/// any probe.
///
/// # Errors
///
/// Returns the [`Refusal`] of [`held`] for the path, the query or the body;
/// never one for a header.
pub fn fitting(
    operation: &RouteMatch,
    query: Option<&str>,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<HeaderMap, Refusal> {
    holding(operation, (query, headers, body), Strictness::LeaveOut)
}

/// Holds the `Content-Type` the client sent with a request to `operation` to
/// the media types the operation lists, and returns the listed media type it
/// names, or `None` when the client sent none.
///
/// The value is negotiated as [`held`] negotiates it, so a request the
/// gateway answers itself is held to the same rule as one it routes; no other
/// header, path or query value is read.
///
/// # Errors
///
/// Returns [`Refusal::UnsupportedMediaType`] for a `Content-Type` that names
/// no listed media type, or carries a parameter other than a `utf-8`
/// charset (RFC 9110 §8.3).
pub fn content_type(
    operation: &RouteMatch,
    headers: &HeaderMap,
) -> Result<Option<HeaderValue>, Refusal> {
    let mut sent = HeaderMap::new();
    for line in headers.get_all(CONTENT_TYPE) {
        sent.append(CONTENT_TYPE, line.clone());
    }
    if sent.is_empty() {
        return Ok(None);
    }
    let composed = composed(operation, &sent, Strictness::Refuse)?;
    match composed.get(CONTENT_TYPE) {
        Some(value) => Ok(Some(value.clone())),
        None => body_media_type(operation, &sent, true),
    }
}

/// Holds the declared values of a request to `operation`, each header to
/// `strictness`.
fn holding(
    operation: &RouteMatch,
    (query, headers, body): (Option<&str>, &HeaderMap, &[u8]),
    strictness: Strictness,
) -> Result<HeaderMap, Refusal> {
    path::held(operation)?;
    if let Some(query) = query {
        query::values(operation, query)?;
    }
    let mut sent = composed(operation, headers, strictness)?;
    if let Some(value) = body_media_type(operation, headers, !body.is_empty())? {
        sent.insert(CONTENT_TYPE, value);
    }
    Ok(sent)
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
    /// An openEHR `HIER_OBJECT_ID`.
    HierObjectId,
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
            Self::HierObjectId => f.write_str("an openEHR HIER_OBJECT_ID"),
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

    use super::{Carrier, Expected, MalformedValue, Refusal, content_type, held};
    use http::{HeaderMap, Method};
    use openehr_its::rest::routes::{Lookup, ParamKind, RouteMatch, lookup};

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

    /// The value of `name` that `held` sends for `lines` under `operation`.
    fn sent(operation: &RouteMatch, lines: &[(&'static str, &str)], name: &str) -> Option<String> {
        held(operation, None, &headers(lines), &[])
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
                held(&directory(), Some(&query), &HeaderMap::new(), &[]).is_ok(),
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
            let refused = held(&directory(), Some(&query), &HeaderMap::new(), &[]);
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
    fn a_free_text_parameter_passes_unclassified() {
        let tags = operation(&Method::GET, &format!("{EHR}/tags"));
        let query = "tag_key=4711&tag_value=O%27Sentinel-4711&tag_target_path=%2Fcontent";
        assert!(held(&tags, Some(query), &HeaderMap::new(), &[]).is_ok());
        let query = "path=O%27Sentinel-4711";
        assert!(held(&directory(), Some(query), &HeaderMap::new(), &[]).is_ok());
        let update = operation(
            &Method::PUT,
            &format!("{EHR}/composition/8849182c-82ad-4088-a07f-48ead4180515"),
        );
        let lines = [("if-match", "4711"), ("openehr-audit-details", "a=1, b=2")];
        assert_eq!(Some("4711".to_owned()), sent(&update, &lines, "if-match"));
    }

    #[test]
    fn the_content_type_of_a_query_post_is_held_to_its_listed_media_type() {
        for path in ["/query/aql", "/query/org::q", "/query/org::q/1.0.0"] {
            let query = operation(&Method::POST, path);
            let named = |value: &str| content_type(&query, &headers(&[("content-type", value)]));
            for value in ["application/json", "Application/JSON; charset=UTF-8"] {
                assert_eq!(
                    Ok(Some("application/json".to_owned())),
                    named(value).map(|sent| sent.and_then(|v| v.to_str().ok().map(str::to_owned))),
                    "{path} {value}"
                );
            }
            for value in ["text/plain", "application/xml", "application/json; q=1"] {
                assert!(
                    matches!(named(value), Err(Refusal::UnsupportedMediaType { .. })),
                    "{path} {value}"
                );
            }
            let other = headers(&[("accept", "text/html"), ("prefer", "4711")]);
            assert_eq!(Ok(None), content_type(&query, &other), "{path}");
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
}
