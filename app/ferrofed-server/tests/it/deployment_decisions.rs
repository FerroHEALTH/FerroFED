// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The book's §13.4 page answers every obligation §13.4 puts on a
//! deployment, each in a section of its own that cites the obligation and
//! names the configuration that changes the answer, and carries the
//! operator's template with one entry per obligation (§13.4, N25). CP-39 is
//! an operator point, scored against a deployment and never against the
//! gateway (§16.2), so these tests carry no conformance marker; the matrix
//! records the page as its documentation.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

/// The page.
const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/deployment-decisions.md"
);

/// The client authentication page, which links it.
const AUTHENTICATION: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/authentication.md"
);

/// Each §13.4 obligation: the page's heading for it, the anchor the
/// specification gives it, and the line the template opens it with.
const OBLIGATIONS: [(&str, &str, &str); 5] = [
    (
        "## 1. Which identity is verified across the trust boundary",
        "authn-which-identity",
        "1. Identity across the trust boundary",
    ),
    (
        "## 2. Who authenticates the end user, and where that trust stops",
        "authn-end-user",
        "2. End-user authentication",
    ),
    (
        "## 3. Purpose of use",
        "authn-purpose-of-use",
        "3. Purpose of use",
    ),
    (
        "## 4. What the token is bound to",
        "authn-token-binding",
        "4. Token binding",
    ),
    (
        "## 5. What the technique does not cover",
        "authn-residual-risk",
        "5. Risk carried by agreement",
    ),
];

/// The page's `## ` sections, each its heading line and its body, in order.
fn sections(text: &str) -> Vec<(&str, String)> {
    let mut sections: Vec<(&str, String)> = Vec::new();
    for line in text.lines() {
        if line.starts_with("## ") {
            sections.push((line, String::new()));
        } else if let Some((_, body)) = sections.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    sections
}

#[test]
fn the_page_answers_each_obligation_in_a_section_of_its_own() -> TestResult {
    let text = std::fs::read_to_string(PAGE)?;
    let sections = sections(&text);
    let headings: Vec<&str> = sections.iter().map(|(heading, _)| *heading).collect();
    let expected: Vec<&str> = OBLIGATIONS
        .iter()
        .map(|(heading, _, _)| *heading)
        .chain(["## The operator's template"])
        .collect();
    assert_eq!(expected, headings, "one section per obligation, in order");
    for ((heading, body), (_, anchor, _)) in sections.iter().zip(OBLIGATIONS) {
        assert!(
            body.contains(&format!("security.html#{anchor})")),
            "{heading} cites §13.4 {anchor}"
        );
        assert!(
            body.contains("**What changes the answer:**"),
            "{heading} names the configuration that changes the answer"
        );
    }
    Ok(())
}

#[test]
fn the_template_has_an_entry_for_each_obligation() -> TestResult {
    let text = std::fs::read_to_string(PAGE)?;
    let template = sections(&text)
        .into_iter()
        .find(|(heading, _)| *heading == "## The operator's template")
        .map(|(_, body)| body)
        .ok_or("the page carries the template")?;
    for (_, _, entry) in OBLIGATIONS {
        assert!(
            template.lines().any(|line| line == entry),
            "the template opens {entry}"
        );
    }
    Ok(())
}

#[test]
fn the_authentication_page_links_the_decisions() -> TestResult {
    let text = std::fs::read_to_string(AUTHENTICATION)?;
    assert!(
        text.contains("(deployment-decisions.md)"),
        "the client authentication page links the §13.4 decisions"
    );
    Ok(())
}
