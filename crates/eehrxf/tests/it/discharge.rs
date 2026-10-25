// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The discharge report crosswalk holds against the vendored Xt-EHR and
//! HL7 Europe Hospital Discharge Report packages.

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
use eehrxf::crosswalk::discharge::DISCHARGE;
use eehrxf::crosswalk::discharge::HDR_COMPOSITION;

use crate::support::hdr;
use crate::support::xtehr;

/// Checks `crosswalk` against the vendored packages it names.
fn check(crosswalk: &Crosswalk<'_>) -> Result<Vec<Defect>, Box<dyn Error>> {
    let model = xtehr()?;
    let profile = hdr(crosswalk.profile)?;
    Ok(crosswalk.check(model.dataset(crosswalk.root)?, &profile))
}

/// The discharge report rows with `edit` applied to each.
fn rows_with(edit: impl Fn(Row) -> Option<Row>) -> Vec<Row> {
    DISCHARGE.rows.iter().copied().filter_map(edit).collect()
}

#[test]
fn the_discharge_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    assert_eq!(check(&DISCHARGE)?, []);
    Ok(())
}

#[test]
fn the_discharge_category_carries_the_crosswalk() {
    let crosswalk = Category::Discharge.crosswalks().first().copied();
    assert!(
        crosswalk
            .is_some_and(|crosswalk| crosswalk.root == DISCHARGE.root
                && crosswalk.profile == HDR_COMPOSITION),
        "the discharge report names its crosswalk"
    );
    assert_eq!(Category::Discharge.roots(), [DISCHARGE.root]);
}

#[test]
fn every_populated_element_of_the_model_has_a_row() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    let dataset = model.dataset(DISCHARGE.root)?;
    let uncovered: Vec<&str> = dataset
        .model()
        .elements()
        .iter()
        .map(|element| element.path().as_str())
        .filter(|path| {
            Populate::of(dataset.obligations(), path).is_some() && DISCHARGE.row(path).is_none()
        })
        .collect();
    assert_eq!(uncovered, Vec::<&str>::new());
    Ok(())
}

#[test]
fn a_row_dropped_for_a_shall_element_is_found() -> Result<(), Box<dyn Error>> {
    let path = "EHDSDischargeReport.body.medicationSummary.medicationUse";
    let rows = rows_with(|row| (row.path != path).then_some(row));
    let defects = check(&Crosswalk {
        rows: &rows,
        ..DISCHARGE
    })?;
    assert!(
        defects.contains(&Defect::Uncovered {
            path: String::from(path),
        }),
        "{defects:?}"
    );
    Ok(())
}

/// A context profile the diagnostic summary's section does not target.
const PROCEDURE: &[&str] = &["http://hl7.eu/fhir/base/StructureDefinition/procedure-eu-core"];

#[test]
fn a_context_its_section_does_not_target_is_found() -> Result<(), Box<dyn Error>> {
    let path = "EHDSDischargeReport.body.courseOfEncounter.diagnosticSummary";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                contexts: PROCEDURE,
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..DISCHARGE
    })?;
    assert!(
        defects.contains(&Defect::Context {
            path,
            eps: "Composition.section:sectionDiagnosticSummary.entry",
            profile: "http://hl7.eu/fhir/base/StructureDefinition/procedure-eu-core",
        }),
        "{defects:?}"
    );
    Ok(())
}

#[test]
fn a_section_whose_entries_target_any_resource_admits_its_profiles() -> Result<(), Box<dyn Error>> {
    let row = DISCHARGE
        .row("EHDSDischargeReport.body.admissionEvaluation.objectiveFindings")
        .ok_or("the crosswalk carries the admission findings")?;
    assert_eq!(
        row.eps,
        "Composition.section:sectionAdmissionEvaluation.entry"
    );
    let none: &[&str] = &[];
    assert_ne!(row.contexts, none);
    assert_eq!(check(&DISCHARGE)?, []);
    Ok(())
}

#[test]
fn a_row_stating_another_obligation_is_found() -> Result<(), Box<dyn Error>> {
    let path = "EHDSDischargeReport.body.synthesis";
    let rows = rows_with(|row| {
        Some(if row.path == path {
            Row {
                producer: Some(Populate::Shall),
                ..row
            }
        } else {
            row
        })
    });
    let defects = check(&Crosswalk {
        rows: &rows,
        ..DISCHARGE
    })?;
    assert!(
        defects.contains(&Defect::Obligation {
            path,
            row: Some(Populate::Shall),
            package: Some(Populate::Should),
        }),
        "{defects:?}"
    );
    Ok(())
}
