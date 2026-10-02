// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate: the last check on every request the gateway composes
//! for a node (§5.4.1, N33).
//!
//! The rewrite already refuses a query whose patient identifier would survive
//! into a node query. The gate is the second layer, independent of how the
//! request was built: right before a request leaves, it re-reads the AQL text
//! and paging of the body, the path and query of the URL, and the headers the
//! gateway adds, against the identifiers resolution consumed, and refuses to
//! send a request that still carries one in any form a node could read it in.
//! The authority of the URL (its host and port) is never read: it is the
//! endpoint URL of the operator's registry, built from no request, and
//! §5.4.1 names the path, the query string and the headers. A refusal names
//! the part of the request, never the value, and nothing is sent. A write
//! body is never inspected or altered (§5.4 scope note).
//!
//! The dispatcher passes every header it adds except the minted
//! `X-Request-Id`: that value is an
//! [`OutboundId`](crate::outbound_id::OutboundId), made from no client input,
//! so it cannot carry an identifier, and a short all-hex identifier can occur
//! inside it by chance, which would refuse a valid request.
//!
//! A single-node route forwards a client request, so the gate also decides
//! which parts of it travel at all, from the parameters the matched ITS-REST
//! operation declares (`openehr-its`'s `routes::lookup`): the client headers
//! the operation declares and nothing else ([`forwarded_headers`]), and a
//! query string only when the operation declares every parameter in it
//! ([`forwarded_query`]). The gateway cannot tell an identifying value from
//! any other by looking at it, so an undeclared header is stripped and an
//! undeclared query parameter is refused (§5.4.1, N33).

use std::fmt;

use http::HeaderMap;
use openehr_its::rest::routes::RouteMatch;
use openehr_query::printer::escape_string;
use secrecy::{ExposeSecret, SecretString};
use url::Url;

/// The client headers a single-node route never forwards, whatever the
/// operation declares.
///
/// `Authorization` is the client's credential at the gateway, and the gateway
/// authenticates onward with the endpoint's own credentials (§13).
/// `X-Request-Id` is the client's free text, and a node receives the
/// gateway's minted [`OutboundId`](crate::outbound_id::OutboundId) instead.
// NOTE: §5.4.1, N33: no ITS-REST operation declares either header, and a
// client value in either could name a patient, so both stay withheld.
pub const WITHHELD_HEADERS: [&str; 2] = ["authorization", "x-request-id"];

/// The query parameters a single-node route never forwards, whatever the
/// operation declares: the patient identifier and its namespace (§5.4.2).
// NOTE: §5.4.1, N33: only `GET /ehr` declares them, which resolution answers
// at the gateway, so no forwarded request carries them.
pub const WITHHELD_QUERY_PARAMETERS: [&str; 2] = ["subject_id", "subject_namespace"];

