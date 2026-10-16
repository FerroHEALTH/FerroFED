// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Which parts of a client request a single-node route forwards at all
//! (§5.4.1, N33).
//!
//! The client headers the matched ITS-REST operation declares travel and
//! nothing else ([`forwarded_headers`]), and a query string only when the
//! operation declares every parameter in it ([`forwarded_query`]).

use http::HeaderMap;
use openehr_its::rest::routes::RouteMatch;

/// The client headers a single-node route never forwards, whatever the
/// operation declares, beside every header under
/// [`WITHHELD_HEADER_PREFIX`].
///
/// `Authorization` is the client's credential at the gateway, and the gateway
/// authenticates onward with the endpoint's own credentials (§13).
/// `X-Request-Id` is the client's free text, and a node receives the
/// gateway's minted [`OutboundId`](crate::outbound_id::OutboundId) instead.
/// Each name is compared without regard to case (RFC 9110 §5.1).
// NOTE: §5.4.1, N33: no ITS-REST operation declares these headers, and a
// client value in either could name a patient, so both stay withheld.
pub const WITHHELD_HEADERS: [&str; 2] = ["authorization", "x-request-id"];

/// The name prefix of the federation's own headers, none of which a
/// single-node route forwards, whatever the operation declares.
///
/// Every request header the specification defines carries it
/// (`openehr_federation::headers::ALL`): the targeting headers (§8.4), the
/// completion strategy (§11.4) and the dedup mode (§10). Each is consumed at
/// the gateway and means nothing at a node. A name is compared without
/// regard to case (RFC 9110 §5.1).
// NOTE: §5.4.1, N33: the gateway copies no header without a rule naming it, so
// the whole family is withheld by name, a header the gateway does not read too.
pub const WITHHELD_HEADER_PREFIX: &str = "openEHR-federation-";

/// Whether the client header `name` is one a single-node route never
/// forwards: one of [`WITHHELD_HEADERS`], or any name under
/// [`WITHHELD_HEADER_PREFIX`].
#[must_use]
pub fn is_withheld_header(name: &str) -> bool {
    let federation = name
        .get(..WITHHELD_HEADER_PREFIX.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(WITHHELD_HEADER_PREFIX));
    federation
        || WITHHELD_HEADERS
            .iter()
            .any(|withheld| name.eq_ignore_ascii_case(withheld))
}

/// The query parameters a single-node route never forwards, whatever the
/// operation declares: the patient identifier and its namespace (§5.4.2).
// NOTE: §5.4.1, N33: only `GET /ehr` declares them, which resolution answers
// at the gateway, so no forwarded request carries them.
pub const WITHHELD_QUERY_PARAMETERS: [&str; 2] = ["subject_id", "subject_namespace"];

/// The client headers of `client` a single-node route forwards for
/// `operation`, less every header [`is_withheld_header`] names.
///
/// Every field line of each header the operation declares is kept, its value
/// byte for byte. A field name is compared without regard to case (RFC 9110 §5.1). The
/// federation's own request headers are consumed at the gateway and never
/// forwarded, even were an operation to declare one.
#[must_use]
pub fn forwarded_headers(operation: &RouteMatch, client: &HeaderMap) -> HeaderMap {
    admitted_headers(|name| operation.header_param(name).is_some(), client)
}

/// The headers of `client` whose name `declared` accepts, less every header
/// [`is_withheld_header`] names, each field line kept byte for byte.
fn admitted_headers(declared: impl Fn(&str) -> bool, client: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for (name, value) in client {
        if !is_withheld_header(name.as_str()) && declared(name.as_str()) {
            forwarded.append(name.clone(), value.clone());
        }
    }
    forwarded
}

/// `query` when `operation` declares every parameter in it and none is one of
/// [`WITHHELD_QUERY_PARAMETERS`], to be forwarded as received.
///
/// A parameter's name is compared percent-decoded, and an empty pair
/// (`a=1&&b=2`) carries nothing and is ignored.
///
/// # Errors
///
/// Returns [`UnlistedParameter`] naming the first other parameter by its
/// position, never by its name or value, either of which may be the
/// identifier.
pub fn forwarded_query<'q>(
    operation: &RouteMatch,
    query: &'q str,
) -> Result<&'q str, UnlistedParameter> {
    crate::declared::query::admitted(query, |name| {
        !WITHHELD_QUERY_PARAMETERS.contains(&name) && operation.query_key(name).is_some()
    })?;
    Ok(query)
}

