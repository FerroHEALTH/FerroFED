// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The reload table of the book's registry page held to the code: its
//! "Reloaded" column names exactly the sections [`reloadable`] returns, its
//! count says how many, and every section a binding marks for a restart is in
//! its "Needs a restart" column. The book documents the default build, which
//! compiles every binding. No specification governs reloading: our own
//! design.

use std::collections::BTreeSet;
use std::error::Error;

use ferrofed_server::binding::{Reload, compiled, reloadable};

type TestResult = Result<(), Box<dyn Error>>;

/// The book page that documents reloading.
const BOOK_PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/operate/registry.md"
);

/// The header row of the reload table.
const HEADER: &str = "| Reloaded | Needs a restart |";

/// The section keys a table cell names in backticks, brackets removed, in
/// the order written.
fn keys(cell: &str) -> Vec<String> {
    cell.split('`')
        .skip(1)
        .step_by(2)
        .map(|quoted| quoted.trim_matches(['[', ']']).to_owned())
        .collect()
}

/// The two columns of the reload table: the first section each "Reloaded"
/// cell names, and every section the "Needs a restart" cells name.
fn table(page: &str) -> Result<(Vec<String>, BTreeSet<String>), Box<dyn Error>> {
    let lines = page
        .lines()
        .skip_while(|line| line.trim() != HEADER)
        .skip(2);
    let mut reloaded = Vec::new();
    let mut restart = BTreeSet::new();
    for line in lines.take_while(|line| line.starts_with('|')) {
        let cells: Vec<&str> = line.split('|').collect();
        let [_, applies, restarts, _] = cells.as_slice() else {
            return Err(format!("a row of two cells: {line}").into());
        };
        if let Some(first) = keys(applies).into_iter().next() {
            reloaded.push(first);
        }
        restart.extend(keys(restarts));
    }
    if reloaded.is_empty() {
        return Err(format!("{BOOK_PAGE} has no table headed {HEADER}").into());
    }
    Ok((reloaded, restart))
}

/// The number the sentence before the table states, as a word.
fn stated_count(page: &str) -> Result<usize, Box<dyn Error>> {
    const WORDS: [&str; 13] = [
        "Zero", "One", "Two", "Three", "Four", "Five", "Six", "Seven", "Eight", "Nine", "Ten",
        "Eleven", "Twelve",
    ];
    let sentence = page
        .lines()
        .find(|line| line.ends_with("sections take effect on a reload:"))
        .ok_or("the page states how many sections a reload applies")?;
    let word = sentence.split_whitespace().next().unwrap_or_default();
    WORDS
        .iter()
        .position(|candidate| *candidate == word)
        .ok_or_else(|| format!("a count spelled as a word: {sentence}").into())
}

#[test]
fn the_reloaded_column_names_exactly_the_reloadable_sections() -> TestResult {
    let page = std::fs::read_to_string(BOOK_PAGE)?;
    let (reloaded, _) = table(&page)?;
    let documented: BTreeSet<&str> = reloaded.iter().map(String::as_str).collect();
    assert_eq!(
        reloaded.len(),
        documented.len(),
        "no row twice: {reloaded:?}"
    );
    let code: BTreeSet<&str> = reloadable().into_iter().collect();
    assert_eq!(code, documented, "{BOOK_PAGE}, the Reloaded column");
    assert_eq!(
        code.len(),
        stated_count(&page)?,
        "the count before the table"
    );
    Ok(())
}

#[test]
fn every_section_a_binding_restarts_for_is_in_the_restart_column() -> TestResult {
    let page = std::fs::read_to_string(BOOK_PAGE)?;
    let (_, restart) = table(&page)?;
    for binding in compiled() {
        for section in binding.sections() {
            assert_eq!(
                section.reload == Reload::Restart,
                restart.contains(section.key),
                "{}: {} in the Needs a restart column",
                binding.name(),
                section.key
            );
        }
    }
    Ok(())
}
