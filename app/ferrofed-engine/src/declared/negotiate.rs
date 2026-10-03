// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The three HTTP fields the gateway composes from the client's value and
//! the operation's listed values, so a node receives a listed value and never
//! client text: `Accept` (RFC 9110 §12.5.1), `Content-Type` (RFC 9110
//! §8.3) and `Prefer` (RFC 7240 §2).
//!
//! Each field is parsed by its own grammar, and the value composed is always
//! one the ITS-REST operation lists, spelled as the table spells it.

use http::HeaderValue;
use mime::Mime;

/// The `charset` value every listed media type is read in (RFC 8259 §8.1).
const UTF_8: &str = "utf-8";

/// The media-range parameter that carries the weight (RFC 9110 §12.4.2).
const WEIGHT: &str = "q";

/// The text of every field line of `lines`, skipping a line that is not
/// visible ASCII, which names no listed value.
fn texts<'a>(lines: &'a [&'a HeaderValue]) -> impl Iterator<Item = &'a str> {
    // NOTE: RFC 9110 §5.5, a field value outside visible ASCII is opaque, so it names no listed value.
    lines.iter().filter_map(|line| line.to_str().ok())
}

/// The elements of a list-valued field line (RFC 9110 §5.6.1): split at
/// every comma outside a quoted string, each trimmed of optional
/// whitespace, empty elements dropped.
fn elements(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut start = 0;
    for (index, character) in text.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(text.get(start..index).unwrap_or_default());
                start = index.saturating_add(1);
            }
            _ => {}
        }
    }
    out.push(text.get(start..).unwrap_or_default());
    out.into_iter()
        .map(|element| element.trim_matches([' ', '\t']))
        .filter(|element| !element.is_empty())
        .collect()
}

/// A weight in thousandths, from a `qvalue` (RFC 9110 §12.4.2), or `None`
/// when `text` is no `qvalue`.
fn weight(text: &str) -> Option<u16> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let digits = fraction.len();
    if digits > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let scaled = format!("{fraction:0<3}").parse::<u16>().ok()?;
    match whole {
        "0" => Some(scaled),
        "1" if scaled == 0 => Some(1000),
        _ => None,
    }
}

/// Whether the only parameters of `media` other than any `skip` are a `charset`
/// of `utf-8`, which every listed media type is read in.
fn utf_8_only(media: &Mime, skip: Option<&str>) -> bool {
    media.params().all(|(name, value)| {
        let name = name.as_str();
        skip.is_some_and(|skip| name.eq_ignore_ascii_case(skip))
            || (name.eq_ignore_ascii_case(mime::CHARSET.as_str())
                && value.as_str().eq_ignore_ascii_case(UTF_8))
    })
}

/// One media range of an `Accept` value: its precedence and weight.
struct Range {
    media: Mime,
    precedence: u8,
    weight: u16,
}

impl Range {
    /// The media range `element` reads as, or `None` when it is no media
    /// range, its weight is no `qvalue`, or it carries a parameter no listed
    /// media type has.
    fn read(element: &str) -> Option<Self> {
        let media: Mime = element.parse().ok()?;
        let weight = match media.get_param(WEIGHT) {
            Some(value) => weight(value.as_str())?,
            None => 1000,
        };
        // NOTE: RFC 9110 §12.5.1, a range with a parameter matches only a type that has it, and a
        // listed type has none but the utf-8 charset it is read in (RFC 8259 §8.1).
        if !utf_8_only(&media, Some(WEIGHT)) {
            return None;
        }
        let precedence = if media.type_() == mime::STAR {
            0
        } else if media.subtype() == mime::STAR {
            1
        } else {
            2
        };
        Some(Self {
            media,
            precedence,
            weight,
        })
    }

    /// Whether this range covers the media type `offered`.
    fn covers(&self, offered: &Mime) -> bool {
        match self.precedence {
            0 => true,
            1 => self.media.type_() == offered.type_(),
            _ => self.media.type_() == offered.type_() && self.media.subtype() == offered.subtype(),
        }
    }
}

