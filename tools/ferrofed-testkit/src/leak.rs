// SPDX-FileCopyrightText: Vernum Projecten B.V.
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
//! [`Needles::in_text`] reads any other captured text, the gateway's own log
//! among it.
//!
//! No specification governs the search itself; it is FerroFED's own design.
//!
//! [`proxy`]: crate::proxy

use crate::proxy::Capture;
use crate::seed::PatientId;
use std::fmt;

/// The length of a fragment window over the identifier.
///
/// Long enough that no header name or AQL keyword the gateway sends holds
/// one by chance (`x-request-id` holds the four characters `est-`).
pub const FRAGMENT: usize = 6;

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

    /// Returns every sighting of a needle in `journal`, in every carrier of
    /// every request, the body included.
    #[must_use]
    pub fn in_journal(&self, journal: &[Capture]) -> Vec<Sighting> {
        self.search(journal, true)
    }

    /// Returns every sighting of a needle in `journal` outside the request
    /// bodies: in a path, a query string, a header name or a header value.
    ///
    /// This is the search over a forwarded write, whose body is client
    /// content the gateway passes through unmodified (§5.4 scope note, N33).
    #[must_use]
    pub fn in_journal_outside_bodies(&self, journal: &[Capture]) -> Vec<Sighting> {
        self.search(journal, false)
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

    /// The sightings in `journal`, bodies included when `bodies` is set.
    fn search(&self, journal: &[Capture], bodies: bool) -> Vec<Sighting> {
        let mut sightings = Vec::new();
        for (index, capture) in journal.iter().enumerate() {
            let mut carriers: Vec<(Carrier, &[u8])> =
                vec![(Carrier::Path, capture.path.as_bytes())];
            if let Some(query) = capture.query.as_deref() {
                carriers.push((Carrier::Query, query.as_bytes()));
            }
            for (name, value) in &capture.headers {
                carriers.push((Carrier::HeaderName, name.as_bytes()));
                carriers.push((Carrier::Header(name.clone()), value));
            }
            if bodies {
                carriers.push((Carrier::Body, &capture.body));
            }
            for (carrier, bytes) in carriers {
                let forms = decoded_forms(bytes);
                for needle in &self.needles {
                    if forms.iter().any(|form| holds(form, needle.as_bytes())) {
                        sightings.push(Sighting {
                            request: index,
                            method: capture.method.clone(),
                            path: capture.path.clone(),
                            carrier: carrier.clone(),
                            needle: needle.clone(),
                        });
                    }
                }
            }
        }
        sightings
    }
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
}

impl fmt::Display for Sighting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} in {} of request {} ({} {})",
            self.needle, self.carrier, self.request, self.method, self.path
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
    needle.is_empty()
        || haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
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
        assert!(!needles.in_text("FFD-TEST-0010").is_empty());
        assert!(needles.in_text("7f4c-0010 and port 40010").is_empty());
    }
}
