// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The syntax highlighting of the AQL and the JSON the console shows,
//! rendered on the server.
//!
//! Each tokenizer is one linear pass over the characters of its input, so a
//! multi-byte character is never split, and the texts of its tokens joined
//! together are the input exactly: text the grammar does not know is a
//! [`Kind::Plain`] token, never dropped. The colours a token takes are
//! decoration over text that reads the same without them, and the code
//! block's markup is written by `view!`, which escapes every token's text.
//! No specification governs the highlighting: our own design.

use std::iter::Peekable;
use std::str::Chars;

use leptos::prelude::*;

/// The class of one token, which names the class it is painted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Whitespace, and text the grammar does not classify.
    Plain,
    /// An AQL keyword, or a JSON literal (`true`, `false`, `null`).
    Keyword,
    /// An AQL identifier: a variable, an RM type, a function.
    Identifier,
    /// An AQL path or predicate: `/data[at0001]/items`, `[openEHR-EHR-…]`.
    Path,
    /// A quoted string.
    String,
    /// An AQL parameter, `$name`.
    Parameter,
    /// A number.
    Number,
    /// An AQL comment, from `--` to the end of the line.
    Comment,
    /// A JSON member name.
    Key,
    /// Punctuation: brackets, commas, operators.
    Punctuation,
}

impl Kind {
    /// The class the stylesheet paints a token of this kind with, or `None`
    /// for text drawn as it stands.
    #[must_use]
    pub const fn class(self) -> Option<&'static str> {
        match self {
            Self::Plain | Self::Identifier | Self::Punctuation => None,
            Self::Keyword => Some("tok-keyword"),
            Self::Path => Some("tok-path"),
            Self::String => Some("tok-string"),
            Self::Parameter => Some("tok-parameter"),
            Self::Number => Some("tok-number"),
            Self::Comment => Some("tok-comment"),
            Self::Key => Some("tok-key"),
        }
    }
}

/// One token: its kind and the exact text of the input it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// What the text is.
    pub kind: Kind,
    /// The text, as the input has it.
    pub text: String,
}

/// The longest input the tokenizers classify; a longer one is one
/// [`Kind::Plain`] token, so a page never carries a span per character of a
/// very large text.
pub const MAX_HIGHLIGHT_BYTES: usize = 256 * 1024;

/// The AQL keywords, in upper case (openEHR AQL, the grammar's reserved
/// words), matched in any case.
const AQL_KEYWORDS: &[&str] = &[
    "AND",
    "AS",
    "ASC",
    "ASCENDING",
    "BY",
    "CONTAINS",
    "DESC",
    "DESCENDING",
    "DISTINCT",
    "EXISTS",
    "FALSE",
    "FETCH",
    "FROM",
    "LIKE",
    "LIMIT",
    "MATCHES",
    "NOT",
    "NULL",
    "OFFSET",
    "OR",
    "ORDER",
    "SELECT",
    "TOP",
    "TRUE",
    "WHERE",
    "XOR",
];

/// The tokens of the AQL text `aql`.
#[must_use]
pub fn aql(aql: &str) -> Vec<Token> {
    if aql.len() > MAX_HIGHLIGHT_BYTES {
        return plain(aql);
    }
    let mut out = Vec::new();
    let mut chars = aql.chars().peekable();
    while let Some(&next) = chars.peek() {
        let (kind, text) = match next {
            '\'' | '"' => (Kind::String, quoted(&mut chars)),
            '$' => (Kind::Parameter, parameter(&mut chars)),
            '[' => (Kind::Path, predicate(&mut chars)),
            '/' => path(&mut chars),
            '-' => comment_or_dash(&mut chars),
            c if c.is_ascii_digit() => (Kind::Number, number(&mut chars)),
            c if starts_word(c) => {
                let word = take_while(&mut chars, continues_word);
                let kind = if is_keyword(&word) {
                    Kind::Keyword
                } else {
                    Kind::Identifier
                };
                (kind, word)
            }
            c if c.is_whitespace() => (Kind::Plain, take_while(&mut chars, char::is_whitespace)),
            c if "(),;=<>!*.{}:+".contains(c) => (Kind::Punctuation, one(&mut chars)),
            _ => (Kind::Plain, one(&mut chars)),
        };
        push(&mut out, kind, text);
    }
    out
}

