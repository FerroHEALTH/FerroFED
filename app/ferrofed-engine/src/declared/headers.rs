// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The headers a routed request sends: each header its ITS-REST operation
//! declares, held to its kind, with `Accept`, `Content-Type` and `Prefer`
//! composed as listed values, and the `Content-Type` a body travels with
//! (§5.4.1, N33).
//!
//! A header the route does not forward is not read here:
//! [`hygiene::forwarded_headers`] strips it first.

use http::header::{ACCEPT, CONTENT_TYPE};
use http::{HeaderMap, HeaderName, HeaderValue};
use openehr_its::rest::routes::{Param, ParamKind, RouteMatch};

use super::kind::header_fits;
use super::{Carrier, Expected, MalformedValue, Refusal, negotiate};
use crate::hygiene;

/// The `Prefer` request header (RFC 7240 §2).
const PREFER: &str = "prefer";

/// The `Content-Type` the route sends for the body of a request to
/// `operation`, or `None` when it sends none.
///
/// The listed values are the media types the operation's body is declared in
/// ([`RouteMatch::request_media`]), which `openehr-its` holds equal to the
/// values of a declared `Content-Type` parameter. A `Content-Type` the client
/// sent travels as the listed media type it names. With none sent, a body
/// travels with the first listed media type; a request with no body, or to
/// an operation that takes none, travels without one.
pub(super) fn body_media_type(
    operation: &RouteMatch,
    client: &HeaderMap,
    body: bool,
) -> Result<Option<HeaderValue>, Refusal> {
    let accepted = operation.request_media;
    if accepted.is_empty() {
        return Ok(None);
    }
    let lines: Vec<&HeaderValue> = client.get_all(CONTENT_TYPE).iter().collect();
    if !lines.is_empty() {
        return negotiate::content_type(accepted, &lines)
            .and_then(listed)
            .map(Some)
            .ok_or(Refusal::UnsupportedMediaType { accepted });
    }
    if !body {
        return Ok(None);
    }
    // NOTE: ITS-REST 1.1.0 and RFC 9110 §8.3 give an absent Content-Type no default, so no
    // specification governs this: our own design sends the first listed, as for Accept.
    Ok(accepted.first().copied().and_then(listed))
}

