// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The dataset model read from the vendored Xt-EHR *EHDS Logical Information
//! Models* 1.0.0 package.

#![expect(
    clippy::panic_in_result_fn,
    reason = "a test asserts, and returns its setup errors"
)]

use std::error::Error;

use eehrxf::category::XTEHR_CANONICAL;
use eehrxf::dataset::ABLE_TO_POPULATE;
use eehrxf::dataset::ElementPath;
use eehrxf::dataset::Max;
use eehrxf::dataset::PRODUCER;

use super::support::xtehr;

#[test]
fn the_package_is_the_pinned_one() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    assert_eq!(model.package().name(), "xtehr.eu.ehds.models");
    assert_eq!(model.package().version(), "1.0.0");
    assert_eq!(model.package().to_string(), "xtehr.eu.ehds.models#1.0.0");
    Ok(())
}

#[test]
fn every_logical_definition_is_read() -> Result<(), Box<dyn Error>> {
    // The package holds 42 logical specializations and 40 obligations
    // profiles over them, and nothing else of kind logical.
    let model = xtehr()?;
    assert_eq!(model.models().count(), 42, "the logical models");
    assert_eq!(model.profiles().count(), 40, "the obligations profiles");
    for logical in model.models() {
        assert!(
            logical.url().starts_with(XTEHR_CANONICAL),
            "{} sits under the package canonical",
            logical.url()
        );
    }
    Ok(())
}

#[test]
fn every_element_path_starts_at_its_model_and_hangs_under_an_element() -> Result<(), Box<dyn Error>>
{
    let model = xtehr()?;
    for logical in model.models() {
        let root = logical
            .elements()
            .first()
            .ok_or_else(|| format!("{} has no elements", logical.name()))?;
        assert_eq!(
            root.path().as_str(),
            logical.name(),
            "the first element is the root"
        );
        assert_eq!(root.path().parent(), None, "the root has no parent");
        for element in logical.elements().iter().skip(1) {
            assert_eq!(element.path().model(), logical.name(), "{}", element.path());
            let parent = element
                .path()
                .parent()
                .ok_or_else(|| format!("{} has no parent", element.path()))?;
            assert!(
                logical.element(parent.as_str()).is_some(),
                "{} hangs under {parent}, which {} lacks",
                element.path(),
                logical.name()
            );
            assert_eq!(
                logical.element(element.path().as_str()),
                Some(element),
                "{} is found by its own path",
                element.path()
            );
        }
    }
    Ok(())
}

#[test]
fn every_profile_constrains_a_model_of_the_package() -> Result<(), Box<dyn Error>> {
    let model = xtehr()?;
    for profile in model.profiles() {
        let base = model
            .model(profile.constrains())
            .ok_or_else(|| format!("{} constrains a missing model", profile.url()))?;
        for element in base.elements() {
            assert!(
                profile.element(element.path().as_str()).is_some(),
                "{} drops {} of {}",
                profile.name(),
                element.path(),
                base.name()
            );
        }
        for path in profile.obligations().keys() {
            assert!(
                profile.element(path.as_str()).is_some(),
                "{} obliges {path}, which its snapshot lacks",
                profile.name()
            );
        }
    }
    Ok(())
}

#[test]
fn the_patient_summary_allergy_section_is_required_and_producer_populated()
-> Result<(), Box<dyn Error>> {
    // EHDSPatientSummary: `allergiesAndIntolerances` 1..1, its list 0..*, and
    // EHDSPatientSummaryObligations puts SHALL:able-to-populate for the
    // producer on both.
    let model = xtehr()?;
    let summary = model
        .model("http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummary")
        .ok_or("the patient summary model")?;
    let section = summary
        .element("EHDSPatientSummary.allergiesAndIntolerances")
        .ok_or("the allergy section")?;
    assert_eq!(section.cardinality().to_string(), "1..1");
    assert!(
        section.cardinality().is_required(),
        "the section is required"
    );
    assert_eq!(section.types(), ["Base"], "a section is a group");
    assert_eq!(section.short(), Some("Section: Allergies and intolerances"));
    let list = summary
        .element("EHDSPatientSummary.allergiesAndIntolerances.allergyIntolerance")
        .ok_or("the allergy list")?;
    assert_eq!(list.cardinality().upper(), Max::Unbounded);
    assert_eq!(
        list.types(),
        ["http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSAllergyIntolerance"],
        "the list holds the allergy building block"
    );
    let profile = model
        .profile(
            "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSPatientSummaryObligations",
        )
        .ok_or("the patient summary obligations")?;
    let populated: Vec<&str> = profile
        .paths_with(ABLE_TO_POPULATE, PRODUCER)
        .map(ElementPath::as_str)
        .collect();
    assert!(
        populated.contains(&"EHDSPatientSummary.allergiesAndIntolerances"),
        "{populated:?}"
    );
    assert!(
        populated.contains(&"EHDSPatientSummary.allergiesAndIntolerances.allergyIntolerance"),
        "{populated:?}"
    );
    assert!(
        !populated.contains(&"EHDSPatientSummary.alerts"),
        "alerts are SHOULD, not SHALL: {populated:?}"
    );
    Ok(())
}

#[test]
fn a_profile_slices_a_choice_element_its_model_leaves_whole() -> Result<(), Box<dyn Error>> {
    // EHDSLaboratoryObservationObligations slices `result.value[x]` by type
    // and obliges each slice; the slice is keyed on its id, which differs
    // from its path, and the model itself holds only `value[x]`.
    let model = xtehr()?;
    let observation = model
        .model("http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryObservation")
        .ok_or("the laboratory observation model")?;
    let profile = model
        .profile(
            "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSLaboratoryObservationObligations",
        )
        .ok_or("the laboratory observation obligations")?;
    let slice = "EHDSLaboratoryObservation.result.value[x]:valueQuantity";
    assert!(
        observation
            .element("EHDSLaboratoryObservation.result.value[x]")
            .is_some()
    );
    assert!(
        observation.element(slice).is_none(),
        "the model leaves the choice whole"
    );
    let sliced = profile.element(slice).ok_or("the quantity slice")?;
    assert_eq!(sliced.path().model(), "EHDSLaboratoryObservation");
    assert!(
        profile.obligations().contains_key(sliced.path()),
        "the slice is obliged"
    );
    Ok(())
}

#[test]
fn a_profile_reaches_into_the_logical_type_an_element_names() -> Result<(), Box<dyn Error>> {
    // EHDSHealthProfessional.name is an EHDSHumanName; the obligations
    // profile expands it and obliges the producer to populate `name.family`.
    let model = xtehr()?;
    let professional = model
        .model("http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSHealthProfessional")
        .ok_or("the health professional model")?;
    let name = professional
        .element("EHDSHealthProfessional.name")
        .ok_or("the name element")?;
    assert_eq!(
        name.types(),
        ["http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSHumanName"]
    );
    assert!(
        professional
            .element("EHDSHealthProfessional.name.family")
            .is_none()
    );
    let profile = model
        .profile(
            "http://www.xt-ehr.eu/fhir/models/StructureDefinition/EHDSHealthProfessionalObligations",
        )
        .ok_or("the health professional obligations")?;
    let populated: Vec<&str> = profile
        .paths_with(ABLE_TO_POPULATE, PRODUCER)
        .map(ElementPath::as_str)
        .collect();
    assert!(
        populated.contains(&"EHDSHealthProfessional.name.family"),
        "{populated:?}"
    );
    Ok(())
}
