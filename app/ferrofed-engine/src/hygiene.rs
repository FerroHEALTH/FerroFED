// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The outbound gate: the last check on every request the gateway composes
//! for a node (§5.4.1, N33).
//!
//! The rewrite already refuses a query whose patient identifier would survive
//! into a node query. The gate is the second layer, independent of how the
//! request was built: right before a request leaves, it re-reads the AQL text
//! and paging of the body, the URL, and the headers the gateway adds, against
//! the identifiers resolution consumed, and refuses to send a request that
//! still carries one in any form a node could read it in. A refusal names
//! the part of the request, never the value, and nothing is sent. A write
//! body is never inspected or altered (§5.4 scope note); this gate covers the
//! query requests the gateway composes.
//!
//! The dispatcher passes every header it adds except the minted
//! `X-Request-Id`: that value is an
//! [`OutboundId`](crate::outbound_id::OutboundId), made from no client input,
//! so it cannot carry an identifier, and a short all-hex identifier can occur
//! inside it by chance, which would refuse a valid request.

use std::fmt;

use openehr_query::printer::escape_string;
use secrecy::{ExposeSecret, SecretString};

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
            } else if request.url.contains(value) || percent_decoded(request.url).contains(value) {
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
    /// The URL the request is sent to.
    pub url: &'a str,
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
    /// The URL, raw or percent-decoded.
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
    use super::{Outbound, Part, Withheld, percent_decoded};
    use secrecy::SecretString;

    const SENTINEL: &str = "O'Sentinel-4711";

    fn withheld() -> Withheld {
        Withheld::new([SecretString::from(SENTINEL)])
    }

    fn request<'a>(
        aql: &'a str,
        url: &'a str,
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
            "https://cdr.example.org/v1/query/aql",
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
            withheld().found_in(&request(aql, "https://cdr.example.org/v1/query/aql", &[]))
        );
    }

    #[test]
    fn the_identifier_percent_encoded_in_the_url_is_found() {
        let url = "https://cdr.example.org/v1/query/aql?x=O%27Sentinel-4711";
        assert_eq!(
            Some(Part::Url),
            withheld().found_in(&request("SELECT 1", url, &[]))
        );
    }

    #[test]
    fn the_identifier_in_a_header_names_the_header() {
        let headers = [("X-Request-Id", "req-O'Sentinel-4711")];
        assert_eq!(
            Some(Part::Header("X-Request-Id")),
            withheld().found_in(&request("SELECT 1", "https://cdr.example.org/", &headers))
        );
    }

    #[test]
    fn no_identifier_withheld_finds_nothing() {
        let aql =
            "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value='O\\'Sentinel-4711'";
        assert_eq!(
            None,
            Withheld::none().found_in(&request(aql, "https://cdr.example.org/", &[]))
        );
    }

    const EHR_ID: &str = "7d44b88c-4199-4bad-97dc-d78268e01398";

    fn scoped(aql: &str) -> Outbound<'_> {
        Outbound {
            scope: Some(EHR_ID),
            ..request(aql, "https://cdr.example.org/v1/query/aql", &[])
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
            short().found_in(&request(&aql, "https://cdr.example.org/", &[]))
        );
    }

    #[test]
    fn an_identifier_equal_to_the_scope_ehr_id_fails_closed() {
        let aql =
            format!("SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE e/ehr_id/value = '{EHR_ID}'");
        let equal = Withheld::new([SecretString::from(EHR_ID)]);
        assert_eq!(Some(Part::Aql), equal.found_in(&scoped(&aql)));
    }

    #[test]
    fn debug_counts_and_never_shows_a_value() {
        let shown = format!("{:?}", withheld());
        assert!(!shown.contains("Sentinel"), "{shown}");
        assert!(shown.contains('1'), "{shown}");
    }

    #[test]
    fn percent_decoding_keeps_a_broken_escape() {
        assert_eq!("a'b%zz%4", percent_decoded("a%27b%zz%4"));
    }
}
