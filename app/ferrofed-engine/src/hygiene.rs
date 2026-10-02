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
//! which parts of it travel at all: the client headers named in
//! [`FORWARDED_HEADERS`] and nothing else ([`forwarded_headers`]), and a query
//! string only when every parameter in it is one of
//! [`FORWARDED_QUERY_PARAMETERS`] ([`forwarded_query`]). The gateway cannot
//! tell an identifying value from any other by looking at it, so an unnamed
//! header is stripped and an unnamed query parameter is refused (§5.4.1, N33).

use std::fmt;

use http::{HeaderMap, HeaderName};
use openehr_query::printer::escape_string;
use secrecy::{ExposeSecret, SecretString};
use url::Url;

/// The client request headers a single-node route forwards to the node: the
/// request headers ITS-REST 1.1.0 defines for the EHR API.
///
/// `Accept`, `Content-Type`, `If-Match` and `Prefer` are the operations'
/// declared parameters; `openehr-version`, `openehr-audit-details`,
/// `openehr-template-id`, `openehr-item-tag` and `openehr-version-item-tag`
/// are the commit headers the ITS-REST overview defines (Requests and
/// responses). `Authorization` is never among them: the gateway authenticates
/// onward with the endpoint's own credentials (§13). The federation's own
/// request headers are consumed at the gateway and never forwarded.
pub const FORWARDED_HEADERS: [&str; 9] = [
    "accept",
    "content-type",
    "if-match",
    "prefer",
    "openehr-version",
    "openehr-audit-details",
    "openehr-template-id",
    "openehr-item-tag",
    "openehr-version-item-tag",
];

/// The query parameters a single-node route forwards: those ITS-REST 1.1.0
/// defines for the operations under `/ehr/{ehr_id}`.
///
/// `version_at_time` (`EHR_STATUS`, `COMPOSITION`, `DIRECTORY` and their
/// versioned forms), `path` (`DIRECTORY`), and `tag_key`, `tag_value` and
/// `tag_target_path` (the EHR's item tags). `subject_id` and
/// `subject_namespace` carry the patient identifier and are never among them.
pub const FORWARDED_QUERY_PARAMETERS: [&str; 5] = [
    "version_at_time",
    "path",
    "tag_key",
    "tag_value",
    "tag_target_path",
];

/// The client headers of `client` a single-node route forwards: every field
/// line of each name in [`FORWARDED_HEADERS`], values byte for byte.
#[must_use]
pub fn forwarded_headers(client: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for name in FORWARDED_HEADERS {
        let name = HeaderName::from_static(name);
        for value in client.get_all(&name) {
            forwarded.append(name.clone(), value.clone());
        }
    }
    forwarded
}

/// `query` when every parameter in it is one of
/// [`FORWARDED_QUERY_PARAMETERS`], to be forwarded as received.
///
/// A parameter's name is compared percent-decoded, and an empty pair
/// (`a=1&&b=2`) carries nothing and is ignored.
///
/// # Errors
///
/// Returns [`UnlistedParameter`] naming the first other parameter by its
/// position, never by its name or value, either of which may be the
/// identifier.
pub fn forwarded_query(query: &str) -> Result<&str, UnlistedParameter> {
    let unlisted = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .position(|pair| {
            let name = pair.split('=').next().unwrap_or(pair);
            let name = percent_decoded(name);
            !FORWARDED_QUERY_PARAMETERS.contains(&name.as_str())
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
    "query parameter {position} is not one ITS-REST defines under /ehr/{{ehr_id}}, so the request was not sent"
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

    #[test]
    fn only_the_named_client_headers_are_forwarded_byte_for_byte() {
        let mut client = http::HeaderMap::new();
        client.insert("authorization", "Bearer client-token".parse().unwrap());
        client.insert("x-patient", SENTINEL.parse().unwrap());
        client.insert("if-match", "\"uid::cdr-a.example.org::1\"".parse().unwrap());
        client.append("openehr-audit-details", "change_type=249".parse().unwrap());
        client.append("openehr-audit-details", "committer=c-1".parse().unwrap());
        client.insert("openehr-federation-endpoint", "node-a-pub".parse().unwrap());
        let forwarded = super::forwarded_headers(&client);
        assert_eq!(3, forwarded.len(), "{forwarded:?}");
        assert_eq!(
            Some("\"uid::cdr-a.example.org::1\""),
            forwarded.get("if-match").and_then(|v| v.to_str().ok())
        );
        assert_eq!(2, forwarded.get_all("openehr-audit-details").iter().count());
        assert!(forwarded.get("authorization").is_none());
        assert!(forwarded.get("x-patient").is_none());
        assert!(forwarded.get("openehr-federation-endpoint").is_none());
    }

    #[test]
    fn a_query_of_named_parameters_is_forwarded_as_received() {
        let query = "version_at_time=2026-01-01T00:00:00Z&path=a%2Fb&&tag_key=k";
        assert_eq!(Ok(query), super::forwarded_query(query));
        assert_eq!(Ok(""), super::forwarded_query(""));
        assert_eq!(
            Ok("version%5Fat%5Ftime=x"),
            super::forwarded_query("version%5Fat%5Ftime=x")
        );
    }

    #[test]
    fn an_unnamed_query_parameter_is_refused_by_position_never_by_name() {
        let refused = super::forwarded_query("path=a&patient=O%27Sentinel-4711");
        assert_eq!(Err(super::UnlistedParameter { position: 2 }), refused);
        let shown = refused.map_err(|e| e.to_string()).unwrap_err();
        assert!(
            !shown.contains("patient") && !shown.contains("Sentinel"),
            "{shown}"
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query("subject_id=4711&subject_namespace=x")
        );
        assert_eq!(
            Err(super::UnlistedParameter { position: 1 }),
            super::forwarded_query("O%27Sentinel-4711")
        );
    }

    #[test]
    fn percent_decoding_keeps_a_broken_escape() {
        assert_eq!("a'b%zz%4", percent_decoded("a%27b%zz%4"));
    }
}
