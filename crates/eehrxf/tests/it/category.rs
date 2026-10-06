// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! Every category this build carries resolves to a dataset of the vendored
//! Xt-EHR package, with the elements its producer must be able to populate.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::Category;

use super::support::xtehr;

#[test]
fn every_enabled_category_resolves_to_a_dataset_of_the_package() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    for category in Category::ENABLED {
        assert!(!category.roots().is_empty(), "{category:?} names a dataset");
        for root in category.roots() {
            let dataset = model.dataset(*root)?;
            assert_eq!(dataset.model().url(), root.model);
            assert_eq!(dataset.obligations().constrains(), root.model);
            let populated = dataset.producer_elements().count();
            assert!(
                populated > 0,
                "{category:?}: {} names elements a producer must populate",
                dataset.obligations().name()
            );
        }
    }
    Ok(())
}

#[cfg(feature = "patient-summary")]
#[test]
fn the_patient_summary_producer_populates_its_five_required_sections() -> Result<(), Box<dyn Error>>
{
    // EHDSPatientSummary: alerts and the rest are optional; allergies,
    // problems, medication summary, devices and procedures are 1..1 and
    // SHALL:able-to-populate for the producer (27 elements in all).
    let model = xtehr()?;
    let [root] = Category::PatientSummary.roots() else {
        return Err("the patient summary names one dataset".into());
    };
    let dataset = model.dataset(*root)?;
    let populated: Vec<&str> = dataset
        .producer_elements()
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(populated.len(), 27, "{populated:?}");
    let required_sections: Vec<&str> = dataset
        .model()
        .elements()
        .iter()
        .filter(|element| {
            element.types() == ["Base"]
                && element.cardinality().is_required()
                && element
                    .path()
                    .parent()
                    .is_some_and(|parent| parent.as_str() == "EHDSPatientSummary")
                && element.path().as_str() != "EHDSPatientSummary.header"
        })
        .map(|element| element.path().as_str())
        .collect();
    assert_eq!(
        required_sections,
        [
            "EHDSPatientSummary.allergiesAndIntolerances",
            "EHDSPatientSummary.problems",
            "EHDSPatientSummary.medicationSummary",
            "EHDSPatientSummary.medicalDevicesAndImplants",
            "EHDSPatientSummary.procedures",
        ]
    );
    for section in &required_sections {
        assert!(
            populated.contains(section),
            "{section} is producer-populated"
        );
    }
    Ok(())
}

#[cfg(feature = "imaging")]
#[test]
fn imaging_carries_the_report_and_the_study() {
    let models: Vec<&str> = Category::Imaging
        .roots()
        .iter()
        .map(|root| root.model)
        .collect();
    assert_eq!(
        models,
        [
            "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingReport",
            "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSImagingStudy",
        ]
    );
}

#[test]
fn the_enabled_set_follows_the_features() {
    let expected = [
        cfg!(feature = "patient-summary"),
        cfg!(feature = "prescription"),
        cfg!(feature = "dispensation"),
        cfg!(feature = "imaging"),
        cfg!(feature = "laboratory"),
        cfg!(feature = "discharge"),
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    assert_eq!(Category::ENABLED.len(), expected);
}
