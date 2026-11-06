// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The identifier-leakage oracle of track 10, read over the [`proxy`]
//! journal.
//!
//! "Node-side capture is inspected for any occurrence of the value in the
//! dispatched AQL, path, query string or headers. Zero occurrences is a
//! pass" (Federation Tier with AQL §16.3, track 10; N33, CP-26). [`Needles`]
//! holds what a search looks for: the identifier, its issuing namespace, and
//! every fragment of the identifier that neither a UUID nor a port can hold,
//! so a partial copy is found and an `ehr_id`, a minted request id or an
//! operating-system port never matches by chance. A search reads every
//! carrier of a [`Capture`] raw and percent-decoded, ASCII case ignored, and
//! each dot-separated segment of a token in a header value base64url-decoded,
//! so a claim a node would decode is read too; [`Needles::in_text`] reads any
//! other captured text, the gateway's own log among it.
//!
//! [`Needles::exact`] holds a fixture's withheld values as they are, each at
//! least [`WITHHELD_MIN`] characters long, and [`Needles::assert_absent`] is
//! the one check every N33 scan of what a node received makes, directly or
//! through [`assert_withheld`]: a failure names the request, the carrier and
//! the position of each sighting.
//!
//! No specification governs the search itself; it is FerroFED's own design.
//!
//! [`proxy`]: crate::proxy

use crate::proxy::Capture;
use crate::seed::PatientId;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::fmt;

/// The length of a fragment window over the identifier.
///
/// Long enough that no header name or AQL keyword the gateway sends holds
/// one by chance (`x-request-id` holds the four characters `est-`).
pub const FRAGMENT: usize = 6;

/// The fewest characters a value [`Needles::exact`] takes may have.
///
/// A token or a signature in a header is random base64url, and a needle of
/// four characters turns up in one by chance; at twelve, with ASCII case
/// ignored, the odds are below 1 in 10^18 per position.
pub const WITHHELD_MIN: usize = 12;

/// How many rounds of percent-decoding a search applies, so a value encoded
/// twice is still found.
const DECODE_ROUNDS: usize = 3;

/// The needles a leakage search looks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needles {
    /// The identifier itself, then its namespace, then its fragments.
    needles: Vec<String>,
}

/// The identifier cannot be searched for without matching by chance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the identifier is spelled in the alphabet of a UUID or a port, so a search would match an ehr_id or a port by chance"
)]
pub struct Collides;

/// A withheld value is too short to be searched for without matching a
/// token by chance.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "the withheld value {0:?} has fewer than {min} characters, so a search could match a token or a signature by chance", min = WITHHELD_MIN
)]
pub struct TooShort(pub String);

/// Whether `text` holds a character neither a UUID nor a port can hold: one
/// outside the hexadecimal digits and the hyphen.
fn outside_uuid_alphabet(text: &str) -> bool {
    text.chars().any(|c| !c.is_ascii_hexdigit() && c != '-')
}

impl Needles {
    /// Returns the needles of `patient`: its value, its namespace, and every
    /// [`FRAGMENT`]-character window of the value that holds a character
    /// outside the UUID alphabet.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_testkit::leak::Needles;
    /// use ferrofed_testkit::seed::PatientId;
    ///
    /// let needles = Needles::of(PatientId::new(1, 10))?;
    /// assert_eq!(needles.identifier(), "ffd-test-0010");
    /// assert!(needles.as_slice().iter().any(|needle| needle == "t-0010"));
    /// assert!(!needles.as_slice().iter().any(|needle| needle == "0010"));
    /// # Ok::<(), ferrofed_testkit::leak::Collides>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`Collides`] when the value itself is spelled in the UUID
    /// alphabet, so no search for it could tell a leak from an `ehr_id`.
    pub fn of(patient: PatientId) -> Result<Self, Collides> {
        Self::new(&patient.value(), &patient.namespace())
    }

    /// Returns the needles of the identifier `value` in `namespace`, built
    /// as [`Needles::of`] builds them.
    ///
    /// # Errors
    ///
    /// Returns [`Collides`] when `value` is spelled in the UUID alphabet.
    pub fn new(value: &str, namespace: &str) -> Result<Self, Collides> {
        if !outside_uuid_alphabet(value) {
            return Err(Collides);
        }
        let mut needles = vec![value.to_owned()];
        if !namespace.is_empty() {
            needles.push(namespace.to_owned());
        }
        let chars: Vec<char> = value.chars().collect();
        for window in chars.windows(FRAGMENT) {
            let fragment: String = window.iter().collect();
            if outside_uuid_alphabet(&fragment) && !needles.contains(&fragment) {
                needles.push(fragment);
            }
        }
        Ok(Self { needles })
    }

