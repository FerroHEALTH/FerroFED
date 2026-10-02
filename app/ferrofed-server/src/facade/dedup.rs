// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dedup mode a request selects with `openEHR-federation-dedup` (§10,
//! N15).
//!
//! A request without the header gets the default, `none` (§10.1, N15). The
//! header carries the name of a mode `OPTIONS {base}/` lists in
//! `dedup.modes` (§7a.2): `none`, accepted explicitly, or
//! `version-identity` (§10.2). Any other value, or more than one, is a `400`,
//! never served as `none` in its place, as §11.4 rules for a completeness
//! value a gateway does not offer. A refusal names the header and never
//! quotes its value.

use http::HeaderMap;
use openehr_federation::dedup::DedupMode;
use openehr_federation::headers::DEDUP;

/// Why a request's dedup header is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DedupError {
    /// The header is given more than once.
    #[error("the openEHR-federation-dedup header is given more than once")]
    Repeated,
    /// The header names no mode the gateway offers.
    #[error(
        "the openEHR-federation-dedup header takes \"none\" or \"version-identity\" (§10, §7a.2)"
    )]
    Value,
}

/// The dedup mode `headers` select.
///
/// # Errors
/// Returns a [`DedupError`] for a repeated header and for a value that names
/// no offered mode.
pub fn of(headers: &HeaderMap) -> Result<DedupMode, DedupError> {
    let mut values = headers.get_all(DEDUP).iter();
    let Some(value) = values.next() else {
        return Ok(DedupMode::None);
    };
    if values.next().is_some() {
        return Err(DedupError::Repeated);
    }
    // NOTE: §7a.2, a mode is a name OPTIONS lists, so a value that is not
    // visible ASCII names none of them and is refused like any other.
    let name = value.to_str().map_err(|_opaque| DedupError::Value)?;
    DedupMode::named(name).ok_or(DedupError::Value)
}

#[cfg(test)]
mod tests {
    use super::{DedupError, of};
    use http::{HeaderMap, HeaderValue};
    use openehr_federation::dedup::DedupMode;
    use openehr_federation::headers::DEDUP;

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(DEDUP, HeaderValue::from_static(value));
        }
        map
    }

    // conformance: CP-9
    #[test]
    fn no_header_is_the_pass_through_default() {
        assert_eq!(of(&headers(&[])), Ok(DedupMode::None), "§10.1, N15");
    }

    // conformance: CP-9
    #[test]
    fn each_offered_mode_is_selected_by_its_name() {
        assert_eq!(of(&headers(&["none"])), Ok(DedupMode::None));
        assert_eq!(
            of(&headers(&["version-identity"])),
            Ok(DedupMode::VersionIdentity)
        );
    }

    // conformance: CP-9
    #[test]
    fn any_other_value_is_refused() {
        for value in [
            "",
            "None",
            "VERSION-IDENTITY",
            "content-hash",
            "version-identity, none",
            "object-id",
        ] {
            assert_eq!(of(&headers(&[value])), Err(DedupError::Value), "{value:?}");
        }
        let mut opaque = HeaderMap::new();
        opaque.append(
            DEDUP,
            HeaderValue::from_bytes(b"version-identit\xff").expect("an opaque header value"),
        );
        assert_eq!(of(&opaque), Err(DedupError::Value));
    }

    // conformance: CP-9
    #[test]
    fn a_repeated_header_is_refused() {
        assert_eq!(of(&headers(&["none", "none"])), Err(DedupError::Repeated));
    }

    #[test]
    fn a_refusal_never_quotes_the_value() {
        let message = of(&headers(&["synthetic-12345"]))
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(
            !message.is_empty() && !message.contains("synthetic-12345"),
            "{message}"
        );
    }
}