/// The tokens of the JSON text `json`.
#[must_use]
pub fn json(json: &str) -> Vec<Token> {
    if json.len() > MAX_HIGHLIGHT_BYTES {
        return plain(json);
    }
    let mut out = Vec::new();
    let mut chars = json.chars().peekable();
    while let Some(&next) = chars.peek() {
        let (kind, text) = match next {
            '"' => {
                let text = quoted(&mut chars);
                // NOTE: RFC 8259 §4: a member name is the string a `:` follows,
                // whitespace allowed between them.
                let kind = if names_a_member(&chars) {
                    Kind::Key
                } else {
                    Kind::String
                };
                (kind, text)
            }
            '-' | '0'..='9' => (Kind::Number, number(&mut chars)),
            c if c.is_ascii_alphabetic() => {
                let word = take_while(&mut chars, |c| c.is_ascii_alphabetic());
                let kind = if matches!(word.as_str(), "true" | "false" | "null") {
                    Kind::Keyword
                } else {
                    Kind::Plain
                };
                (kind, word)
            }
            c if c.is_whitespace() => (Kind::Plain, take_while(&mut chars, char::is_whitespace)),
            '{' | '}' | '[' | ']' | ',' | ':' => (Kind::Punctuation, one(&mut chars)),
            _ => (Kind::Plain, one(&mut chars)),
        };
        push(&mut out, kind, text);
    }
    out
}

/// The code block of `tokens`, `label` naming the region it scrolls in:
/// each painted token a span, the rest text, every text escaped by `view!`.
#[must_use]
pub fn code_block(label: String, tokens: Vec<Token>) -> impl IntoView {
    let spans = tokens
        .into_iter()
        .map(|token| match token.kind.class() {
            Some(class) => {
                leptos::either::Either::Left(view! { <span class=class>{token.text}</span> })
            }
            None => leptos::either::Either::Right(token.text),
        })
        .collect_view();
    view! {
        <pre class="code-block" role="region" aria-label=label tabindex="0">
            <code>{spans}</code>
        </pre>
    }
}

/// The whole of `text` as one plain token, none for empty text.
fn plain(text: &str) -> Vec<Token> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![Token {
        kind: Kind::Plain,
        text: text.to_owned(),
    }]
}

/// Adds `text` as a token of `kind`, joining it to the last token when that
/// is of the same kind, so a run costs one span.
fn push(out: &mut Vec<Token>, kind: Kind, text: String) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.kind == kind
    {
        last.text.push_str(&text);
        return;
    }
    out.push(Token { kind, text });
}

/// The next character alone.
fn one(chars: &mut Peekable<Chars<'_>>) -> String {
    chars.next().map(String::from).unwrap_or_default()
}

/// The run of characters from here that `accept` takes.
fn take_while(chars: &mut Peekable<Chars<'_>>, accept: impl Fn(char) -> bool) -> String {
    let mut text = String::new();
    while let Some(c) = chars.next_if(|c| accept(*c)) {
        text.push(c);
    }
    text
}

/// A string from its opening quote to the same quote closing it, a
/// backslash escaping the character after it; an unclosed string runs to
/// the end.
fn quoted(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut text = String::new();
    let Some(quote) = chars.next() else {
        return text;
    };
    text.push(quote);
    let mut escaped = false;
    for c in chars.by_ref() {
        text.push(c);
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            break;
        }
    }
    text
}

/// Whether the next character that is not whitespace is `:`.
fn names_a_member(chars: &Peekable<Chars<'_>>) -> bool {
    chars.clone().find(|c| !c.is_whitespace()) == Some(':')
}