    /// Returns needles that are exactly `values`, in order, with no namespace
    /// and no fragments.
    ///
    /// # Examples
    ///
    /// ```
    /// use ferrofed_testkit::leak::Needles;
    ///
    /// let needles = Needles::exact(["SENTINEL-PATIENT-WITHHELD-7q"])?;
    /// assert_eq!(needles.identifier(), "SENTINEL-PATIENT-WITHHELD-7q");
    /// assert!(Needles::exact(["38kq"]).is_err());
    /// # Ok::<(), ferrofed_testkit::leak::TooShort>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`TooShort`] for the first value with fewer than
    /// [`WITHHELD_MIN`] characters.
    pub fn exact<'a>(values: impl IntoIterator<Item = &'a str>) -> Result<Self, TooShort> {
        let mut needles = Vec::new();
        for value in values {
            if value.chars().count() < WITHHELD_MIN {
                return Err(TooShort(value.to_owned()));
            }
            if !needles.iter().any(|needle| needle == value) {
                needles.push(value.to_owned());
            }
        }
        Ok(Self { needles })
    }

    /// Returns the identifier itself.
    #[must_use]
    pub fn identifier(&self) -> &str {
        self.needles.first().map_or("", String::as_str)
    }

    /// Returns every needle: the identifier, its namespace, its fragments.
    #[must_use]
    pub fn as_slice(&self) -> &[String] {
        &self.needles
    }

    /// Returns the needles `text` holds, raw or percent-decoded, ASCII case
    /// ignored.
    #[must_use]
    pub fn in_text(&self, text: &str) -> Vec<&str> {
        let forms = decoded_forms(text.as_bytes());
        self.needles
            .iter()
            .filter(|needle| forms.iter().any(|form| holds(form, needle.as_bytes())))
            .map(String::as_str)
            .collect()
    }

    /// Returns every sighting of a needle in `journal`, in every carrier of
    /// every request that `scope` reaches.
    #[must_use]
    pub fn in_journal(&self, journal: &[Capture], scope: Scope) -> Vec<Sighting> {
        let mut sightings = Vec::new();
        for (index, capture) in journal.iter().enumerate() {
            for (carrier, bytes) in carriers(capture, scope) {
                let forms = decoded_forms(&bytes);
                for needle in &self.needles {
                    if let Some(position) = forms
                        .iter()
                        .find_map(|form| position(form, needle.as_bytes()))
                    {
                        sightings.push(Sighting {
                            request: index,
                            method: capture.method.clone(),
                            path: capture.path.clone(),
                            carrier: carrier.clone(),
                            needle: needle.clone(),
                            position,
                        });
                    }
                }
            }
        }
        sightings
    }
}

impl Needles {
    /// Asserts that no needle reaches any carrier of any request in
    /// `journal` that `scope` reaches (§5.4.1, N33).
    ///
    /// Every N33 scan of what a node received runs through here, so a
    /// failure always lists each sighting with the request's position,
    /// method and path, the carrier, and the byte position.
    ///
    /// # Panics
    ///
    /// Panics, prefixed with `context`, when a needle is seen.
    pub fn assert_absent(&self, journal: &[Capture], scope: Scope, context: &str) {
        let sightings = self.in_journal(journal, scope);
        assert!(
            sightings.is_empty(),
            "{context}: a withheld value reached the node (§5.4.1, N33):\n  {}",
            sightings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }
}

/// Every carrier of `capture` that `scope` reaches, with its bytes: the
/// path, the query string, each header name and value, each token segment
/// of a header value decoded, and under [`Scope::WholeRequest`] the body.
fn carriers(capture: &Capture, scope: Scope) -> Vec<(Carrier, Vec<u8>)> {
    let mut carriers = vec![(Carrier::Path, capture.path.as_bytes().to_vec())];
    if let Some(query) = capture.query.as_deref() {
        carriers.push((Carrier::Query, query.as_bytes().to_vec()));
    }
    for (name, value) in &capture.headers {
        carriers.push((Carrier::HeaderName, name.as_bytes().to_vec()));
        carriers.push((Carrier::Header(name.clone()), value.clone()));
        for decoded in token_segments(value) {
            carriers.push((Carrier::Token(name.clone()), decoded));
        }
    }
    if scope == Scope::WholeRequest {
        carriers.push((Carrier::Body, capture.body.clone()));
    }
    carriers
}

/// Every segment of a compact token in `value` that decodes as base64url: a
/// JWS or a JWT in an `Authorization`, `DPoP` or conveyance header.
fn token_segments(value: &[u8]) -> Vec<Vec<u8>> {
    let text = String::from_utf8_lossy(value);
    text.split(|c: char| c.is_whitespace() || c == ',')
        .filter(|word| word.contains('.'))
        .flat_map(|word| word.split('.'))
        .filter(|segment| !segment.is_empty())
        .filter_map(|segment| URL_SAFE_NO_PAD.decode(segment.trim_end_matches('=')).ok())
        .collect()
}

/// Asserts that no value of `withheld` reaches any carrier of any request in
/// `journal` that `scope` reaches (§5.4.1, N33), through
/// [`Needles::assert_absent`] over [`Needles::exact`].
///
/// # Panics
///
/// Panics, prefixed with `context`, when a value is shorter than
/// [`WITHHELD_MIN`] or a value is seen.
#[expect(
    clippy::panic,
    reason = "an assertion helper fails the test that calls it, naming what it saw"
)]
pub fn assert_withheld(journal: &[Capture], withheld: &[&str], scope: Scope, context: &str) {
    match Needles::exact(withheld.iter().copied()) {
        Ok(needles) => needles.assert_absent(journal, scope, context),
        Err(short) => panic!("{context}: {short}"),
    }
}