/// A query parameter a single-node route does not forward, so the request is
/// refused before anything is sent (§5.4.1, N33).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "query parameter {position} is not one the ITS-REST operation declares, so the request was not sent"
)]
pub struct UnlistedParameter {
    /// The parameter's position in the query string, counted from 1.
    pub position: usize,
}

#[cfg(test)]
mod tests {
    use openehr_its::rest::routes::{Lookup, RouteMatch, lookup};

    const SENTINEL: &str = "O'Sentinel-4711";

    /// The operation `method` and `path` address, from `openehr-its`'s table.
    fn operation(method: &http::Method, path: &str) -> RouteMatch {
        match lookup(method, path) {
            Lookup::Matched(matched) => matched,
            other => panic!("{method} {path} names no operation: {other:?}"),
        }
    }

    fn composition_update() -> RouteMatch {
        operation(&http::Method::PUT, "/ehr/7d44/composition/u::s::1")
    }

    #[test]
    fn only_the_declared_client_headers_are_forwarded_byte_for_byte() {
        let mut client = http::HeaderMap::new();
        client.insert("authorization", "Bearer client-token".parse().unwrap());
        client.insert("x-request-id", "req-O'Sentinel-4711".parse().unwrap());
        client.insert("x-patient", SENTINEL.parse().unwrap());
        client.insert("if-match", "\"uid::cdr-a.example.org::1\"".parse().unwrap());
        client.append("openehr-audit-details", "change_type=249".parse().unwrap());
        client.append("openehr-audit-details", "committer=c-1".parse().unwrap());
        client.insert("openehr-federation-endpoint", "node-a-pub".parse().unwrap());
        let forwarded = super::forwarded_headers(&composition_update(), &client);
        assert_eq!(3, forwarded.len(), "{forwarded:?}");
        assert_eq!(
            Some("\"uid::cdr-a.example.org::1\""),
            forwarded.get("if-match").and_then(|v| v.to_str().ok())
        );
        assert_eq!(2, forwarded.get_all("openehr-audit-details").iter().count());
        assert!(forwarded.get("authorization").is_none());
        assert!(forwarded.get("x-request-id").is_none());
        assert!(forwarded.get("x-patient").is_none());
        assert!(forwarded.get("openehr-federation-endpoint").is_none());
    }

    // conformance: CP-28
    #[test]
    fn the_targeting_headers_are_withheld_from_every_operation() {
        let mut client = http::HeaderMap::new();
        client.insert("openEHR-federation-endpoint", "node-a-pub".parse().unwrap());
        client.insert("openEHR-Federation-Organisation", "org-a".parse().unwrap());
        client.insert("accept", "application/json".parse().unwrap());
        for operation in [
            composition_update(),
            operation(&http::Method::GET, "/ehr/7d44/composition/u::s::1"),
            operation(&http::Method::POST, "/ehr/7d44/composition"),
            operation(&http::Method::GET, "/ehr/7d44/ehr_status"),
        ] {
            let forwarded = super::forwarded_headers(&operation, &client);
            assert!(
                forwarded.get("openehr-federation-endpoint").is_none()
                    && forwarded.get("openehr-federation-organisation").is_none(),
                "§8.4: targeting means nothing at a node: {forwarded:?}"
            );
        }
        for name in [
            openehr_federation::headers::ENDPOINT,
            openehr_federation::headers::ORGANISATION,
        ] {
            assert!(super::is_withheld_header(name), "{name}");
        }
    }