/// The client headers of `client` a single-node route forwards for
/// `operation`: every field line of each header the operation declares,
/// values byte for byte, less [`WITHHELD_HEADERS`].
///
/// A field name is compared without regard to case (RFC 9110 §5.1). The
/// federation's own request headers, which no ITS-REST operation declares,
/// are consumed at the gateway and never forwarded.
#[must_use]
pub fn forwarded_headers(operation: &RouteMatch, client: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for (name, value) in client {
        let withheld = WITHHELD_HEADERS.contains(&name.as_str());
        if !withheld && operation.header_param(name.as_str()).is_some() {
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
    let unlisted = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .position(|pair| {
            let name = pair.split('=').next().unwrap_or(pair);
            let name = percent_decoded(name);
            WITHHELD_QUERY_PARAMETERS.contains(&name.as_str())
                || operation.query_key(&name).is_none()
        });
    match unlisted {
        Some(index) => Err(UnlistedParameter {
            position: index.saturating_add(1),
        }),
        None => Ok(query),
    }
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

/// The identifiers resolution consumed for one query, which no request to a
/// node may carry (§5.4.1).
///
/// `Debug` prints how many there are and never a value.
#[derive(Clone, Default)]
pub struct Withheld(Vec<SecretString>);

impl Withheld {
    /// No identifier is withheld: a query that named no patient.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The identifiers `values`, ignoring empty ones, which no request could
    /// be checked against.
    #[must_use]
    pub fn new(values: impl IntoIterator<Item = SecretString>) -> Self {
        Self(
            values
                .into_iter()
                .filter(|value| !value.expose_secret().is_empty())
                .collect(),
        )
    }

    /// Whether no identifier is withheld.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The first part of `request` that carries a withheld identifier, in any
    /// form a node could read it in, or `None` for a clean request.
    ///
    /// The scope literal the rewrite wrote (`'<ehr_id>'`) is masked out of
    /// the AQL text before it is read, so a short identifier that happens to
    /// occur inside the node's own `ehr_id` is no false refusal. An
    /// identifier that contains the whole `ehr_id` is never masked: the
    /// cross-reference maps a patient to an `ehr_id` the node minted, so the
    /// two are equal only through a defect, and the gate then fails closed.
    #[must_use]
    pub fn found_in(&self, request: &Outbound<'_>) -> Option<Part> {
        self.0.iter().find_map(|value| {
            let value = value.expose_secret();
            let escaped = escape_string(value);
            let in_aql = |fragment: &str| fragment.contains(value) || fragment.contains(&escaped);
            let aql_carries = match request.scope.filter(|scope| !value.contains(*scope)) {
                Some(scope) => {
                    let token = format!("'{}'", escape_string(scope));
                    request.aql.split(token.as_str()).any(in_aql)
                }
                None => in_aql(request.aql),
            };
            if aql_carries {
                Some(Part::Aql)
            } else if request.paging.iter().any(|number| number.contains(value)) {
                Some(Part::Paging)
            } else if carried_in_target(request.url, value) {
                Some(Part::Url)
            } else {
                request
                    .headers
                    .iter()
                    .find(|(_, text)| text.contains(value) || percent_decoded(text).contains(value))
                    .map(|(name, _)| Part::Header(name))
            }
        })
    }
}

impl fmt::Debug for Withheld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Withheld")
            .field("identifiers", &self.0.len())
            .finish()
    }
}

/// What a request to a node carries that a node can read: the parts the gate
/// checks.
#[derive(Debug, Clone, Copy)]
pub struct Outbound<'a> {
    /// The AQL text of the body.
    pub aql: &'a str,
    /// The node's own `ehr_id` the rewrite scoped the AQL to, if any.
    pub scope: Option<&'a str>,
    /// The body's paging members, as the text they are sent as.
    pub paging: &'a [String],
    /// The URL the request is sent to, of which the gate reads the path, the
    /// query and the fragment, and never the authority.
    pub url: &'a Url,
    /// The headers the gateway adds, by name, other than the minted
    /// `X-Request-Id`.
    pub headers: &'a [(&'static str, &'a str)],
}

/// The part of a request that carried a withheld identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The AQL text of the body, raw or as an AQL string literal.
    Aql,
    /// A paging member of the body.
    Paging,
    /// The path, query or fragment of the URL, raw or percent-decoded.
    Url,
    /// A header the gateway adds.
    Header(&'static str),
}

impl fmt::Display for Part {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Aql => f.write_str("the AQL text"),
            Self::Paging => f.write_str("a paging member"),
            Self::Url => f.write_str("the URL"),
            Self::Header(name) => write!(f, "the {name} header"),
        }
    }
}

/// Whether the path, query or fragment of `url`, raw or percent-decoded,
/// carries `value`.
fn carried_in_target(url: &Url, value: &str) -> bool {
    // NOTE: §5.4.1, N33 name the request path, the query string and the headers;
    // the authority comes from the operator's registry, never a request, so it is unread.
    let mut target = url.path().to_owned();
    if let Some(query) = url.query() {
        target.push('?');
        target.push_str(query);
    }
    if let Some(fragment) = url.fragment() {
        target.push('#');
        target.push_str(fragment);
    }
    target.contains(value) || percent_decoded(&target).contains(value)
}

