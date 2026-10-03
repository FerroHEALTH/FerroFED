// SPDX-FileCopyrightText: Vernum Projecten B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `error` an endpoint record carries for a node that answered with a
//! failure (§9.5, §11.1, §11.2): the node's HTTP status, then an excerpt of
//! the node's own message.
//!
//! Every path that reports a node's failure in `meta.federation.endpoints[]`
//! writes it here: the federated query, the fan-out template upload and the
//! stored-query distribution and drift check. The excerpt is the node's text
//! cut to [`MESSAGE_LIMIT`] characters, with every control, format and
//! separator character turned into a space and every withheld identifier
//! replaced by [`MASK`], so nothing the gateway withholds for the request
//! reaches the answer (§5.4.1, N33). The HTTP client's reason for an
//! `offline` or `time-out` node is held to the same rule.

use http::StatusCode;
use openehr_federation::outcome::ErrorDetail;
use openehr_its::rest::client::ErrorBody;

use crate::hygiene::Withheld;
use crate::hygiene::mask::MASK;

/// The longest excerpt of a node's own message an `error` carries, in
/// characters.
pub const MESSAGE_LIMIT: usize = 512;

/// The member of a node's ITS-REST `Error` read as its error code, which the
/// open `Error` schema admits beside `message` and `validationErrors`.
pub const CODE_MEMBER: &str = "code";

/// The `error` of a node that answered `status` with `body`, carrying the
/// status and an excerpt of the node's message.
///
/// The message is the ITS-REST `Error.message` when the body is one, the
/// body's text otherwise.
#[must_use]
pub fn answered(status: StatusCode, body: &ErrorBody, withheld: &Withheld) -> ErrorDetail {
    said(format!("the node answered {status}"), body, withheld)
}

/// The `error` that reads `lead`, then an excerpt of the node's own message
/// in `body`.
///
/// `lead` is the gateway's text, which names the node's status. When the
/// node sent no message, or nothing printable, the `error` is `lead` alone.
/// When a withheld identifier survives in a form [`Withheld::masked`] cannot
/// replace, the whole message is [`MASK`].
// NOTE: §9.5 and §11.1 carry the node's own failure in `error`, at minimum its
// HTTP status, so the node's message follows the status, bounded and masked.
#[must_use]
pub fn said(lead: String, body: &ErrorBody, withheld: &Withheld) -> ErrorDetail {
    match body.message().or_else(|| body.text()) {
        Some(message) => followed_by(lead, message, withheld),
        None => ErrorDetail::Text(lead),
    }
}

/// The `error` that reads `lead`, then an excerpt of `reason`, text the
/// gateway writes but did not compose: a node's message, or the HTTP
/// client's account of why the node was not reached.
///
/// The excerpt follows the rule of [`said`].
pub(crate) fn followed_by(lead: String, reason: &str, withheld: &Withheld) -> ErrorDetail {
    match excerpt(reason, withheld) {
        Excerpt::Kept(kept) => ErrorDetail::Text(format!("{lead}: {kept}")),
        Excerpt::Withheld => ErrorDetail::Text(format!("{lead}: {MASK}")),
        Excerpt::Empty => ErrorDetail::Text(lead),
    }
}

/// What may be copied of a node's text `said`: at most [`MESSAGE_LIMIT`]
/// printable characters with every withheld identifier replaced, or `None`
/// when one survives in a form [`Withheld::masked`] cannot replace.
pub(crate) fn excerpt_of(said: &str, withheld: &Withheld) -> Option<String> {
    match excerpt(said, withheld) {
        Excerpt::Kept(kept) => Some(kept),
        Excerpt::Empty => Some(String::new()),
        Excerpt::Withheld => None,
    }
}

/// What remains of a node's message once it is cleaned.
enum Excerpt {
    /// The cleaned, bounded excerpt.
    Kept(String),
    /// Nothing printable remained.
    Empty,
    /// A withheld identifier survived the replacement.
    Withheld,
}

fn excerpt(said: &str, withheld: &Withheld) -> Excerpt {
    // NOTE: §5.4.1, N33; masking both before and after the cleaning is our own
    // design: collapsing a gap can join the pieces of an identifier.
    let Some(masked) = withheld.masked(said) else {
        return Excerpt::Withheld;
    };
    let Some(cleaned) = withheld.masked(&printable(&masked)) else {
        return Excerpt::Withheld;
    };
    if cleaned.is_empty() {
        Excerpt::Empty
    } else {
        Excerpt::Kept(bounded(&cleaned))
    }
}

/// `text` with every run of whitespace, control, format and separator
/// characters turned into one space, trimmed at both ends.
fn printable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for character in text.chars() {
        if character.is_whitespace() || is_invisible(character) {
            gap = true;
            continue;
        }
        if gap && !out.is_empty() {
            out.push(' ');
        }
        gap = false;
        out.push(character);
    }
    out
}

