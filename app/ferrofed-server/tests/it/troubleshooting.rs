// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The error-code table of the book's troubleshooting page held to the
//! code: one row for every code of [`Code::GATEWAY`] with its status, and
//! none for a code the gateway does not answer, each row with its cause, how
//! to confirm it, and a fix that links the page explaining it. No
//! specification governs troubleshooting: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

use ferrofed_server::error::Code;

type TestResult = Result<(), Box<dyn Error>>;

/// The book page that leads from a symptom to its cause.
const BOOK_PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/troubleshooting.md"
);

/// The heading of the section that holds the code table.
const SECTION: &str = "## Error codes";

/// The header row of the code table.
const HEADER: &str = "| Code | Status | Cause | Confirm | Fix |";

/// One row of the code table: the status and the cells after it.
#[derive(Debug)]
struct Row {
    /// The status cell.
    status: String,
    /// The cause, confirm and fix cells.
    rest: Vec<String>,
}

/// Every row of the code table, by the code its first cell names.
fn rows() -> Result<BTreeMap<String, Row>, Box<dyn Error>> {
    let page = std::fs::read_to_string(BOOK_PAGE)?;
    let section: Vec<&str> = page
        .lines()
        .skip_while(|line| line.trim() != SECTION)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .collect();
    if section.is_empty() {
        return Err(format!("{BOOK_PAGE} has no section headed {SECTION}").into());
    }
    let table = section
        .iter()
        .skip_while(|line| line.trim() != HEADER)
        .skip(2)
        .take_while(|line| line.starts_with('|'));
    let mut rows = BTreeMap::new();
    for line in table {
        let cells: Vec<String> = line.split('|').map(|cell| cell.trim().to_owned()).collect();
        let [_, code, status, cause, confirm, fix, _] = cells.as_slice() else {
            return Err(format!("a row of five cells: {line}").into());
        };
        let code = code
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
            .ok_or_else(|| format!("a code in backticks: {line}"))?;
        let row = Row {
            status: status.clone(),
            rest: vec![cause.clone(), confirm.clone(), fix.clone()],
        };
        if rows.insert(code.to_owned(), row).is_some() {
            return Err(format!("{code} has one row").into());
        }
    }
    if rows.is_empty() {
        return Err(format!("{BOOK_PAGE} has no table headed {HEADER}").into());
    }
    Ok(rows)
}

#[test]
fn the_page_has_a_row_for_every_gateway_code_with_its_status() -> TestResult {
    let rows = rows()?;
    let answered: BTreeMap<&str, String> = Code::GATEWAY
        .into_iter()
        .map(|code| (code.as_str(), code.status().as_str().to_owned()))
        .collect();
    let documented: BTreeSet<&str> = rows.keys().map(String::as_str).collect();
    assert_eq!(
        answered.keys().copied().collect::<BTreeSet<_>>(),
        documented,
        "website/book/src/operate/troubleshooting.md has a row for exactly the codes of Code::GATEWAY"
    );
    for (code, status) in answered {
        let row = rows.get(code).ok_or("every code has a row")?;
        assert_eq!(status, row.status, "the row of {code} names its status");
    }
    Ok(())
}

#[test]
fn every_row_names_a_cause_a_confirmation_and_a_linked_fix() -> TestResult {
    for (code, row) in rows()? {
        assert!(
            row.rest.iter().all(|cell| !cell.is_empty()),
            "the row of {code} fills every cell: {row:?}"
        );
        let fix = row.rest.last().ok_or("a row has a fix cell")?;
        assert!(
            fix.contains("]("),
            "the fix of {code} links the page that explains it: {fix}"
        );
    }
    Ok(())
}
