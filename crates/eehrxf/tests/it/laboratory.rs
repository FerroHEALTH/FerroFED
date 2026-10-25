// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The laboratory report crosswalk holds against the vendored Xt-EHR and
//! HL7 Europe Laboratory Report packages, over the composition and the
//! `DiagnosticReport` it names.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Category;
use eehrxf::crosswalk::Crosswalk;
use eehrxf::crosswalk::Defect;
use eehrxf::crosswalk::Populate;
use eehrxf::crosswalk::Row;
use eehrxf::crosswalk::laboratory::LAB_COMPOSITION;
use eehrxf::crosswalk::laboratory::LAB_DIAGNOSTIC_REPORT;
use eehrxf::crosswalk::laboratory::LABORATORY;

use crate::support::lab;
use crate::support::xtehr;

/// Checks `crosswalk` against the vendored packages it names.
fn check(crosswalk: &Crosswalk<'_>) -> Result<Vec<Defect>, Box<dyn Error>> {
    let model = xtehr()?;
    let profile = lab(crosswalk.profile)?;
    let companions = crosswalk
        .companions
        .iter()
        .map(|url| lab(url))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crosswalk.check_with(model.dataset(crosswalk.root)?, &profile, &companions))
}

/// The laboratory report rows with `edit` applied to each.
fn rows_with(edit: impl Fn(Row) -> Option<Row>) -> Vec<Row> {
    LABORATORY.rows.iter().copied().filter_map(edit).collect()
}

#[test]
fn the_laboratory_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    assert_eq!(check(&LABORATORY)?, []);
    Ok(())
}

#[test]
fn the_laboratory_category_carries_the_crosswalk() {
    let crosswalk = Category::Laboratory.crosswalks().first().copied();
    assert!(
        crosswalk.is_some_and(|crosswalk| crosswalk.root == LABORATORY.root
            && crosswalk.profile == LAB_COMPOSITION
            && crosswalk.companions == [LAB_DIAGNOSTIC_REPORT]),
        "the laboratory report names its crosswalk"
    );
    assert_eq!(Category::Laboratory.roots(), [LABORATORY.root]);
}

#[test]
fn every_populated_element_of_the_model_has_a_row() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    let dataset = model.dataset(LABORATORY.root)?;
    let uncovered: Vec<&str> = dataset
        .model()
        .elements()
        .iter()
        .map(|element| element.path().as_str())
        .filter(|path| {
            Populate::of(dataset.obligations(), path).is_some() && LABORATORY.row(path).is_none()
        })
        .collect();
    assert_eq!(uncovered, Vec::<&str>::new());
    Ok(())
}

#[test]
fn the_results_sit_in_the_diagnostic_report() -> Result<(), Box<dyn Error>> {
    let row = LABORATORY
        .row("EHDSLaboratoryReport.body.resultData.laboratoryTestResults")
        .ok_or("the crosswalk carries the test results")?;
    assert_eq!(row.eps, "DiagnosticReport.result");
    assert_eq!(
        row.contexts,
        ["http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab"]
    );
    Ok(())
}

#[test]
fn a_crosswalk_checked_without_its_companion_is_refused() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    let profile = lab(LABORATORY.profile)?;
    let defects = LABORATORY.check(model.dataset(LABORATORY.root)?, &profile);
    assert!(
        defects.contains(&Defect::Companions { urls: Vec::new() }),
        "{defects:?}"
    );
    assert!(
        defects.contains(&Defect::Slice {
            path: "EHDSLaboratoryReport.body.specimen",
            eps: "DiagnosticReport.specimen",
        }),
        "{defects:?}"
    );
    Ok(())
}

#[test]
fn a_row_dropped_for_a_shall_element_is_found() -> Result<(), Box<dyn Error>> {
    let path = "EHDSLaboratoryReport.body.specimen";
    let rows = rows_with(|row| (row.path != path).then_some(row));
    let defects = check(&Crosswalk {
        rows: &rows,
        ..LABORATORY
    })?;
    assert!(
        defects.contains(&Defect::Uncovered {
            path: String::from(path),
        }),
        "{defects:?}"
    );
    Ok(())
}

/// A context profile the specimen reference does not target.
const RESULT: &[&str] =
    &["http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab"];

#[test]
fn a_context_its_reference_does_not_target_is_found() -> Result<(), Box<dyn Error>> {
    let path = "EHDSLaboratoryReport.body.specimen";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                contexts: RESULT,
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..LABORATORY
    })?;
    assert!(
        defects.contains(&Defect::Context {
            path,
            eps: "DiagnosticReport.specimen",
            profile: "http://hl7.eu/fhir/laboratory/StructureDefinition/Observation-resultslab-eu-lab",
        }),
        "{defects:?}"
    );
    Ok(())
}