/// The carriers of a captured request that [`Needles::in_journal`] searches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Every carrier, the body included.
    WholeRequest,
    /// A path, a query string, a header name or a header value, never the
    /// body.
    ///
    /// This is the search over a forwarded write, whose body is client
    /// content the gateway passes through unmodified (§5.4 scope note, N33).
    OutsideBodies,
}

/// The part of a captured request a needle was seen in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Carrier {
    /// The request path.
    Path,
    /// The query string.
    Query,
    /// A header name.
    HeaderName,
    /// The value of the header with this name.
    Header(String),
    /// A token segment in the value of the header with this name, decoded
    /// from base64url.
    Token(String),
    /// The request body.
    Body,
}

impl fmt::Display for Carrier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path => f.write_str("the path"),
            Self::Query => f.write_str("the query string"),
            Self::HeaderName => f.write_str("a header name"),
            Self::Header(name) => write!(f, "the {name} header"),
            Self::Token(name) => write!(f, "a decoded token segment of the {name} header"),
            Self::Body => f.write_str("the body"),
        }
    }
}

/// One needle seen in one carrier of one captured request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sighting {
    /// The request's position in the journal, counted from 0.
    pub request: usize,
    /// The request method.
    pub method: String,
    /// The request path.
    pub path: String,
    /// Where in the request the needle was.
    pub carrier: Carrier,
    /// The needle.
    pub needle: String,
    /// The byte position of the needle in the carrier, after the rounds of
    /// percent-decoding it took to find it.
    pub position: usize,
}

impl fmt::Display for Sighting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} at byte {} of {} of request {} ({} {})",
            self.needle, self.position, self.carrier, self.request, self.method, self.path
        )
    }
}

/// Returns `bytes` raw and after each round of percent-decoding that changed
/// them.
fn decoded_forms(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut forms = vec![bytes.to_vec()];
    for _ in 0..DECODE_ROUNDS {
        let Some(last) = forms.last() else { break };
        let next = percent_decoded(last);
        if &next == last {
            break;
        }
        forms.push(next);
    }
    forms
}

/// Returns `bytes` with every `%XX` escape replaced by its byte and every
/// `+` by a space, the form-encoding of a query string; a malformed escape is
/// kept as it is.
fn percent_decoded(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while let Some((&byte, tail)) = rest.split_first() {
        match (byte, tail) {
            (b'%', [high, low, after @ ..]) => {
                if let (Some(high), Some(low)) = (hex(*high), hex(*low)) {
                    out.push((high << 4) | low);
                    rest = after;
                    continue;
                }
                out.push(byte);
            }
            (b'+', _) => out.push(b' '),
            _ => out.push(byte),
        }
        rest = tail;
    }
    out
}

/// The value of the hexadecimal digit `digit`.
fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

/// Whether `haystack` holds `needle`, ASCII case ignored.
fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    position(haystack, needle).is_some()
}

/// The first byte position of `needle` in `haystack`, ASCII case ignored.
fn position(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::{Collides, FRAGMENT, Needles, outside_uuid_alphabet, percent_decoded};

    #[test]
    fn no_fragment_is_spelled_in_the_uuid_alphabet_and_each_has_the_window_length() {
        let needles = Needles::new("ffd-test-0010", "urn:oid:2.999.1.1").unwrap();
        for fragment in needles.as_slice().iter().skip(2) {
            assert_eq!(FRAGMENT, fragment.chars().count(), "{fragment}");
            assert!(outside_uuid_alphabet(fragment), "{fragment}");
        }
        assert!(needles.as_slice().contains(&"d-test".to_owned()));
        assert!(!needles.as_slice().contains(&"-0010".to_owned()));
    }

    #[test]
    fn an_identifier_spelled_like_a_uuid_is_refused() {
        assert_eq!(Err(Collides), Needles::new("4199", "urn:oid:2.999.1"));
        assert_eq!(
            Err(Collides),
            Needles::new("2222aaaa-2222-4222-8222-222222222222", "")
        );
    }

    #[test]
    fn percent_escapes_and_plus_signs_decode_and_a_malformed_escape_stays() {
        assert_eq!(b"ffd-test".to_vec(), percent_decoded(b"%66fd%2Dtest"));
        assert_eq!(b"a b%zz%4".to_vec(), percent_decoded(b"a+b%zz%4"));
    }

    #[test]
    fn a_needle_is_found_twice_encoded_and_in_another_case() {
        let needles = Needles::new("ffd-test-0010", "").unwrap();
        let found = needles.in_text("x=%2566fd-test-0010");
        assert_eq!(Some(&"ffd-test-0010"), found.first());
        assert_ne!(needles.in_text("FFD-TEST-0010"), Vec::<&str>::new());
        assert_eq!(
            needles.in_text("7f4c-0010 and port 40010"),
            Vec::<&str>::new()
        );
    }
}