/// `text` with every `%XX` escape decoded, invalid UTF-8 replaced; an escape
/// that is not two hex digits is kept as written.
fn percent_decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let escape = (byte == b'%')
            .then(|| {
                let high = hex(*bytes.get(index.saturating_add(1))?)?;
                let low = hex(*bytes.get(index.saturating_add(2))?)?;
                Some((high << 4) | low)
            })
            .flatten();
        if let Some(decoded) = escape {
            out.push(decoded);
            index = index.saturating_add(3);
        } else {
            out.push(byte);
            index = index.saturating_add(1);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use super::{Outbound, Part, Withheld, percent_decoded};
    use openehr_its::rest::routes::{Lookup, RouteMatch, lookup};
    use secrecy::SecretString;
    use url::Url;

    const SENTINEL: &str = "O'Sentinel-4711";

    /// The query URL of an endpoint whose authority and path hold no
    /// withheld value.
    static QUERY_URL: LazyLock<Url> = LazyLock::new(|| url("https://cdr.example.org/v1/query/aql"));

    fn url(text: &str) -> Url {
        Url::parse(text).expect("a test URL")
    }

    fn withheld() -> Withheld {
        Withheld::new([SecretString::from(SENTINEL)])
    }

    fn request<'a>(
        aql: &'a str,
        url: &'a Url,
        headers: &'a [(&'static str, &'a str)],
    ) -> Outbound<'a> {
        Outbound {
            aql,
            scope: None,
            paging: &[],
            url,
            headers,
        }
    }

    #[test]
    fn a_clean_request_passes() {
        let clean = request(
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value='7d44'",
            &QUERY_URL,
            &[("X-Request-Id", "req-1")],
        );
        assert_eq!(None, withheld().found_in(&clean));
    }

    #[test]
    fn the_identifier_as_an_aql_literal_is_found_escaped() {
        let aql =
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value='O\\'Sentinel-4711'";
        assert_eq!(
            Some(Part::Aql),
            withheld().found_in(&request(aql, &QUERY_URL, &[]))
        );
    }

    #[test]
    fn the_identifier_percent_encoded_in_the_url_is_found() {
        let target = url("https://cdr.example.org/v1/query/aql?x=O%27Sentinel-4711");
        assert_eq!(
            Some(Part::Url),
            withheld().found_in(&request("SELECT 1", &target, &[]))
        );
    }

    #[test]
    fn the_identifier_in_a_header_names_the_header() {
        let headers = [("X-Request-Id", "req-O'Sentinel-4711")];
        assert_eq!(
            Some(Part::Header("X-Request-Id")),
            withheld().found_in(&request("SELECT 1", &QUERY_URL, &headers))
        );
    }

    #[test]
    fn no_identifier_withheld_finds_nothing() {
        let aql =
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value='O\\'Sentinel-4711'";
        assert_eq!(
            None,
            Withheld::none().found_in(&request(aql, &QUERY_URL, &[]))
        );
    }

    const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

    fn scoped(aql: &str) -> Outbound<'_> {
        Outbound {
            scope: Some(EHR_ID),
            ..request(aql, &QUERY_URL, &[])
        }
    }

    fn short() -> Withheld {
        Withheld::new([SecretString::from("4199")])
    }

    #[test]
    fn a_short_identifier_inside_the_scope_ehr_id_passes() {
        let aql =
            format!("SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'");
        assert_eq!(None, short().found_in(&scoped(&aql)));
    }

    #[test]
    fn the_same_short_identifier_elsewhere_is_found() {
        let aql = format!(
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}' AND c/name/value = '4199'"
        );
        assert_eq!(Some(Part::Aql), short().found_in(&scoped(&aql)));
    }

    #[test]
    fn without_a_scope_the_ehr_id_is_read_like_any_text() {
        let aql =
            format!("SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'");
        assert_eq!(
            Some(Part::Aql),
            short().found_in(&request(&aql, &QUERY_URL, &[]))
        );
    }

    #[test]
    fn an_identifier_equal_to_the_scope_ehr_id_fails_closed() {
        let aql =
            format!("SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'");
        let equal = Withheld::new([SecretString::from(EHR_ID)]);
        assert_eq!(Some(Part::Aql), equal.found_in(&scoped(&aql)));
    }

    // conformance: CP-26
    #[test]
    fn the_authority_of_the_url_is_never_read() {
        let target = url("https://4199@cdr-4199.example.org:4199/v1/query/aql");
        assert_eq!(
            None,
            short().found_in(&request(
                "SELECT c FROM EHR e CONTAINS COMPOSITION c",
                &target,
                &[]
            ))
        );
    }

    // conformance: CP-26
    #[test]
    fn the_path_query_and_fragment_are_read_under_any_authority() {
        for text in [
            "https://cdr.example.org/openehr-4199/v1/query/aql",
            "https://cdr.example.org/v1/query/aql?ehr=4199",
            "https://cdr.example.org/v1/query/aql?ehr=%34199",
            "https://cdr.example.org/v1/query/aql#4199",
            "https://cdr.example.org:4199/v1/query/aql?ehr=4199",
        ] {
            let target = url(text);
            assert_eq!(
                Some(Part::Url),
                short().found_in(&request("SELECT 1", &target, &[])),
                "{text}"
            );
        }
    }

    #[test]
    fn a_value_across_the_path_and_the_query_is_found() {
        let target = url("https://cdr.example.org/v1/query/aql?x=1");
        let across = Withheld::new([SecretString::from("aql?x")]);
        assert_eq!(
            Some(Part::Url),
            across.found_in(&request("SELECT 1", &target, &[]))
        );
    }

    #[test]
    fn debug_counts_and_never_shows_a_value() {
        let shown = format!("{:?}", withheld());
        assert!(!shown.contains("Sentinel"), "{shown}");
        assert!(shown.contains('1'), "{shown}");
    }

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

    #[test]
    fn percent_decoding_keeps_a_broken_escape() {
        assert_eq!("a'b%zz%4", percent_decoded("a%27b%zz%4"));
    }
}