/// Whether `c` starts a word: a letter in any script, or `_`.
fn starts_word(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// Whether `c` continues a word.
fn continues_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `word` is an AQL keyword, in any case.
fn is_keyword(word: &str) -> bool {
    AQL_KEYWORDS
        .iter()
        .any(|keyword| keyword.eq_ignore_ascii_case(word))
}

/// A parameter: `$` and the word after it.
fn parameter(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut text = one(chars);
    text.push_str(&take_while(chars, continues_word));
    text
}

/// A number: digits, a fraction, an exponent, and a leading minus in JSON.
fn number(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut text = String::new();
    if let Some(minus) = chars.next_if_eq(&'-') {
        text.push(minus);
    }
    let mut previous = None;
    while let Some(c) = chars.next_if(|&c| {
        c.is_ascii_digit()
            || matches!(c, '.' | 'e' | 'E')
            || (matches!(c, '+' | '-') && matches!(previous, Some('e' | 'E')))
    }) {
        text.push(c);
        previous = Some(c);
    }
    text
}

/// A predicate from `[` to the `]` that closes it, strings inside it taken
/// whole, so a `]` in a quoted value does not end it.
fn predicate(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut text = one(chars);
    let mut depth = 1_u32;
    while let Some(&c) = chars.peek() {
        match c {
            '\'' | '"' => text.push_str(&quoted(chars)),
            _ => {
                text.push_str(&one(chars));
                if c == '[' {
                    depth = depth.saturating_add(1);
                } else if c == ']' {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        break;
                    }
                }
            }
        }
    }
    text
}

/// A path from its `/`: each step's name and its predicates, or the `/`
/// alone as punctuation when no name follows it.
fn path(chars: &mut Peekable<Chars<'_>>) -> (Kind, String) {
    let mut text = String::new();
    while chars.peek() == Some(&'/') {
        text.push_str(&one(chars));
        text.push_str(&take_while(chars, continues_word));
        while chars.peek() == Some(&'[') {
            text.push_str(&predicate(chars));
        }
    }
    if text == "/" {
        (Kind::Punctuation, text)
    } else {
        (Kind::Path, text)
    }
}

/// A comment from `--` to the end of the line, or a `-` alone as
/// punctuation.
fn comment_or_dash(chars: &mut Peekable<Chars<'_>>) -> (Kind, String) {
    let dash = one(chars);
    if chars.peek() != Some(&'-') {
        return (Kind::Punctuation, dash);
    }
    let mut text = dash;
    text.push_str(&take_while(chars, |c| c != '\n' && c != '\r'));
    (Kind::Comment, text)
}

#[cfg(test)]
mod tests {
    use super::{Kind, Token, aql, json};

    /// The tokens of `tokens` that are not whitespace, as kind and text
    /// with the whitespace a plain run joins at its ends left off.
    fn shown(tokens: &[Token]) -> Vec<(Kind, &str)> {
        tokens
            .iter()
            .filter(|token| !token.text.trim().is_empty())
            .map(|token| (token.kind, token.text.trim()))
            .collect()
    }

    /// The input the tokens of `tokens` join to.
    fn joined(tokens: &[Token]) -> String {
        tokens.iter().map(|token| token.text.as_str()).collect()
    }

    const QUERY: &str = "SELECT c/uid/value, o/data[at0001]/events[at0006 and name/value='Any \
                         event']/data/items[at0004]/value/magnitude AS weight\n\
                         FROM EHR e[ehr_id/value=$ehr_id] CONTAINS COMPOSITION \
                         c[openEHR-EHR-COMPOSITION.encounter.v1]\n\
                         -- the newest first\n\
                         WHERE o/time > '2020-01-01' and 1.5e3 >= 10 ORDER BY c/context/start_time DESC LIMIT 10";

    #[test]
    fn every_kind_of_aql_text_has_its_token() {
        let tokens = aql(QUERY);
        let shown = shown(&tokens);
        assert_eq!(Some(&(Kind::Keyword, "SELECT")), shown.first());
        assert!(shown.contains(&(Kind::Identifier, "c")));
        assert!(shown.contains(&(Kind::Path, "/uid/value")));
        assert!(shown.contains(&(
            Kind::Path,
            "/data[at0001]/events[at0006 and name/value='Any event']/data/items[at0004]/value/magnitude"
        )));
        assert!(shown.contains(&(Kind::Keyword, "AS")));
        assert!(shown.contains(&(Kind::Path, "[ehr_id/value=$ehr_id]")));
        assert!(shown.contains(&(Kind::Path, "[openEHR-EHR-COMPOSITION.encounter.v1]")));
        assert!(shown.contains(&(Kind::Comment, "-- the newest first")));
        assert!(shown.contains(&(Kind::String, "'2020-01-01'")));
        assert!(shown.contains(&(Kind::Keyword, "and")));
        assert!(shown.contains(&(Kind::Number, "1.5e3")));
        assert!(shown.contains(&(Kind::Number, "10")));
        assert!(shown.contains(&(Kind::Punctuation, ">=")));
        assert!(shown.contains(&(Kind::Keyword, "DESC")));
        assert_eq!(QUERY, joined(&tokens));
    }

