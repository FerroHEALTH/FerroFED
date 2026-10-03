// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `Debug` rendering of a URL that may carry a credential.
//!
//! A URL may carry a user name and password in its userinfo (RFC 3986
//! §3.2.1) and a credential in its query, so every type of this crate that
//! holds one shows it through [`RedactedUrl`] with both replaced by [`REDACTED`]. No
//! specification governs this: our own design.

use std::fmt;

/// The fixed text a rendering shows in place of a credential.
pub(crate) const REDACTED: &str = "***";

/// A URL whose `Debug` shows it with its userinfo and its query redacted.
pub(crate) struct RedactedUrl<'a>(pub(crate) &'a str);

impl fmt::Debug for RedactedUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&redact(self.0), f)
    }
}

/// Returns `url` with its userinfo and its query replaced by [`REDACTED`].
///
/// The authority runs from after `://` to the first `/`, `?` or `#`, and the
/// userinfo is everything in it before its last `@`, since a host holds no
/// `@` (RFC 3986 §3.2). The userinfo becomes `***@` and the query `?***`; the
/// scheme, the host, the port, the path and the fragment show as written.
/// Text with no `://` has no authority to find a credential in, so it shows as
/// [`REDACTED`] whole. The text is never decoded, so it is redacted as
/// written.
fn redact(url: &str) -> String {
    if url.is_empty() {
        return String::new();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return REDACTED.to_owned();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at_checked(end).unwrap_or((rest, ""));
    let host = match authority.rfind('@') {
        Some(at) => format!("{REDACTED}@{}", authority.get(at + 1..).unwrap_or_default()),
        None => authority.to_owned(),
    };
    let tail = match tail.split_once('?') {
        Some((path, query)) => {
            let fragment = query
                .find('#')
                .and_then(|at| query.get(at..))
                .unwrap_or_default();
            format!("{path}?{REDACTED}{fragment}")
        }
        None => tail.to_owned(),
    };
    format!("{scheme}://{host}{tail}")
}

#[cfg(test)]
mod tests {
    use super::{REDACTED, RedactedUrl, redact};

    #[test]
    fn every_part_that_may_carry_a_credential_is_redacted() {
        for (raw, redacted) in [
            (
                "https://Qz7user@pix.example.org/fhir",
                "https://***@pix.example.org/fhir",
            ),
            (
                "https://Qz7user:Qz7password@pix.example.org:8443/fhir/Patient/$ihe-pix",
                "https://***@pix.example.org:8443/fhir/Patient/$ihe-pix",
            ),
            (
                "https://u:p@ss@pix.example.org/",
                "https://***@pix.example.org/",
            ),
            (
                "https://u:p@pix.example.org?x=1",
                "https://***@pix.example.org?***",
            ),
            (
                "https://pdq.example.org/fhir/Patient?_page=Qz7token#f",
                "https://pdq.example.org/fhir/Patient?***#f",
            ),
            ("urn:uuid:1e7f2a52-0d1c-4b0f-9a52-5d3c1c0e2f11", REDACTED),
            ("u:Qz7password@pix.example.org/x", REDACTED),
        ] {
            assert_eq!(redacted, redact(raw), "{raw}");
        }
    }

    #[test]
    fn a_url_without_a_credential_shows_as_written() {
        for raw in [
            "https://pix.example.org/fhir/Patient/$ihe-pix",
            "https://directory.example.org/fhir/Endpoint/@handle",
            "http://127.0.0.1:8080/fhir/",
            "",
        ] {
            assert_eq!(raw, redact(raw), "{raw}");
        }
    }

    #[test]
    fn debug_quotes_the_redacted_url() {
        let shown = format!(
            "{:?}",
            RedactedUrl("https://u:Qz7password@pix.example.org/")
        );
        assert_eq!("\"https://***@pix.example.org/\"", shown);
    }
}
