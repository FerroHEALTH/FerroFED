// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The query string the generated ITS-REST parameter decoders read.
//!
//! The decoders of `openehr-its` read a query string as
//! `application/x-www-form-urlencoded`, where a `+` is a space. The gateway's
//! client contract reads every query parameter percent-decoded by RFC 3986
//! §2.1, where a `+` is itself, so a client's `+` is written `%2B` before a
//! decoder reads it.

use std::borrow::Cow;

/// Returns `query` with every `+` written as `%2B`, so a generated decoder
/// reads it as a literal plus.
// NOTE: RFC 3986 §2.1 gives `+` no meaning of its own; the client contract keeps it a plus.
pub(crate) fn literal_plus(query: Option<&str>) -> Option<Cow<'_, str>> {
    query.map(|text| {
        if text.contains('+') {
            Cow::Owned(text.replace('+', "%2B"))
        } else {
            Cow::Borrowed(text)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::literal_plus;

    #[test]
    fn a_plus_is_written_as_its_percent_encoding() {
        assert_eq!(
            literal_plus(Some("subject_id=A+B&n=%20")).as_deref(),
            Some("subject_id=A%2BB&n=%20")
        );
        assert_eq!(literal_plus(Some("a=b")).as_deref(), Some("a=b"));
        assert_eq!(literal_plus(None), None);
    }
}
