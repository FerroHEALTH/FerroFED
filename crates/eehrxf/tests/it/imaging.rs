// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The imaging report and imaging study crosswalks hold against the
//! vendored Xt-EHR and HL7 Europe Imaging Report packages.

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
use eehrxf::crosswalk::imaging::IMAGING_COMPOSITION;
use eehrxf::crosswalk::imaging::IMAGING_DIAGNOSTIC_REPORT;
use eehrxf::crosswalk::imaging::IMAGING_REPORT;
use eehrxf::crosswalk::imaging::IMAGING_STUDY;
use eehrxf::crosswalk::imaging::IMAGING_STUDY_PROFILE;

use crate::support::imaging;
use crate::support::xtehr;

/// Checks `crosswalk` against the vendored packages it names.
fn check(crosswalk: &Crosswalk<'_>) -> Result<Vec<Defect>, Box<dyn Error>> {
    let model = xtehr()?;
    let profile = imaging(crosswalk.profile)?;
    let companions = crosswalk
        .companions
        .iter()
        .map(|url| imaging(url))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(crosswalk.check_with(model.dataset(crosswalk.root)?, &profile, &companions))
}

/// The elements of `crosswalk`'s model a producer shall or should be able to
/// populate that no row carries.
fn uncovered(crosswalk: &Crosswalk<'_>) -> Result<Vec<String>, Box<dyn Error>> {
    let model = xtehr()?;
    let dataset = model.dataset(crosswalk.root)?;
    Ok(dataset
        .model()
        .elements()
        .iter()
        .map(|element| element.path().as_str())
        .filter(|path| {
            Populate::of(dataset.obligations(), path).is_some() && crosswalk.row(path).is_none()
        })
        .map(str::to_owned)
        .collect())
}

#[test]
fn the_imaging_report_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    assert_eq!(check(&IMAGING_REPORT)?, []);
    Ok(())
}

#[test]
fn the_imaging_study_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>> {
    assert_eq!(check(&IMAGING_STUDY)?, []);
    Ok(())
}

#[test]
fn the_imaging_category_carries_both_crosswalks_in_root_order() {
    let crosswalks = Category::Imaging.crosswalks();
    let roots: Vec<_> = crosswalks.iter().map(|crosswalk| crosswalk.root).collect();
    assert_eq!(roots, Category::Imaging.roots());
    let [report, study] = crosswalks else {
        panic!("imaging carries two crosswalks");
    };
    assert_eq!(report.profile, IMAGING_COMPOSITION);
    assert_eq!(report.companions, [IMAGING_DIAGNOSTIC_REPORT]);
    assert_eq!(study.profile, IMAGING_STUDY_PROFILE);
}

#[test]
fn every_populated_element_of_both_models_has_a_row() -> Result<(), Box<dyn Error>> {
    assert_eq!(uncovered(&IMAGING_REPORT)?, Vec::<String>::new());
    assert_eq!(uncovered(&IMAGING_STUDY)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn a_required_section_named_by_no_row_is_found() -> Result<(), Box<dyn Error>> {
    let rows: Vec<Row> = IMAGING_REPORT
        .rows
        .iter()
        .copied()
        .filter(|row| !row.eps.starts_with("Composition.section:order"))
        .collect();
    let defects = check(&Crosswalk {
        rows: &rows,
        ..IMAGING_REPORT
    })?;
    assert!(
        defects.contains(&Defect::RequiredSlice {
            id: String::from("Composition.section:order"),
        }),
        "{defects:?}"
    );
    Ok(())
}

#[test]
fn the_study_instance_uid_is_the_required_identifier_slice() -> Result<(), Box<dyn Error>> {
    let row = IMAGING_STUDY
        .row("EHDSImagingStudy.header.identifier")
        .ok_or("the study crosswalk carries the identifier")?;
    assert_eq!(row.eps, "ImagingStudy.identifier:studyInstanceUid");
    let rows: Vec<Row> = IMAGING_STUDY
        .rows
        .iter()
        .copied()
        .filter(|row| row.path != "EHDSImagingStudy.header.identifier")
        .collect();
    let defects = check(&Crosswalk {
        rows: &rows,
        ..IMAGING_STUDY
    })?;
    assert!(
        defects.contains(&Defect::RequiredSlice {
            id: String::from("ImagingStudy.identifier:studyInstanceUid"),
        }),
        "{defects:?}"
    );
    Ok(())
}