/// The listed media type of `offered` an `Accept` of `lines` prefers, or
/// `None` when it admits none (RFC 9110 §12.5.1).
///
/// Each listed type takes the weight of the most specific range that covers
/// it, the first such range on a tie, and `0` when none does. The heaviest
/// listed type with a weight above `0` wins, the first listed on a tie. A
/// value with no element at all states no preference (RFC 9110 §12.4.1), so
/// the first listed type answers it.
pub(super) fn accept(offered: &[&'static str], lines: &[&HeaderValue]) -> Option<&'static str> {
    let ranges: Vec<Option<Range>> = texts(lines).flat_map(elements).map(Range::read).collect();
    if ranges.is_empty() {
        return offered.first().copied();
    }
    let mut best: Option<(&'static str, u16)> = None;
    for listed in offered {
        let Ok(media) = listed.parse::<Mime>() else {
            continue;
        };
        let mut covering: Option<&Range> = None;
        for range in ranges.iter().flatten() {
            let wins = covering.is_none_or(|held| range.precedence > held.precedence);
            if range.covers(&media) && wins {
                covering = Some(range);
            }
        }
        let weight = covering.map_or(0, |range| range.weight);
        if weight > 0 && best.is_none_or(|(_, held)| weight > held) {
            best = Some((listed, weight));
        }
    }
    best.map(|(listed, _)| listed)
}

/// The listed media type of `accepted` a `Content-Type` of `lines` names, or
/// `None` when it names none (RFC 9110 §8.3).
///
/// The type and subtype are compared without regard to case (RFC 9110
/// §8.3.1). A `charset` of `utf-8` is the one parameter admitted, and is
/// dropped, since a listed value carries none (RFC 8259 §8.1).
pub(super) fn content_type(
    accepted: &[&'static str],
    lines: &[&HeaderValue],
) -> Option<&'static str> {
    let [line] = lines else {
        return None;
    };
    let media: Mime = line.to_str().ok()?.trim_matches([' ', '\t']).parse().ok()?;
    if !utf_8_only(&media, None) {
        return None;
    }
    accepted.iter().copied().find(|listed| {
        listed
            .parse::<Mime>()
            .is_ok_and(|candidate| candidate.essence_str() == media.essence_str())
    })
}

/// The `Prefer` value of the listed preferences of `declared` that `lines`
/// states, each in its listed spelling, or `None` when it states none.
///
/// A preference name is compared without regard to case and its value with
/// it, a quoted value read as its content; only the first instance of a
/// preference counts, and its parameters are dropped (RFC 7240 §2). Any other
/// preference is ignored, never refused (RFC 7240 §2).
pub(super) fn prefer(declared: &[&'static str], lines: &[&HeaderValue]) -> Option<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut kept: Vec<&'static str> = Vec::new();
    for element in texts(lines).flat_map(elements) {
        let head = element.split(';').next().unwrap_or(element);
        let (name, value) = head.split_once('=').unwrap_or((head, ""));
        let name = name.trim_matches([' ', '\t']).to_ascii_lowercase();
        if name.is_empty() || seen.contains(&name) {
            continue;
        }
        let value = unquoted(value.trim_matches([' ', '\t']));
        if let Some(listed) = declared.iter().copied().find(|listed| {
            let (listed_name, listed_value) = listed.split_once('=').unwrap_or((listed, ""));
            listed_name.eq_ignore_ascii_case(&name) && listed_value == value
        }) {
            kept.push(listed);
        }
        seen.push(name);
    }
    (!kept.is_empty()).then(|| kept.join(", "))
}

/// The content of a quoted string (RFC 9110 §5.6.4), or `text` itself when
/// it is a token.
fn unquoted(text: &str) -> String {
    let Some(inner) = text
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    else {
        return text.to_owned();
    };
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for character in inner.chars() {
        if escaped || character != '\\' {
            out.push(character);
            escaped = false;
        } else {
            escaped = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{accept, content_type, elements, prefer, weight};
    use http::HeaderValue;

    const LOCATABLE: [&str; 4] = [
        "application/json",
        "application/xml",
        "application/openehr.wt.flat+json",
        "application/openehr.wt.structured+json",
    ];

    const RETURN: [&str; 3] = [
        "return=representation",
        "return=minimal",
        "return=identifier",
    ];

    fn line(text: &str) -> HeaderValue {
        HeaderValue::from_str(text).expect("a test header value")
    }

    fn accepted(text: &str) -> Option<&'static str> {
        let value = line(text);
        accept(&LOCATABLE, &[&value])
    }

    #[test]
    fn any_media_type_is_answered_by_the_first_listed() {
        assert_eq!(Some("application/json"), accepted("*/*"));
        assert_eq!(Some("application/json"), accepted("application/*"));
        assert_eq!(Some("application/json"), accept(&LOCATABLE, &[]));
        assert_eq!(Some("application/json"), accepted(" , "));
    }

    #[test]
    fn a_list_picks_the_heaviest_listed_match() {
        assert_eq!(
            Some("application/xml"),
            accepted("text/html, application/xml;q=0.9, */*;q=0.1")
        );
        assert_eq!(
            Some("application/openehr.wt.flat+json"),
            accepted("application/json;q=0.5, application/openehr.wt.flat+json")
        );
        assert_eq!(
            Some("application/xml"),
            accepted("application/*;q=0.2, application/xml, application/json;q=0")
        );
        assert_eq!(Some("application/json"), accepted("Application/JSON"));
    }

    #[test]
    fn the_most_specific_range_sets_the_weight() {
        assert_eq!(
            Some("application/xml"),
            accepted("application/json;q=0, application/*;q=0.5, */*;q=0.1")
        );
    }

    #[test]
    fn a_utf_8_charset_is_read_and_any_other_parameter_matches_nothing() {
        assert_eq!(
            Some("application/json"),
            accepted("application/json; charset=UTF-8")
        );
        assert_eq!(None, accepted("application/json; patient=4711"));
        assert_eq!(None, accepted("application/json; charset=latin1"));
    }

    #[test]
    fn nothing_listed_or_a_zero_weight_is_not_acceptable() {
        assert_eq!(None, accepted("text/html"));
        assert_eq!(None, accepted("*/*;q=0"));
        assert_eq!(None, accepted("application/json;q=2"));
        assert_eq!(None, accepted("4711"));
    }

    #[test]
    fn a_qvalue_follows_its_grammar() {
        assert_eq!(Some(1000), weight("1"));
        assert_eq!(Some(1000), weight("1.000"));
        assert_eq!(Some(500), weight("0.5"));
        assert_eq!(Some(1), weight("0.001"));
        assert_eq!(None, weight("1.5"));
        assert_eq!(None, weight("0.0001"));
        assert_eq!(None, weight("-0"));
    }

    #[test]
    fn a_quoted_comma_splits_no_element() {
        assert_eq!(vec!["a;x=\"1,2\"", "b"], elements(" a;x=\"1,2\" ,, b,"));
    }

    #[test]
    fn a_content_type_is_its_listed_media_type() {
        let sent = |text: &str| content_type(&LOCATABLE, &[&line(text)]);
        assert_eq!(Some("application/json"), sent("application/json"));
        assert_eq!(
            Some("application/json"),
            sent("Application/JSON; charset=utf-8")
        );
        assert_eq!(None, sent("application/json; charset=utf-16"));
        assert_eq!(None, sent("application/json; patient=4711"));
        assert_eq!(None, sent("text/plain"));
        assert_eq!(None, sent("4711"));
        assert_eq!(None, content_type(&LOCATABLE, &[]));
        let two = [line("application/json"), line("application/xml")];
        assert_eq!(None, content_type(&LOCATABLE, &[&two[0], &two[1]]));
    }

    #[test]
    fn prefer_keeps_the_first_listed_preference_in_its_listed_spelling() {
        let sent = |text: &str| prefer(&RETURN, &[&line(text)]);
        assert_eq!(Some("return=minimal".to_owned()), sent("return=minimal"));
        assert_eq!(
            Some("return=minimal".to_owned()),
            sent("RETURN = \"minimal\"; patient=4711, respond-async, wait=10")
        );
        assert_eq!(
            Some("return=representation".to_owned()),
            sent("patient=4711, return=representation, return=minimal")
        );
        assert_eq!(None, sent("return=MINIMAL"));
        assert_eq!(None, sent("return=4711, return=minimal"));
        assert_eq!(None, sent("4711"));
    }
}
