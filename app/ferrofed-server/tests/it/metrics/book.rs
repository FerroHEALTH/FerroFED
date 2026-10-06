// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The `reason` row of the metrics page's label table held to the code: it
//! names exactly the reason of every [`Refusal`], the values
//! `ferrofed_security_events_total` carries on `caller-refused`. No
//! specification governs metrics: our own design.
#![allow(
    clippy::panic_in_result_fn,
    reason = "test assertions in tests that return their setup errors"
)]

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_server::auth::refusal::Refusal;

type TestResult = Result<(), Box<dyn Error>>;

/// The book page that documents the metrics.
const BOOK_PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/metrics.md"
);

/// The start of the label table's `reason` row.
const ROW: &str = "| `reason` |";

/// The words after which the row lists the values.
const LEAD: &str = "challenge names:";

/// The reason values the `reason` row lists, in backticks after [`LEAD`].
fn documented() -> Result<Vec<String>, Box<dyn Error>> {
    let page = std::fs::read_to_string(BOOK_PAGE)?;
    let row = page
        .lines()
        .find(|line| line.starts_with(ROW))
        .ok_or_else(|| format!("{BOOK_PAGE} has a row starting {ROW}"))?;
    let (_, values) = row
        .split_once(LEAD)
        .ok_or_else(|| format!("the reason row lists its values after {LEAD:?}: {row}"))?;
    Ok(values
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect())
}

#[test]
fn the_reason_row_names_exactly_the_reason_of_every_refusal() -> TestResult {
    let listed = documented()?;
    let documented: BTreeSet<&str> = listed.iter().map(String::as_str).collect();
    assert_eq!(listed.len(), documented.len(), "each reason is listed once");
    let emitted: BTreeSet<&str> = Refusal::ALL.into_iter().map(Refusal::reason).collect();
    assert_eq!(
        emitted, documented,
        "website/book/src/operate/metrics.md lists exactly the reasons auth::refusal emits"
    );
    Ok(())
}
