// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The information sheet's table of the categories received, held to the
//! categories this build receives documents of (Regulation (EU) 2025/327
//! Art 38(1), Annex II 2.2 and 2.3): a category the sheet calls received
//! has a document profile here, and one with a profile is called received.

use std::error::Error;

use eehrxf::category::Category;

type TestResult = Result<(), Box<dyn Error>>;

/// The information sheet.
const SHEET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../website/book/src/evaluate/information-sheet.md"
);

/// The heading of the table's section.
const HEADING: &str = "### The categories received";

/// The six codes of Art 14(1) in HL7 Europe's
/// `EEHRxFDocumentPriorityCategoryCS`, in the order of the article.
const CODES: [&str; 6] = [
    "Patient-Summaries",
    "Electronic-Prescriptions",
    "Electronic-Dispensations",
    "Medical-Imaging",
    "Laboratory-Reports",
    "Discharge-Reports",
];

/// Every `(code, received)` row of the sheet's table, in its order.
fn rows(sheet: &str) -> Result<Vec<(String, bool)>, Box<dyn Error>> {
    let (_, section) = sheet.split_once(HEADING).ok_or("the section")?;
    let mut rows = Vec::new();
    for line in section.lines().skip_while(|line| !line.starts_with('|')) {
        if !line.starts_with('|') {
            break;
        }
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(code), Some(received)) = (cells.get(2), cells.get(3)) else {
            return Err(format!("a row of four cells: {line}").into());
        };
        let Some(code) = code
            .strip_prefix('`')
            .and_then(|code| code.strip_suffix('`'))
        else {
            continue;
        };
        let received = match *received {
            "yes" => true,
            "no" => false,
            other => return Err(format!("{code}: yes or no, not {other}").into()),
        };
        rows.push((code.to_owned(), received));
    }
    Ok(rows)
}

#[test]
fn the_sheet_names_every_category_once_in_the_order_of_art_14() -> TestResult {
    let rows = rows(&std::fs::read_to_string(SHEET)?)?;
    let codes: Vec<&str> = rows.iter().map(|(code, _)| code.as_str()).collect();
    assert_eq!(codes, CODES);
    Ok(())
}

#[test]
fn the_sheet_calls_received_exactly_the_categories_this_build_receives() -> TestResult {
    let rows = rows(&std::fs::read_to_string(SHEET)?)?;
    for (code, received) in &rows {
        let receivable = Category::of_code(code).is_some_and(|category| {
            category.profiles().is_some() && category.document_type().is_some()
        });
        assert_eq!(
            *received, receivable,
            "{code}: the sheet and the build disagree"
        );
    }
    for category in Category::ENABLED {
        if category.profiles().is_some() {
            assert!(
                rows.iter()
                    .any(|(code, received)| code == category.code() && *received),
                "{}: received, but not on the sheet",
                category.code()
            );
        }
    }
    Ok(())
}