    // conformance: CP-26
    #[test]
    fn every_federation_header_is_withheld_even_where_an_operation_declares_it() {
        let mut client = http::HeaderMap::new();
        for name in openehr_federation::headers::ALL {
            client.insert(name, SENTINEL.parse().unwrap());
        }
        client.insert("OPENEHR-FEDERATION-PATIENT", SENTINEL.parse().unwrap());
        client.insert("openehr-federation-", SENTINEL.parse().unwrap());
        client.insert("authorization", "Bearer client-token".parse().unwrap());
        client.insert("x-request-id", SENTINEL.parse().unwrap());
        client.insert("openehr-version", "1".parse().unwrap());
        client.insert("openehr-federationless", "kept".parse().unwrap());
        let declares_everything = |_: &str| true;
        let forwarded = super::admitted_headers(declares_everything, &client);
        let mut names: Vec<&str> = forwarded.keys().map(http::HeaderName::as_str).collect();
        names.sort_unstable();
        assert_eq!(
            vec!["openehr-federationless", "openehr-version"],
            names,
            "§5.4.1, N33: a federation header never reaches a node"
        );
        for name in openehr_federation::headers::ALL {
            assert!(
                name.starts_with(super::WITHHELD_HEADER_PREFIX),
                "{name} is outside the withheld family"
            );
        }
    }

    #[test]
    fn a_name_shorter_than_the_prefix_is_never_read_as_it() {
        assert!(!super::is_withheld_header("openehr-fed"));
        assert!(!super::is_withheld_header(""));
        assert!(super::is_withheld_header("Authorization"));
        assert!(super::is_withheld_header("X-Request-ID"));
    }

    #[test]
    fn a_header_one_operation_declares_is_stripped_from_one_that_does_not() {
        let mut client = http::HeaderMap::new();
        client.insert("if-match", "\"uid::cdr-a.example.org::1\"".parse().unwrap());
        client.insert("accept", "application/json".parse().unwrap());
        let update = super::forwarded_headers(&composition_update(), &client);
        assert!(update.get("if-match").is_some(), "{update:?}");
        let read = operation(&http::Method::GET, "/ehr/7d44/composition/u::s::1");
        let read = super::forwarded_headers(&read, &client);
        assert!(read.get("if-match").is_none(), "{read:?}");
        assert_eq!(
            Some("application/json"),
            read.get("accept").and_then(|v| v.to_str().ok())
        );
    }

    #[test]
    fn a_query_of_declared_parameters_is_forwarded_as_received() {
        let directory = operation(&http::Method::GET, "/ehr/7d44/directory");
        let query = "version_at_time=2026-01-01T00:00:00Z&path=a%2Fb&";
        assert_eq!(Ok(query), super::forwarded_query(&directory, query));
        assert_eq!(Ok(""), super::forwarded_query(&directory, ""));
        assert_eq!(
            Ok("version%5Fat%5Ftime=x"),
            super::forwarded_query(&directory, "version%5Fat%5Ftime=x")
        );
        let tags = operation(&http::Method::GET, "/ehr/7d44/tags");
        let query = "tag_key=k&&tag_value=v&tag_target_path=p";
        assert_eq!(Ok(query), super::forwarded_query(&tags, query));
    }

    #[test]
    fn a_parameter_another_operation_declares_is_refused() {
        let composition = operation(&http::Method::GET, "/ehr/7d44/composition/u::s::1");
        assert_eq!(
            Ok("version_at_time=x"),
            super::forwarded_query(&composition, "version_at_time=x")
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 2 }),
            super::forwarded_query(&composition, "version_at_time=x&path=a")
        );
    }

    #[test]
    fn an_undeclared_query_parameter_is_refused_by_position_never_by_name() {
        let directory = operation(&http::Method::GET, "/ehr/7d44/directory");
        let refused = super::forwarded_query(&directory, "path=a&patient=O%27Sentinel-4711");
        assert_eq!(Err(super::UnlistedParameter { position: 2 }), refused);
        let shown = refused.map_err(|e| e.to_string()).unwrap_err();
        assert!(
            !shown.contains("patient") && !shown.contains("Sentinel"),
            "{shown}"
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query(&directory, "subject_id=4711&subject_namespace=x")
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query(&directory, "O%27Sentinel-4711")
        );
    }

    #[test]
    fn the_subject_parameters_are_refused_even_where_declared() {
        let by_subject = operation(&http::Method::GET, "/ehr");
        assert!(by_subject.query_param("subject_id").is_some());
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query(&by_subject, "subject_id=4711&subject_namespace=x")
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query(&by_subject, "subject%5Fnamespace=x")
        );
    }
}
