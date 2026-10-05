// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The client deadline a request asks for with `Prefer: wait` (§11.5, N38;
//! RFC 7240 §4.3).
//!
//! `Prefer` is a comma-separated list of preferences over one or more header
//! lines, each a case-insensitive token with an optional value and optional
//! `;` parameters (RFC 7240 §2). Only the first `wait` counts, and its value
//! is `delta-seconds`, a run of digits. A server ignores a preference it does
//! not understand (RFC 7240 §2), so a malformed `wait` is ignored and the
//! request runs under the gateway's own budget: it is never refused, and
//! never read as some other deadline. A count past what the clock holds
//! saturates, which a client cannot use to extend the budget anyway.

use std::time::Duration;

use http::HeaderMap;

/// The request header a client states its preferences in (RFC 7240 §2).
pub const PREFER: &str = "prefer";

/// The response header naming the preferences the gateway applied (RFC 7240
/// §3).
pub const PREFERENCE_APPLIED: &str = "preference-applied";

/// The `wait` preference token (RFC 7240 §4.3).
const WAIT: &str = "wait";

/// The deadline the first `wait` preference of `headers` asks for, or `None`
/// when there is none or the first one is malformed.
#[must_use]
pub fn wait(headers: &HeaderMap) -> Option<Duration> {
    let first = headers
        .get_all(PREFER)
        .iter()
        // NOTE: RFC 7240 §2, a header line that is not visible ASCII is not
        // a preference list this server understands, so it carries no wait.
        .filter_map(|line| line.to_str().ok())
        .flat_map(elements)
        .find(|element| name(element).eq_ignore_ascii_case(WAIT))?;
    seconds(first)
}

/// Splits one header line into its list elements, at the commas outside a
/// quoted string (RFC 9110 §5.6.1, §5.6.4).
fn elements(line: &str) -> Vec<&str> {
    split_unquoted(line, ',')
        .into_iter()
        .map(str::trim)
        .filter(|element| !element.is_empty())
        .collect()
}

/// The preference name of one list element: the token before any `=` or `;`.
fn name(element: &str) -> &str {
    let end = element.find(['=', ';']).unwrap_or(element.len());
    element.get(..end).map_or(element, str::trim)
}

/// The `delta-seconds` of a `wait` element, or `None` when its value is
/// missing or is not a run of digits (RFC 7240 §4.3, RFC 9111 §1.2.2).
fn seconds(element: &str) -> Option<Duration> {
    let preference = split_unquoted(element, ';').into_iter().next()?;
    let (_, value) = preference.split_once('=')?;
    let value = value.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // NOTE: RFC 9111 §1.2.2, a delta-seconds too large to represent is taken
    // as the greatest value representable.
    Some(Duration::from_secs(
        value.parse::<u64>().unwrap_or(u64::MAX),
    ))
}

/// Splits `text` at every `separator` outside a quoted string, where a
/// backslash inside one escapes the next character (RFC 9110 §5.6.4).
fn split_unquoted(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (at, character) in text.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            _ if character == separator && !quoted => {
                parts.extend(text.get(start..at));
                start = at + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.extend(text.get(start..));
    parts
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::{HeaderMap, HeaderValue};

    use super::{PREFER, wait};

    fn headers(lines: &[&'static str]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for line in lines {
            map.append(PREFER, HeaderValue::from_static(line));
        }
        map
    }

    fn secs(seconds: u64) -> Option<Duration> {
        Duration::from_secs(seconds).into()
    }

    #[test]
    fn no_header_asks_for_no_deadline() {
        assert_eq!(wait(&headers(&[])), None);
    }

    #[test]
    fn a_wait_is_read_in_seconds() {
        assert_eq!(wait(&headers(&["wait=5"])), secs(5));
        assert_eq!(wait(&headers(&["wait=0"])), secs(0));
        assert_eq!(wait(&headers(&["wait = 7"])), secs(7));
    }

    #[test]
    fn the_token_is_case_insensitive() {
        assert_eq!(wait(&headers(&["WAIT=3"])), secs(3));
        assert_eq!(wait(&headers(&["Wait=3"])), secs(3));
    }

    #[test]
    fn a_wait_is_found_among_other_preferences_and_lines() {
        assert_eq!(wait(&headers(&["return=minimal, wait=4"])), secs(4));
        assert_eq!(wait(&headers(&["respond-async", "wait=9"])), secs(9));
        assert_eq!(wait(&headers(&["wait=2; foo=bar"])), secs(2));
        assert_eq!(wait(&headers(&[",, wait=6 ,"])), secs(6));
    }

    #[test]
    fn a_comma_inside_a_quoted_value_does_not_split_the_list() {
        assert_eq!(wait(&headers(&[r#"foo="a, wait=1", wait=8"#])), secs(8));
        assert_eq!(wait(&headers(&[r#"foo="a\", wait=1", wait=8"#])), secs(8));
    }

    #[test]
    fn only_the_first_wait_counts() {
        assert_eq!(wait(&headers(&["wait=3, wait=1"])), secs(3));
        assert_eq!(wait(&headers(&["wait=3", "wait=1"])), secs(3));
        assert_eq!(wait(&headers(&["wait=soon, wait=1"])), None);
    }

    #[test]
    fn a_malformed_wait_is_ignored() {
        for line in [
            "wait",
            "wait=",
            "wait=-1",
            "wait=1.5",
            "wait=+3",
            "wait=3s",
            "wait=\"3\"",
            "wait=0x10",
            "waiting=3",
        ] {
            assert_eq!(wait(&headers(&[line])), None, "{line:?}");
        }
    }

    #[test]
    fn a_wait_too_large_for_the_clock_saturates() {
        assert_eq!(
            wait(&headers(&["wait=99999999999999999999999"])),
            Some(Duration::from_secs(u64::MAX))
        );
    }
}