/// What a header that does not fit its operation comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Strictness {
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
///
/// The `Content-Type` of an operation that takes a body is left to
/// [`body_media_type`]; one an operation declares with no body, as the
/// `GET`s of a versioned object do, is held to its parameter as any other
/// listed header.
pub(super) fn composed(
    operation: &RouteMatch,
    client: &HeaderMap,
    strictness: Strictness,
) -> Result<HeaderMap, Refusal> {
    let forwarded = hygiene::forwarded_headers(operation, client);
    let mut sent = HeaderMap::new();
    for name in forwarded.keys() {
        if *name == CONTENT_TYPE && !operation.request_media.is_empty() {
            continue;
        }
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

#[cfg(test)]
mod tests {
    use crate::declared::{Refusal, fitting, held};
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
        held(operation, None, &headers(lines), &[])
            .expect("the headers are held")
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    /// The listed media type of `operation` that the `Content-Type` `held`
    /// composes for `headers` names, or `None` when it sends none.
    fn content_type(operation: &RouteMatch, headers: &HeaderMap) -> Option<&'static str> {
        let ParamKind::Enum(listed) = operation.header_param("content-type")?.kind else {
            return None;
        };
        let held = held(operation, None, headers, &[]).ok()?;
        let value = held.get("content-type")?;
        listed
            .iter()
            .copied()
            .find(|media| media.as_bytes() == value.as_bytes())
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
        let refused = held(
            &directory(),
            None,
            &headers(&[("accept", "text/html")]),
            &[],
        );
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
            let refused = held(&create(), None, &headers(&[("content-type", value)]), &[]);
            assert!(
                matches!(refused, Err(Refusal::UnsupportedMediaType { .. })),
                "{value}: {refused:?}"
            );
            let shown = refused.map_or_else(|refusal| refusal.to_string(), |_| String::new());
            assert!(!shown.contains("4711"), "{shown}");
        }
    }

    #[test]
    fn the_content_type_names_the_listed_media_type_held_composes() {
        let contribution = operation(&Method::POST, &format!("{EHR}/contribution"));
        for (value, listed) in [
            ("application/json", Some("application/json")),
            ("Application/XML", Some("application/xml")),
            (
                "application/openehr.wt.flat+json; charset=UTF-8",
                Some("application/openehr.wt.flat+json"),
            ),
            (
                "application/openehr.wt.structured+json",
                Some("application/openehr.wt.structured+json"),
            ),
            ("text/plain", None),
        ] {
            let lines = [("content-type", value)];
            assert_eq!(
                listed,
                content_type(&contribution, &headers(&lines)),
                "{value}"
            );
            if listed.is_some() {
                assert_eq!(
                    listed.map(str::to_owned),
                    sent(&contribution, &lines, "content-type"),
                    "{value}"
                );
            }
        }
        assert_eq!(None, content_type(&contribution, &HeaderMap::new()));
        assert_eq!(
            None,
            content_type(
                &directory(),
                &headers(&[("content-type", "application/json")])
            ),
            "GET directory declares no Content-Type"
        );
    }

    /// The versioned stored-query `PUT`, whose body is declared in
    /// `text/plain` and which declares no `Content-Type` parameter.
    fn version_store() -> RouteMatch {
        operation(&Method::PUT, "/definition/query/org.example::q/1.0.0")
    }

    /// The `Content-Type` `held` sends for `lines` and `body` under
    /// `operation`.
    fn body_type(
        operation: &RouteMatch,
        lines: &[(&'static str, &str)],
        body: &[u8],
    ) -> Option<String> {
        held(operation, None, &headers(lines), body)
            .expect("the headers are held")
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    }

    #[test]
    fn a_body_sent_without_a_content_type_travels_as_the_first_declared() {
        assert_eq!(
            Some("text/plain".to_owned()),
            body_type(&version_store(), &[], b"SELECT c FROM COMPOSITION c")
        );
        assert_eq!(
            Some("application/json".to_owned()),
            body_type(&create(), &[], b"{}")
        );
        let contribution = operation(&Method::POST, &format!("{EHR}/contribution"));
        assert!(
            contribution.request_media.len() > 1,
            "contribution_create declares its body in several media types"
        );
        assert_eq!(
            Some("application/json".to_owned()),
            body_type(&contribution, &[], b"{}")
        );
    }

    #[test]
    fn a_listed_content_type_travels_where_no_parameter_declares_it() {
        let lines = [("content-type", "TEXT/plain; charset=utf-8")];
        assert_eq!(
            Some("text/plain".to_owned()),
            body_type(&version_store(), &lines, b"SELECT c FROM COMPOSITION c")
        );
        let refused = held(
            &version_store(),
            None,
            &headers(&[("content-type", "application/json")]),
            b"{}",
        );
        assert!(
            matches!(
                refused,
                Err(Refusal::UnsupportedMediaType {
                    accepted: ["text/plain"]
                })
            ),
            "{refused:?}"
        );
    }

    #[test]
    fn no_body_travels_without_a_content_type() {
        assert_eq!(None, body_type(&version_store(), &[], b""));
        assert_eq!(None, body_type(&directory(), &[], b""));
        assert_eq!(
            None,
            body_type(&directory(), &[("content-type", "text/plain")], b"")
        );
    }

    #[test]
    fn a_body_without_a_content_type_to_several_declared_goes_as_the_first_listed() {
        let mut several = version_store();
        several.request_media = &["text/plain", "application/json"];
        assert_eq!(
            Some("text/plain".to_owned()),
            body_type(&several, &[], b"{}")
        );
        several.request_media = &["application/json", "text/plain"];
        assert_eq!(
            Some("application/json".to_owned()),
            body_type(&several, &[], b"{}")
        );
        let lines = [("content-type", "text/plain")];
        assert_eq!(
            Some("text/plain".to_owned()),
            body_type(&several, &lines, b"{}"),
            "a Content-Type the client sends still names the listed value"
        );
    }

    #[test]
    fn every_operation_declaring_several_body_media_types_lists_canonical_json_first() {
        use openehr_its::rest::generated::{admin, definition, demographic, ehr, query, system};
        /// One route table: `(method, path, operation_id)` per operation.
        type Routes = &'static [(&'static str, &'static str, &'static str)];
        /// One request-media table, index-aligned with its route table.
        type Media = &'static [&'static [&'static str]];
        let groups: [(&str, Routes, Media); 6] = [
            ("admin", admin::ROUTES, admin::ROUTE_REQUEST_MEDIA),
            (
                "definition",
                definition::ROUTES,
                definition::ROUTE_REQUEST_MEDIA,
            ),
            (
                "demographic",
                demographic::ROUTES,
                demographic::ROUTE_REQUEST_MEDIA,
            ),
            ("ehr", ehr::ROUTES, ehr::ROUTE_REQUEST_MEDIA),
            ("query", query::ROUTES, query::ROUTE_REQUEST_MEDIA),
            ("system", system::ROUTES, system::ROUTE_REQUEST_MEDIA),
        ];
        let mut several = 0_usize;
        for (group, routes, media) in groups {
            assert_eq!(
                routes.len(),
                media.len(),
                "{group}: one media row per route"
            );
            for ((method, template, operation_id), listed) in routes.iter().zip(media.iter()) {
                if listed.len() > 1 {
                    several += 1;
                    assert_eq!(
                        Some(&"application/json"),
                        listed.first(),
                        "{group} {operation_id} ({method} {template}) lists {listed:?}: a body sent \
                         without a Content-Type goes as the first listed, which must be the canonical JSON"
                    );
                }
            }
        }
        assert_ne!(
            0, several,
            "the route tables declare no operation with several media types"
        );
    }

    #[test]
    fn a_content_type_declared_without_a_body_is_held_to_its_parameter() {
        let versioned = operation(
            &Method::GET,
            &format!("{EHR}/versioned_composition/8849182c-82ad-4088-a07f-48ead4180515"),
        );
        assert!(versioned.request_media.is_empty(), "the GET takes no body");
        assert_eq!(
            Some("application/xml".to_owned()),
            sent(
                &versioned,
                &[("content-type", "Application/XML")],
                "content-type"
            )
        );
        let refused = held(
            &versioned,
            None,
            &headers(&[("content-type", "text/plain")]),
            &[],
        );
        assert!(
            matches!(refused, Err(Refusal::UnsupportedMediaType { .. })),
            "{refused:?}"
        );
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

    #[test]
    fn an_undeclared_or_withheld_value_is_left_to_the_gate() {
        let sent = headers(&[("x-patient", "4711"), ("authorization", "Bearer 4711")]);
        let held = held(&directory(), Some("patient=4711"), &sent, &[]).expect("held");
        assert!(
            held.get("x-patient").is_none() && held.get("authorization").is_none(),
            "{held:?}"
        );
    }

    #[test]
    fn the_probe_sends_what_fits_its_own_operation() {
        let probe = operation(&Method::GET, EHR);
        let sent = headers(&[("accept", "application/openehr.wt.flat+json")]);
        let kept = fitting(&probe, None, &sent, &[]).expect("the probe is held");
        assert_eq!(
            Some("application/json"),
            kept.get("accept").and_then(|value| value.to_str().ok())
        );
    }
}
