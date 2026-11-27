// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Holds the book's quoted removal refusals to the declarations of
//! [`ALL`](super::ALL).
//!
//! A page quotes a removal refusal in one of two ways. It writes the message
//! `config check` gives (`<key> was replaced by …` or `<key> was removed with
//! no replacement: …`), which must be the message a declaration renders. Or
//! it says in prose that a key is refused by name or with its replacement,
//! and every configuration key that sentence names must be declared. A
//! dated record states what an earlier release did, so a table row or a
//! section whose heading names a release is left out of the prose check. No
//! specification governs this: our own design.

use std::path::{Path, PathBuf};

use ferrohealth_config::removed::{Removals, Removed, Retired};

use super::ALL;

/// The two forms `ferrohealth_config::removed::Removal` renders a refusal in.
const MESSAGES: [&str; 2] = [" was replaced by ", " was removed with no replacement: "];

/// The words that say a sentence describes the named refusal of a retired
/// key, matched in lower case.
const PROSE: [&str; 9] = [
    "refused by name",
    "refuses by name",
    "refuses it by name",
    "naming its replacement",
    "naming the replacement",
    "naming the key that replaces",
    "naming its new key",
    "no longer read",
    "configuration that still sets",
];

/// One page, relative to the book's `src` directory, and its text.
struct Page {
    name: String,
    text: String,
}

/// Returns the book's `src` directory.
fn book() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../website/book/src")
}

/// Returns every Markdown page under `dir`, in path order.
fn pages(root: &Path, dir: &Path, out: &mut Vec<Page>) {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            pages(root, &path, out);
        } else if path.extension().is_some_and(|ext| ext == "md") {
            out.push(Page {
                name: path.strip_prefix(root).unwrap().display().to_string(),
                text: std::fs::read_to_string(&path).unwrap(),
            });
        }
    }
}

/// Returns the message `retired` renders when the sources set it at `key`,
/// which is its own key or its key under a repeating table.
fn message(retired: &Retired, key: &str) -> String {
    let set: Removed = serde_json::from_str("true").unwrap();
    let mut removals = Removals::new();
    if key == retired.key() {
        removals.check(set, retired);
    } else {
        let table = key.strip_suffix(retired.key()).unwrap();
        removals.check_under(set, retired, table.strip_suffix('.').unwrap());
    }
    removals.removals()[0].to_string()
}

/// Whether `key` names `retired`, as its own key or under a repeating table.
fn names(retired: &Retired, key: &str) -> bool {
    key == retired.key()
        || key
            .strip_suffix(retired.key())
            .is_some_and(|table| table.ends_with('.'))
}

/// The paragraphs of `text` with their lines joined, each fenced code line
/// on its own, with the heading each sits under.
fn paragraphs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut heading = String::new();
    let mut current = String::new();
    let mut fenced = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            flush(&mut current, &heading, &mut out);
            fenced = !fenced;
        } else if fenced {
            out.push((heading.clone(), line.to_owned()));
        } else if trimmed.starts_with('#') {
            flush(&mut current, &heading, &mut out);
            trimmed.clone_into(&mut heading);
        } else if trimmed.is_empty() || trimmed.starts_with('|') {
            flush(&mut current, &heading, &mut out);
            if trimmed.starts_with('|') {
                out.push((heading.clone(), trimmed.to_owned()));
            }
        } else {
            current.push(' ');
            current.push_str(trimmed);
        }
    }
    flush(&mut current, &heading, &mut out);
    out
}

/// Moves the paragraph gathered in `current` to `out`, under `heading`.
fn flush(current: &mut String, heading: &str, out: &mut Vec<(String, String)>) {
    if !current.trim().is_empty() {
        out.push((heading.to_owned(), current.trim().to_owned()));
    }
    current.clear();
}

/// The key quoted right before `at` in `text`, the run of characters after
/// the last space or parenthesis without its opening backtick or quote, with
/// the offset it starts at.
fn key_before(text: &str, at: usize) -> (usize, &str) {
    let head = text.get(..at).unwrap_or_default();
    let mut start = head.rfind([' ', '(']).map_or(0, |space| space + 1);
    while head
        .get(start..)
        .is_some_and(|rest| rest.starts_with(['`', '"']))
    {
        start += 1;
    }
    (start, head.get(start..).unwrap_or_default())
}

/// Every refusal message `text` quotes that no declaration renders.
fn unheld_messages(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for form in MESSAGES {
        for (at, _) in text.match_indices(form) {
            let (start, key) = key_before(text, at);
            let held = ALL
                .iter()
                .filter(|retired| names(retired, key))
                .any(|retired| {
                    text.get(start..)
                        .is_some_and(|quoted| quoted.starts_with(&message(retired, key)))
                });
            if !held {
                out.push(key.to_owned());
            }
        }
    }
    out
}