/// Whether `character` is a control character or a format character that
/// changes how the text around it reads without showing itself.
fn is_invisible(character: char) -> bool {
    // NOTE: no specification governs this: our own design, the C0 and C1
    // controls and the bidirectional, zero-width and interlinear format marks.
    character.is_control()
        || matches!(
            character,
            '\u{00AD}'
                | '\u{061C}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FEFF}'
                | '\u{FFF9}'..='\u{FFFB}'
        )
}

/// At most [`MESSAGE_LIMIT`] characters of `said`, cut on a character
/// boundary, with `…` where it was cut.
fn bounded(said: &str) -> String {
    let mut kept: String = said.chars().take(MESSAGE_LIMIT).collect();
    if said.chars().nth(MESSAGE_LIMIT).is_some() {
        kept.push('…');
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::{MESSAGE_LIMIT, answered, bounded, excerpt_of, printable};
    use crate::hygiene::Withheld;
    use crate::hygiene::mask::MASK;
    use http::StatusCode;
    use openehr_federation::outcome::ErrorDetail;
    use openehr_its::rest::client::ErrorBody;
    use secrecy::SecretString;

    /// A synthetic subject, the resolution input of a test query.
    const SUBJECT: &str = "SYNTHETIC-SUBJECT-5e2b";

    fn withheld() -> Withheld {
        Withheld::new([SecretString::from(SUBJECT)])
    }

    fn text(detail: &ErrorDetail) -> &str {
        match detail {
            ErrorDetail::Text(text) => text,
            ErrorDetail::Object(_) => "an object",
        }
    }

    #[test]
    fn a_long_node_message_is_cut_on_a_character_boundary() {
        let said = "é".repeat(MESSAGE_LIMIT + 10);
        let kept = bounded(&said);
        assert_eq!(kept.chars().count(), MESSAGE_LIMIT + 1);
        assert!(kept.ends_with('…'));
        assert_eq!(bounded("short"), "short");
    }

    #[test]
    fn controls_and_invisible_format_marks_become_one_space() {
        assert_eq!(
            "line one line two tail",
            printable("\n line one\r\n\tline two\u{202E}\u{200B} tail\u{7}\u{0}")
        );
        assert_eq!("", printable("\u{FEFF}\u{2066}\n"));
    }

    #[test]
    fn a_withheld_subject_is_replaced_in_the_message() {
        let body = ErrorBody::from_bytes(
            format!(r#"{{"message":"no EHR for subject {SUBJECT} at this node"}}"#).into_bytes(),
        );
        let error = answered(StatusCode::BAD_REQUEST, &body, &withheld());
        assert_eq!(
            format!("the node answered 400 Bad Request: no EHR for subject {MASK} at this node"),
            text(&error)
        );
    }

    #[test]
    fn a_withheld_subject_as_an_aql_literal_is_replaced() {
        let quoted = "O'Sentinel-4711";
        let withheld = Withheld::new([SecretString::from(quoted)]);
        assert_eq!(
            Some(format!("WHERE x = '{MASK}'")),
            excerpt_of("WHERE x = 'O\\'Sentinel-4711'", &withheld)
        );
    }

    #[test]
    fn a_percent_encoded_subject_withholds_the_whole_message() {
        let encoded = SUBJECT.replace('-', "%2D");
        let body =
            ErrorBody::from_bytes(format!("rejected /ehr?subject_id={encoded}").into_bytes());
        let error = answered(StatusCode::NOT_FOUND, &body, &withheld());
        assert_eq!(
            format!("the node answered 404 Not Found: {MASK}"),
            text(&error)
        );
        assert!(!text(&error).contains(&encoded));
    }

    #[test]
    fn a_subject_joined_by_the_cleaning_is_still_replaced() {
        let withheld = Withheld::new([SecretString::from("A B")]);
        assert_eq!(
            Some(format!("x {MASK} y")),
            excerpt_of("x A\n\tB y", &withheld)
        );
    }

    #[test]
    fn an_identifier_inside_another_is_never_cut_in_two() {
        let withheld = Withheld::new([
            SecretString::from("SYN-12"),
            SecretString::from("SYN-12345"),
        ]);
        assert_eq!(
            Some(format!("a {MASK} b {MASK} c")),
            excerpt_of("a SYN-12345 b SYN-12 c", &withheld)
        );
    }

    #[test]
    fn a_message_with_nothing_printable_leaves_the_status_alone() {
        let body = ErrorBody::from_bytes(b"\n\t\r\n".to_vec());
        let error = answered(StatusCode::SERVICE_UNAVAILABLE, &body, &Withheld::none());
        assert_eq!("the node answered 503 Service Unavailable", text(&error));
    }

    #[test]
    fn a_node_that_sent_no_body_is_reported_by_its_status() {
        let error = answered(
            StatusCode::FORBIDDEN,
            &ErrorBody::from_bytes(Vec::new()),
            &withheld(),
        );
        assert_eq!("the node answered 403 Forbidden", text(&error));
    }
}
