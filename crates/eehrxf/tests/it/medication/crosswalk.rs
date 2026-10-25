// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The prescription and dispensation crosswalks hold against the vendored
//! Xt-EHR and HL7 Europe MPD packages.

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

use crate::support::mpd;
use crate::support::xtehr;

/// Checks `crosswalk` against the vendored packages it names.
fn check(crosswalk: &Crosswalk<'_>) -> Result<Vec<Defect>, Box<dyn Error>> {
    let model = xtehr()?;
    let profile = mpd(crosswalk.profile)?;
    Ok(crosswalk.check(model.dataset(crosswalk.root)?, &profile))
}

/// The elements of `crosswalk`'s model a producer shall or should be able
/// to populate that no row carries, in path order.
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

/// `crosswalk` with its rows replaced by `rows`.
fn with_rows<'a>(crosswalk: &Crosswalk<'a>, rows: &'a [Row]) -> Crosswalk<'a> {
    Crosswalk { rows, ..*crosswalk }
}

#[cfg(feature = "prescription")]
mod prescription {
    use eehrxf::crosswalk::prescription::MPD_MEDICATION_REQUEST;
    use eehrxf::crosswalk::prescription::PRESCRIPTION;

    use super::*;

    #[test]
    fn the_prescription_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>>
    {
        assert_eq!(check(&PRESCRIPTION)?, []);
        Ok(())
    }

    #[test]
    fn the_prescription_category_carries_the_crosswalk() {
        let crosswalk = Category::Prescription.crosswalks().first().copied();
        assert!(
            crosswalk.is_some_and(|crosswalk| crosswalk.root == PRESCRIPTION.root
                && crosswalk.profile == MPD_MEDICATION_REQUEST),
            "the prescription names its crosswalk"
        );
        assert_eq!(Category::Prescription.roots(), [PRESCRIPTION.root]);
    }

    #[test]
    fn every_populated_element_has_a_row_but_the_presented_form() -> Result<(), Box<dyn Error>> {
        // The presented form, the prescription as issued, is carried by no
        // element of MedicationRequest-eu-mpd.
        assert_eq!(
            uncovered(&PRESCRIPTION)?,
            ["EHDSMedicationPrescription.presentedForm"]
        );
        Ok(())
    }

    #[test]
    fn a_row_dropped_for_a_shall_element_is_found() -> Result<(), Box<dyn Error>> {
        let rows: Vec<Row> = PRESCRIPTION
            .rows
            .iter()
            .copied()
            .filter(|row| row.path != "EHDSMedicationPrescription.header.identifier")
            .collect();
        let defects = check(&with_rows(&PRESCRIPTION, &rows))?;
        assert!(
            defects.contains(&Defect::Uncovered {
                path: String::from("EHDSMedicationPrescription.header.identifier"),
            }),
            "{defects:?}"
        );
        Ok(())
    }

    #[test]
    fn a_row_naming_an_element_the_profile_lacks_is_found() -> Result<(), Box<dyn Error>> {
        let rows: Vec<Row> = PRESCRIPTION
            .rows
            .iter()
            .copied()
            .map(|row| {
                if row.path == "EHDSMedicationPrescription.prescriptionItem.numberOfRepeats" {
                    Row {
                        eps: "MedicationRequest.dispenseRequest.remainingRepeats",
                        ..row
                    }
                } else {
                    row
                }
            })
            .collect();
        let defects = check(&with_rows(&PRESCRIPTION, &rows))?;
        assert!(
            defects.contains(&Defect::Slice {
                path: "EHDSMedicationPrescription.prescriptionItem.numberOfRepeats",
                eps: "MedicationRequest.dispenseRequest.remainingRepeats",
            }),
            "{defects:?}"
        );
        Ok(())
    }
}

#[cfg(feature = "dispensation")]
mod dispensation {
    use eehrxf::crosswalk::dispensation::DISPENSATION;
    use eehrxf::crosswalk::dispensation::MPD_MEDICATION_DISPENSE;

    use super::*;

    #[test]
    fn the_dispensation_crosswalk_holds_against_the_pinned_packages() -> Result<(), Box<dyn Error>>
    {
        assert_eq!(check(&DISPENSATION)?, []);
        Ok(())
    }

    #[test]
    fn the_dispensation_category_carries_the_crosswalk() {
        let crosswalk = Category::Dispensation.crosswalks().first().copied();
        assert!(
            crosswalk.is_some_and(|crosswalk| crosswalk.root == DISPENSATION.root
                && crosswalk.profile == MPD_MEDICATION_DISPENSE),
            "the dispensation names its crosswalk"
        );
        assert_eq!(Category::Dispensation.roots(), [DISPENSATION.root]);
    }

    #[test]
    fn every_populated_element_has_a_row() -> Result<(), Box<dyn Error>> {
        assert_eq!(uncovered(&DISPENSATION)?, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn the_required_recorded_slice_must_be_named() -> Result<(), Box<dyn Error>> {
        // MedicationDispense-eu-mpd requires extension:recorded (1..1), which
        // carries header.date.
        let rows: Vec<Row> = DISPENSATION
            .rows
            .iter()
            .copied()
            .filter(|row| row.path != "EHDSMedicationDispense.header.date")
            .collect();
        let defects = check(&with_rows(&DISPENSATION, &rows))?;
        assert!(
            defects.contains(&Defect::RequiredSlice {
                id: String::from("MedicationDispense.extension:recorded"),
            }),
            "{defects:?}"
        );
        Ok(())
    }
}