/// Whether `token`, a code span's text, reads as a configuration key or
/// table: lower-case words joined by dots, optionally in brackets.
fn is_key(token: &str) -> bool {
    let bare = token.trim_start_matches('[').trim_end_matches(']');
    !bare.is_empty()
        && bare.starts_with(|c: char| c.is_ascii_lowercase())
        && bare
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.')
}

/// The code spans of `text`.
fn spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// Whether `text` names a release, such as `0.0.19` or `v0.0.19`.
fn names_release(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .any(|word| {
            let parts: Vec<&str> = word.trim_matches('.').split('.').collect();
            parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
        })
}

/// The sentences of `text`, split after a `.`, `;` or `:` that a space or
/// the end follows outside a code span.
fn sentences(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut code = false;
    let mut chars = text.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c == '`' {
            code = !code;
        } else if !code
            && matches!(c, '.' | ';' | ':')
            && chars.peek().is_none_or(|(_, next)| next.is_whitespace())
        {
            out.push(text.get(start..=at).unwrap_or_default());
            start = at + 1;
        }
    }
    out.push(text.get(start..).unwrap_or_default());
    out
}

/// Every key a prose sentence of `text` says is refused by name that no
/// declaration holds.
fn unheld_prose(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for sentence in sentences(text) {
        let lower = sentence.to_lowercase();
        if !PROSE.iter().any(|words| lower.contains(words)) {
            continue;
        }
        for token in spans(sentence).filter(|token| is_key(token)) {
            if !ALL.iter().any(|retired| names(retired, token)) {
                out.push(token.to_owned());
            }
        }
    }
    out
}

/// Returns every quoted refusal of `pages` that no declaration holds, as
/// `page: key`.
fn unheld(pages: &[Page]) -> Vec<String> {
    let mut out = Vec::new();
    for page in pages {
        for (heading, text) in paragraphs(&page.text) {
            let mut keys = unheld_messages(&text);
            let record = names_release(&heading) || (text.starts_with('|') && names_release(&text));
            if !record {
                keys.extend(unheld_prose(&text));
            }
            out.extend(keys.into_iter().map(|key| format!("{}: {key}", page.name)));
        }
    }
    out
}

#[test]
fn every_refusal_the_book_quotes_is_one_the_gateway_gives() {
    let root = book();
    let mut all = Vec::new();
    pages(&root, &root, &mut all);
    assert!(!all.is_empty(), "no page under {}", root.display());
    let unheld = unheld(&all);
    assert!(
        unheld.is_empty(),
        "these pages quote a removal refusal config::retired does not declare: {unheld:#?}"
    );
}

#[test]
fn a_message_no_declaration_renders_is_found() {
    let page = Page {
        name: "operate/sample.md".to_owned(),
        text: "Run `config check`:\n\n```text\nserver.timeout_ms was replaced by server.timeout\n```\n"
            .to_owned(),
    };
    assert_eq!(unheld(&[page]), ["operate/sample.md: server.timeout_ms"]);
}

#[test]
fn a_sentence_naming_an_undeclared_key_as_refused_by_name_is_found() {
    let page = Page {
        name: "operate/identity.md".to_owned(),
        text: "A configuration that still sets\n`[dev]` is refused at load.\n\n\
               The `oauth2` table is refused, naming its replacement.\n"
            .to_owned(),
    };
    assert_eq!(
        unheld(&[page]),
        ["operate/identity.md: [dev]", "operate/identity.md: oauth2"]
    );
}

#[test]
fn a_dated_record_and_a_sentence_naming_no_key_pass() {
    let page = Page {
        name: "evaluate/record.md".to_owned(),
        text: "A key the new release removed is refused by name.\n\n\
               | 10 | 2026-11-05 | 0.0.16 | the `site_root` key is refused by name |\n\n\
               ### To v0.0.22, from v0.0.21\n\n- `server.foo` is refused, naming its replacement.\n"
            .to_owned(),
    };
    assert_eq!(unheld(&[page]), Vec::<String>::new());
}

#[test]
fn a_declaration_renders_the_message_config_check_gives() {
    const DECLARED: Retired =
        Retired::replaced("server.timeout_ms", "server.timeout, a duration", "0.0.22");
    let rendered = message(&DECLARED, "server.timeout_ms");
    assert_eq!(
        rendered,
        "server.timeout_ms was replaced by server.timeout, a duration"
    );
    assert!(names(&DECLARED, "auth.issuer[0].server.timeout_ms"));
    assert!(!names(&DECLARED, "xserver.timeout_ms"));
}