    #[test]
    fn a_parameter_is_its_dollar_and_its_name() {
        let tokens = aql("WHERE e/ehr_id/value = $ehr_id AND c/name/value = $name_2");
        let shown = shown(&tokens);
        assert!(shown.contains(&(Kind::Parameter, "$ehr_id")));
        assert!(shown.contains(&(Kind::Parameter, "$name_2")));
        assert!(shown.contains(&(Kind::Punctuation, "=")));
    }

    #[test]
    fn multi_byte_text_is_never_split_and_reads_back_whole() {
        let text = "SELECT c FROM EHR e CONTAINS COMPOSITION c WHERE c/name/value = 'Überweisung ✓ 患者' \
                    AND größe > 1 -- naïve ☃\nAND 名前 = $für";
        let tokens = aql(text);
        assert_eq!(text, joined(&tokens));
        let shown = shown(&tokens);
        assert!(shown.contains(&(Kind::String, "'Überweisung ✓ 患者'")));
        assert!(shown.contains(&(Kind::Identifier, "größe")));
        assert!(shown.contains(&(Kind::Identifier, "名前")));
        assert!(shown.contains(&(Kind::Comment, "-- naïve ☃")));
        assert!(shown.contains(&(Kind::Parameter, "$für")));
    }

    #[test]
    fn unknown_or_unfinished_text_is_kept_as_it_stands() {
        for text in [
            "SELECT 'unclosed",
            "c/data[at0001",
            "a & b ~ c ` @ # %",
            "-",
            "--",
            "/",
            "$",
            "\u{1F600}\u{200D}\u{1F4BB}",
            "",
        ] {
            assert_eq!(text, joined(&aql(text)), "{text:?}");
            assert_eq!(text, joined(&json(text)), "{text:?}");
        }
        let shown_text = aql("a & b");
        assert!(shown(&shown_text).contains(&(Kind::Plain, "&")));
    }

    #[test]
    fn a_text_over_the_limit_is_one_plain_token() {
        let long = "x".repeat(super::MAX_HIGHLIGHT_BYTES + 1);
        let tokens = aql(&long);
        assert_eq!(1, tokens.len());
        assert_eq!(Some(Kind::Plain), tokens.first().map(|token| token.kind));
    }

    #[test]
    fn json_names_its_keys_strings_numbers_and_literals() {
        let text = "{\n  \"id\": \"example-federation\",\n  \"per_node_ms\": -1.5E+3,\n  \
                    \"fan_out\" : true,\n  \"auth\": null,\n  \"modes\": [\"a:b\", false],\n  \"name\": \"Zoë\"\n}";
        let tokens = json(text);
        assert_eq!(text, joined(&tokens));
        let shown = shown(&tokens);
        assert!(shown.contains(&(Kind::Key, "\"id\"")));
        assert!(shown.contains(&(Kind::String, "\"example-federation\"")));
        assert!(shown.contains(&(Kind::Number, "-1.5E+3")));
        assert!(shown.contains(&(Kind::Key, "\"fan_out\"")));
        assert!(shown.contains(&(Kind::Keyword, "true")));
        assert!(shown.contains(&(Kind::Keyword, "null")));
        assert!(shown.contains(&(Kind::String, "\"a:b\"")));
        assert!(shown.contains(&(Kind::Keyword, "false")));
        assert!(shown.contains(&(Kind::String, "\"Zoë\"")));
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        let text = r#"{"q": "say \"hi\"", "n": 1}"#;
        let shown_tokens = json(text);
        assert!(shown(&shown_tokens).contains(&(Kind::String, r#""say \"hi\"""#)));
        let aql_text = r"WHERE x = 'it\'s' AND y = 1";
        assert!(shown(&aql(aql_text)).contains(&(Kind::String, r"'it\'s'")));
    }
}
