// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The completion strategy a request selects with
//! `openEHR-federation-completeness` (§11.4, N37).
//!
//! The header carries `all`, the default, or `partial`. A request without it
//! runs all-or-nothing. `all` is accepted explicitly, so a client can state its
//! requirement without relying on the default. `partial` selects best-effort
//! where the gateway offers it and is refused with a `400` where it does not,
//! never served all-or-nothing in its place. Any other value, or more than one,
//! is a `400` too. A refusal names the header and never quotes its value.

use ferrofed_engine::fanout::Completion;
use http::HeaderMap;
use openehr_federation::headers::{COMPLETENESS, COMPLETENESS_ALL, COMPLETENESS_PARTIAL};

/// Why a request's completeness header is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CompletenessError {
    /// The header is given more than once.
    #[error("the openEHR-federation-completeness header is given more than once")]
    Repeated,
    /// The header carries neither `all` nor `partial`.
    #[error("the openEHR-federation-completeness header takes \"all\" or \"partial\"")]
    Value,
    /// The request asks for best-effort, which this gateway does not offer.
    #[error(
        "best-effort completion is not offered: openEHR-federation-completeness: partial is refused (§11.4)"
    )]
    NotOffered,
}

/// The completion strategy `headers` select, where best-effort is `offered`
/// or not.
///
/// # Errors
/// Returns a [`CompletenessError`] for a repeated header, a value other than
/// `all` or `partial`, and `partial` where best-effort is not offered.
pub fn of(headers: &HeaderMap, offered: bool) -> Result<Completion, CompletenessError> {
    let mut values = headers.get_all(COMPLETENESS).iter();
    let Some(value) = values.next() else {
        return Ok(Completion::AllOrNothing);
    };
    if values.next().is_some() {
        return Err(CompletenessError::Repeated);
    }
    match value.as_bytes() {
        bytes if bytes == COMPLETENESS_ALL.as_bytes() => Ok(Completion::AllOrNothing),
        bytes if bytes == COMPLETENESS_PARTIAL.as_bytes() => {
            if offered {
                Ok(Completion::BestEffort)
            } else {
                Err(CompletenessError::NotOffered)
            }
        }
        _ => Err(CompletenessError::Value),
    }
}

#[cfg(test)]
mod tests {
    use super::{CompletenessError, of};
    use ferrofed_engine::fanout::Completion;
    use http::{HeaderMap, HeaderValue};
    use openehr_federation::headers::COMPLETENESS;

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for value in values {
            map.append(COMPLETENESS, HeaderValue::from_static(value));
        }
        map
    }

    #[test]
    fn no_header_is_all_or_nothing() {
        assert_eq!(of(&headers(&[]), true), Ok(Completion::AllOrNothing));
        assert_eq!(of(&headers(&[]), false), Ok(Completion::AllOrNothing));
    }

    #[test]
    fn all_is_accepted_explicitly_in_either_offer() {
        assert_eq!(of(&headers(&["all"]), true), Ok(Completion::AllOrNothing));
        assert_eq!(of(&headers(&["all"]), false), Ok(Completion::AllOrNothing));
    }

    #[test]
    fn partial_selects_best_effort_only_where_offered() {
        assert_eq!(of(&headers(&["partial"]), true), Ok(Completion::BestEffort));
        assert_eq!(
            of(&headers(&["partial"]), false),
            Err(CompletenessError::NotOffered)
        );
    }

    #[test]
    fn any_other_value_is_refused() {
        for value in ["", "ALL", "Partial", "best-effort", "partial, all", "none"] {
            assert_eq!(
                of(&headers(&[value]), true),
                Err(CompletenessError::Value),
                "{value:?}"
            );
        }
    }

    #[test]
    fn a_repeated_header_is_refused() {
        assert_eq!(
            of(&headers(&["partial", "partial"]), true),
            Err(CompletenessError::Repeated)
        );
        assert_eq!(
            of(&headers(&["all", "partial"]), true),
            Err(CompletenessError::Repeated)
        );
    }

    #[test]
    fn a_refusal_never_quotes_the_value() {
        let error = of(&headers(&["synthetic-12345"]), true);
        assert_eq!(error, Err(CompletenessError::Value));
        let message = error
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(!message.contains("synthetic-12345"), "{message}");
    }
}
